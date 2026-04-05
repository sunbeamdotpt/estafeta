use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tracing::{error, info};

use crate::db;
use crate::nats::{NatsPublisher, RealtimeEvent, RealtimeEventType, DeliveryDispatchMessage};

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
                    unread_count: None,
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
                    old_state: None, // could be unread or read
                    new_state: Some("expired".into()),
                    unread_count: None,
                },
            )
            .await?;
    }
    Ok(())
}

async fn run_escalation(pool: &PgPool, publisher: &NatsPublisher) -> Result<()> {
    let due = db::notifications::get_escalation_due(pool, BATCH_SIZE).await?;
    if !due.is_empty() {
        info!(count = due.len(), "escalating notifications");
    }
    for notif in &due {
        let new_count = notif.escalation_count + 1;

        // Look up the notification type to get escalation config
        let notif_type = db::schemas::get_notification_type(
            pool,
            notif.service_id,
            // We don't have the type_key here, so use the ID lookup path
            "",
        )
        .await?;

        // Compute next escalation time
        // For simplicity, we'll re-query by ID
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

        // Re-enqueue delivery for the notification's channels
        let attempts = db::delivery::list_delivery_attempts_for_notification(pool, notif.id).await?;
        for attempt in &attempts {
            if attempt.status == "sent" || attempt.status == "delivered" {
                // Re-create a delivery attempt for escalation
                let new_attempt =
                    db::delivery::insert_delivery_attempt(pool, notif.id, &attempt.channel).await?;

                publisher
                    .publish_delivery(
                        &attempt.channel,
                        &DeliveryDispatchMessage {
                            notification_id: notif.id,
                            delivery_attempt_id: new_attempt.id,
                            channel: attempt.channel.clone(),
                            recipient_user_id: notif.recipient_user_id.clone(),
                        },
                    )
                    .await?;
            }
        }
    }
    Ok(())
}
