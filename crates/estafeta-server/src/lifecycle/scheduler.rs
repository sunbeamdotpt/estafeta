use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::db;
use crate::nats::{NatsPublisher, RealtimeEvent, RealtimeEventType};

const BATCH_SIZE: i64 = 100;

/// Background scheduler that handles snooze wake-ups, TTL expiry, and escalation.
/// Safe to run on every instance — uses SELECT FOR UPDATE SKIP LOCKED.
pub struct Scheduler {
    pool: PgPool,
    publisher: NatsPublisher,
}

impl Scheduler {
    /// Create a new scheduler with the given database pool and NATS publisher.
    pub fn new(pool: PgPool, publisher: NatsPublisher) -> Self {
        Self { pool, publisher }
    }

    /// Spawn all scheduler loops as background tasks.
    pub fn spawn(self) {
        let pool = self.pool.clone();
        let publisher = self.publisher.clone();

        // Snooze wake-up: every 30 seconds
        {
            let pool = pool.clone();
            let publisher = publisher.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    if let Err(e) = run_snooze_wake(&pool, &publisher).await {
                        error!(error = %e, "snooze wake-up failed");
                    }
                }
            });
        }

        // TTL expiry: every 60 seconds
        {
            let pool = pool.clone();
            let publisher = publisher.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    if let Err(e) = run_ttl_expiry(&pool, &publisher).await {
                        error!(error = %e, "TTL expiry failed");
                    }
                }
            });
        }

        // Escalation: every 60 seconds
        {
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    if let Err(e) = run_escalation(&pool, &publisher).await {
                        error!(error = %e, "escalation failed");
                    }
                }
            });
        }
    }
}

async fn run_snooze_wake(pool: &PgPool, publisher: &NatsPublisher) -> Result<()> {
    let woken = db::notifications::wake_snoozed(pool, BATCH_SIZE).await?;
    if !woken.is_empty() {
        info!(count = woken.len(), "woke snoozed notifications");
    }
    for notif in &woken {
        publisher
            .publish_state_change(
                &notif.recipient_user_id,
                &RealtimeEvent {
                    event_type: RealtimeEventType::StateChange,
                    notification_id: notif.id,
                    service_slug: String::new(),
                    notification_type: String::new(),
                    level: None,
                    payload: None,
                    group_key: None,
                    metadata: None,
                    old_state: Some("snoozed".into()),
                    new_state: Some("unread".into()),
                    unseen_count: None,
                    action_url: None,
                    icon: None,
                },
            )
            .await?;
    }
    Ok(())
}

async fn run_ttl_expiry(pool: &PgPool, publisher: &NatsPublisher) -> Result<()> {
    let expired = db::notifications::expire_notifications(pool, BATCH_SIZE).await?;
    if !expired.is_empty() {
        info!(count = expired.len(), "expired notifications");
    }
    for notif in &expired {
        publisher
            .publish_state_change(
                &notif.recipient_user_id,
                &RealtimeEvent {
                    event_type: RealtimeEventType::StateChange,
                    notification_id: notif.id,
                    service_slug: String::new(),
                    notification_type: String::new(),
                    level: None,
                    payload: None,
                    group_key: None,
                    metadata: None,
                    old_state: None, // could be unseen, unread or read
                    new_state: Some("expired".into()),
                    unseen_count: None,
                    action_url: None,
                    icon: None,
                },
            )
            .await?;
    }
    Ok(())
}

async fn run_escalation(pool: &PgPool, _publisher: &NatsPublisher) -> Result<()> {
    let due = db::notifications::get_escalation_due(pool, BATCH_SIZE).await?;
    if !due.is_empty() {
        info!(count = due.len(), "escalating notifications");
    }
    for notif in &due {
        let new_count = notif.escalation_count + 1;

        // Look up the notification type to get escalation config
        let notif_type = db::schemas::get_notification_type_by_id(
            pool,
            notif.notification_type_id,
        )
        .await?;

        // Compute next escalation time
        let next_escalation_at = if let Some(nt) = &notif_type {
            if new_count < nt.max_escalations {
                nt.escalation_interval_seconds
                    .map(|s| Utc::now() + Duration::seconds(s as i64))
            } else {
                None // max escalations reached
            }
        } else {
            None
        };

        db::notifications::update_escalation(pool, notif.id, new_count, next_escalation_at).await?;

        // Perform escalation action based on the notification type's config
        let action = notif_type
            .as_ref()
            .map(|nt| nt.escalation_action.as_str())
            .unwrap_or("resurface");

        match action {
            "resurface" => {
                // Set state back to unseen, clear seen_at
                db::notifications::resurface_notification(pool, notif.id).await?;
            }
            "bump" => {
                // Update updated_at to now, bringing it to the top
                db::notifications::bump_notification(pool, notif.id).await?;
            }
            "elevate" => {
                // TODO: implement level elevation
                warn!(
                    notification_id = %notif.id,
                    "escalation_action=elevate is not yet implemented, treating as resurface"
                );
                db::notifications::resurface_notification(pool, notif.id).await?;
            }
            _ => {
                warn!(
                    notification_id = %notif.id,
                    action = %action,
                    "unknown escalation action, treating as resurface"
                );
                db::notifications::resurface_notification(pool, notif.id).await?;
            }
        }
    }
    Ok(())
}
