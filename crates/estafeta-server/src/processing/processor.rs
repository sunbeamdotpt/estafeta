use std::sync::Arc;

use anyhow::Result;
use async_nats::jetstream;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::cache::*;
use crate::db;
use crate::nats::{DeliveryDispatchMessage, NatsPublisher, RealtimeEvent, RealtimeEventType};
use crate::processing::preference_resolver;

/// The main notification processor: pulls from JetStream, persists to PG, and fans out.
pub struct Processor {
    pool: PgPool,
    publisher: NatsPublisher,
    caches: AppCaches,
    consumer: jetstream::consumer::Consumer<jetstream::consumer::pull::Config>,
}

impl Processor {
    pub fn new(
        pool: PgPool,
        publisher: NatsPublisher,
        caches: AppCaches,
        consumer: jetstream::consumer::Consumer<jetstream::consumer::pull::Config>,
    ) -> Self {
        Self {
            pool,
            publisher,
            caches,
            consumer,
        }
    }

    pub async fn run(self) -> Result<()> {
        info!("notification processor started");
        let mut messages: jetstream::consumer::pull::Stream = self
            .consumer
            .messages()
            .await?;

        use futures::StreamExt;
        while let Some(msg) = messages.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, "failed to receive message from JetStream");
                    continue;
                }
            };

            if let Err(e) = self.process_message(&msg).await {
                error!(error = %e, "failed to process notification");
                // NAK will cause redelivery
                if let Err(nak_err) = msg.ack_with(async_nats::jetstream::AckKind::Nak(None)).await {
                    error!(error = %nak_err, "failed to NAK message");
                }
                continue;
            }

            if let Err(e) = msg.ack().await {
                error!(error = %e, "failed to ACK message");
            }
        }

        Ok(())
    }

    async fn process_message(
        &self,
        msg: &async_nats::jetstream::Message,
    ) -> Result<()> {
        let ingest: crate::nats::IngestMessage =
            serde_json::from_slice(&msg.payload)?;

        // Look up notification type from cache
        let cache_key =
            AppCaches::notification_type_key(&ingest.service_slug, &ingest.notification_type);
        let notif_type = self
            .get_or_load_notif_type(&cache_key, &ingest.service_slug, &ingest.notification_type)
            .await?;

        // Look up level severity if provided
        let level_severity = if !ingest.level.is_empty() {
            let level_key =
                AppCaches::notification_level_key(&ingest.service_slug, &ingest.level);
            let level = self
                .get_or_load_level(&level_key, &ingest.service_slug, &ingest.level)
                .await?;
            level.map(|l| l.severity)
        } else {
            None
        };

        let level_id = if !ingest.level.is_empty() {
            let level_key =
                AppCaches::notification_level_key(&ingest.service_slug, &ingest.level);
            let level = self
                .get_or_load_level(&level_key, &ingest.service_slug, &ingest.level)
                .await?;
            level.map(|l| l.id)
        } else {
            None
        };

        // Load user preferences
        let user_prefs = self
            .get_or_load_user_prefs(&ingest.recipient_user_id)
            .await?;

        // Load global policy defaults
        let global_policy = db::preferences::get_global_policy(&self.pool).await?;
        let default_channels = global_policy.default_channels;

        // Resolve delivery config
        let config = preference_resolver::resolve(
            &user_prefs,
            &notif_type,
            level_severity,
            &default_channels,
        );

        // Compute TTL
        let expires_at = config.ttl_seconds.map(|ttl| Utc::now() + Duration::seconds(ttl as i64));

        // Compute escalation
        let next_escalation_at = config.escalation_interval_seconds.and_then(|interval| {
            if config.max_escalations > 0 {
                Some(Utc::now() + Duration::seconds(interval as i64))
            } else {
                None
            }
        });

        // Determine state
        let state = if config.should_deliver {
            "unread"
        } else {
            "dismissed"
        };

        // Persist
        let notification = db::notifications::insert_notification(
            &self.pool,
            &db::notifications::InsertNotification {
                id: ingest.notification_id,
                service_id: notif_type.service_id,
                notification_type_id: notif_type.id,
                level_id,
                recipient_user_id: ingest.recipient_user_id.clone(),
                state: state.to_string(),
                payload: ingest.payload.clone(),
                group_key: ingest.group_key.clone(),
                idempotency_key: ingest.idempotency_key.clone(),
                metadata: serde_json::to_value(&ingest.metadata)?,
                expires_at,
                next_escalation_at,
            },
        )
        .await?;

        if config.should_deliver {
            // Enqueue delivery for each channel
            for channel in &config.channels {
                let attempt = db::delivery::insert_delivery_attempt(
                    &self.pool,
                    notification.id,
                    channel,
                )
                .await?;

                self.publisher
                    .publish_delivery(
                        channel,
                        &DeliveryDispatchMessage {
                            notification_id: notification.id,
                            delivery_attempt_id: attempt.id,
                            channel: channel.clone(),
                            recipient_user_id: ingest.recipient_user_id.clone(),
                        },
                    )
                    .await?;
            }

            // Publish real-time event
            self.publisher
                .publish_realtime(
                    &ingest.recipient_user_id,
                    &RealtimeEvent {
                        event_type: RealtimeEventType::NewNotification,
                        notification_id: notification.id,
                        service_slug: ingest.service_slug.clone(),
                        notification_type: ingest.notification_type.clone(),
                        level: Some(ingest.level.clone()),
                        payload: Some(ingest.payload),
                        group_key: ingest.group_key,
                        metadata: Some(ingest.metadata),
                        old_state: None,
                        new_state: None,
                        unread_count: None,
                    },
                )
                .await?;
        } else {
            info!(
                notification_id = %notification.id,
                reason = config.skip_reason.as_deref().unwrap_or("unknown"),
                "notification auto-dismissed"
            );
        }

        Ok(())
    }

    async fn get_or_load_notif_type(
        &self,
        cache_key: &str,
        service_slug: &str,
        type_key: &str,
    ) -> Result<Arc<CachedNotificationType>> {
        if let Some(cached) = self.caches.notification_types.get(cache_key).await {
            return Ok(cached);
        }

        let service = db::services::get_service_by_slug(&self.pool, service_slug)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown service: {service_slug}"))?;

        let nt = db::schemas::get_notification_type(&self.pool, service.id, type_key)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("unknown notification type: {service_slug}/{type_key}")
            })?;

        let cached = Arc::new(CachedNotificationType {
            id: nt.id,
            service_id: nt.service_id,
            type_key: nt.type_key,
            json_schema: nt.json_schema,
            default_channels: nt.default_channels,
            default_ttl_seconds: nt.default_ttl_seconds,
            escalation_interval_seconds: nt.escalation_interval_seconds,
            max_escalations: nt.max_escalations,
            enabled: nt.enabled,
        });

        self.caches
            .notification_types
            .insert(cache_key.to_string(), cached.clone())
            .await;

        Ok(cached)
    }

    async fn get_or_load_level(
        &self,
        cache_key: &str,
        service_slug: &str,
        level_key: &str,
    ) -> Result<Option<Arc<CachedNotificationLevel>>> {
        if let Some(cached) = self.caches.notification_levels.get(cache_key).await {
            return Ok(Some(cached));
        }

        let service = db::services::get_service_by_slug(&self.pool, service_slug)
            .await?
            .ok_or_else(|| anyhow::anyhow!("unknown service: {service_slug}"))?;

        let level = db::schemas::get_notification_level(&self.pool, service.id, level_key).await?;

        if let Some(level) = level {
            let cached = Arc::new(CachedNotificationLevel {
                id: level.id,
                key: level.key,
                severity: level.severity,
            });
            self.caches
                .notification_levels
                .insert(cache_key.to_string(), cached.clone())
                .await;
            Ok(Some(cached))
        } else {
            Ok(None)
        }
    }

    async fn get_or_load_user_prefs(
        &self,
        user_id: &str,
    ) -> Result<Arc<CachedUserPrefs>> {
        if let Some(cached) = self.caches.user_prefs.get(user_id).await {
            return Ok(cached);
        }

        let pref = db::preferences::get_or_create_user_preference(&self.pool, user_id).await?;
        let svc_prefs = db::preferences::list_service_preferences(&self.pool, user_id).await?;
        let type_prefs = db::preferences::list_type_preferences(&self.pool, user_id).await?;
        let mute_rules = db::mute_rules::list_active_mute_rules(&self.pool, user_id).await?;

        let cached = Arc::new(CachedUserPrefs {
            global_enabled: pref.global_enabled,
            service_prefs: svc_prefs
                .into_iter()
                .map(|sp| CachedServicePref {
                    service_id: sp.service_id,
                    enabled: sp.enabled,
                    min_severity: sp.min_severity,
                    channels: sp.channels,
                })
                .collect(),
            type_prefs: type_prefs
                .into_iter()
                .map(|tp| CachedTypePref {
                    notification_type_id: tp.notification_type_id,
                    enabled: tp.enabled,
                    channels: tp.channels,
                })
                .collect(),
            mute_rules: mute_rules
                .into_iter()
                .map(|mr| CachedMuteRule {
                    service_id: mr.service_id,
                    notification_type_id: mr.notification_type_id,
                    muted_until: mr.muted_until,
                })
                .collect(),
        });

        self.caches
            .user_prefs
            .insert(user_id.to_string(), cached.clone())
            .await;

        Ok(cached)
    }
}
