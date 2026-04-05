use prost_types::Timestamp;
use sqlx::PgPool;
use tonic::{Request, Response, Status};

use estafeta_proto::estafeta::v1::{
    notification_service_server::NotificationService as NotificationServiceTrait,
    ArchiveAllInGroupRequest, ArchiveAllInGroupResponse, ArchiveRequest,
    UnarchiveRequest, ProducerMarkReadRequest,
    GetNotificationRequest, GetUnreadCountRequest, GetUnseenCountRequest,
    ListNotificationsRequest, ListNotificationsResponse, MarkReadRequest,
    MarkSeenRequest, MarkSeenResponse, MarkUnreadRequest, Notification,
    SendNotificationBatchRequest, SendNotificationBatchResponse, SendNotificationRequest,
    SendNotificationResponse, SnoozeRequest, UnreadCountResponse, UnseenCountResponse,
};

use crate::auth::{AuthClaims, KetoClient};
use crate::cache::AppCaches;
use crate::db;
use crate::lifecycle::state_machine::NotificationState;
use crate::nats::{IngestMessage, NatsPublisher};
use crate::processing::schema_validator;

use super::proto_convert;

pub struct NotificationServiceImpl {
    pool: PgPool,
    keto: KetoClient,
    publisher: NatsPublisher,
    caches: AppCaches,
}

impl NotificationServiceImpl {
    pub fn new(
        pool: PgPool,
        keto: KetoClient,
        publisher: NatsPublisher,
        caches: AppCaches,
    ) -> Self {
        Self {
            pool,
            keto,
            publisher,
            caches,
        }
    }
}

fn to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
    Some(Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    })
}

fn opt_timestamp(dt: Option<chrono::DateTime<chrono::Utc>>) -> Option<Timestamp> {
    dt.and_then(|d| to_timestamp(d))
}

fn row_to_proto(row: &db::notifications::NotificationRow) -> Notification {
    Notification {
        id: row.id.to_string(),
        service_slug: String::new(), // filled by caller if needed
        notification_type: String::new(),
        level: String::new(),
        state: NotificationState::from_db_str(&row.state)
            .map(|s| s.to_proto())
            .unwrap_or(0),
        payload: proto_convert::value_to_proto_struct(&row.payload),
        group_key: row.group_key.clone().unwrap_or_default(),
        metadata: row
            .metadata
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        created_at: to_timestamp(row.created_at),
        seen_at: opt_timestamp(row.seen_at),
        read_at: opt_timestamp(row.read_at),
        snoozed_until: opt_timestamp(row.snoozed_until),
        expires_at: opt_timestamp(row.expires_at),
        escalation_count: row.escalation_count,
        recipient_user_id: row.recipient_user_id.clone(),
        action_url: row.action_url.clone().unwrap_or_default(),
        icon: row.icon.clone().unwrap_or_default(),
    }
}

#[tonic::async_trait]
impl NotificationServiceTrait for NotificationServiceImpl {
    async fn send_notification(
        &self,
        request: Request<SendNotificationRequest>,
    ) -> Result<Response<SendNotificationResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        // Authorize: caller must have send permission for the service
        self.keto
            .require_service_send(&req.service_slug, &subject)
            .await?;

        // Look up service and type
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        if !service.enabled {
            return Err(Status::failed_precondition("service is disabled"));
        }

        let notif_type = db::schemas::get_notification_type(&self.pool, service.id, &req.notification_type)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification type not found"))?;

        if !notif_type.enabled {
            return Err(Status::failed_precondition("notification type is disabled"));
        }

        // Validate payload against schema
        let payload_value = req
            .payload
            .as_ref()
            .map(|s| proto_convert::proto_struct_to_value(s))
            .unwrap_or(serde_json::json!({}));

        if let Err(errors) = schema_validator::validate_payload(&notif_type.json_schema, &payload_value) {
            return Err(Status::invalid_argument(format!(
                "payload validation failed: {}",
                errors.join("; ")
            )));
        }

        let notification_id = uuid::Uuid::new_v4();

        // Publish to JetStream for async processing
        let msg = IngestMessage {
            notification_id,
            service_slug: req.service_slug.clone(),
            notification_type: req.notification_type,
            recipient_user_id: req.recipient_user_id,
            level: req.level,
            payload: payload_value,
            idempotency_key: if req.idempotency_key.is_empty() {
                None
            } else {
                Some(req.idempotency_key)
            },
            group_key: if req.group_key.is_empty() {
                None
            } else {
                Some(req.group_key)
            },
            ttl_seconds: if req.ttl_seconds > 0 {
                Some(req.ttl_seconds)
            } else {
                None
            },
            metadata: req.metadata,
            action_url: if req.action_url.is_empty() {
                None
            } else {
                Some(req.action_url)
            },
            icon: if req.icon.is_empty() {
                None
            } else {
                Some(req.icon)
            },
        };

        self.publisher
            .publish_ingest(&req.service_slug, &msg)
            .await
            .map_err(|e| Status::internal(format!("failed to publish: {e}")))?;

        Ok(Response::new(SendNotificationResponse {
            notification_id: notification_id.to_string(),
        }))
    }

    async fn send_notification_batch(
        &self,
        request: Request<SendNotificationBatchRequest>,
    ) -> Result<Response<SendNotificationBatchResponse>, Status> {
        let claims = AuthClaims::from_extensions(request.extensions())?.clone();
        let req = request.into_inner();
        let mut results = Vec::with_capacity(req.notifications.len());
        let mut failed_count = 0i32;

        for notif_req in req.notifications {
            let mut inner_req = tonic::Request::new(notif_req);
            // Inject auth claims so the inner handler can see the subject
            inner_req
                .extensions_mut()
                .insert(claims.clone());

            match self.send_notification(inner_req).await {
                Ok(resp) => results.push(resp.into_inner()),
                Err(_) => {
                    failed_count += 1;
                    results.push(SendNotificationResponse {
                        notification_id: String::new(),
                    });
                }
            }
        }

        Ok(Response::new(SendNotificationBatchResponse {
            results,
            failed_count,
        }))
    }

    async fn get_notification(
        &self,
        request: Request<GetNotificationRequest>,
    ) -> Result<Response<Notification>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let id: uuid::Uuid = req
            .notification_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid notification_id"))?;

        let row = db::notifications::get_notification(&self.pool, id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification not found"))?;

        // Users can only see their own notifications
        if row.recipient_user_id != subject {
            return Err(Status::permission_denied("not your notification"));
        }

        Ok(Response::new(row_to_proto(&row)))
    }

    async fn list_notifications(
        &self,
        request: Request<ListNotificationsRequest>,
    ) -> Result<Response<ListNotificationsResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let states: Vec<String> = req
            .states
            .iter()
            .filter_map(|s| {
                NotificationState::from_proto(*s)
                    .ok()
                    .map(|ns| ns.as_db_str().to_string())
            })
            .collect();

        let page_size = req.pagination.as_ref().map(|p| p.page_size as i64).unwrap_or(50).min(100);
        let offset = req.pagination.as_ref().and_then(|p| p.page_token.parse::<i64>().ok()).unwrap_or(0);

        // Resolve service slugs to IDs
        let mut service_ids = Vec::new();
        for slug in &req.service_slugs {
            if let Some(svc) = db::services::get_service_by_slug(&self.pool, slug).await.map_err(|e| Status::internal(format!("db error: {e}")))? {
                service_ids.push(svc.id);
            }
        }

        let rows = db::notifications::list_notifications(
            &self.pool,
            &subject,
            &states,
            &service_ids,
            &[], // type_ids filtering not implemented in this pass
            if req.group_key.is_empty() { None } else { Some(&req.group_key) },
            page_size,
            offset,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ListNotificationsResponse {
            notifications: rows.iter().map(row_to_proto).collect(),
            pagination: Some(estafeta_proto::estafeta::v1::PaginatedResponse {
                next_page_token: if rows.len() as i64 == page_size {
                    (offset + page_size).to_string()
                } else {
                    String::new()
                },
                total_count: 0,
            }),
        }))
    }

    async fn mark_seen(
        &self,
        request: Request<MarkSeenRequest>,
    ) -> Result<Response<MarkSeenResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let marked = if req.notification_ids.is_empty() {
            // Mark all unseen as seen
            db::notifications::mark_all_seen(&self.pool, &subject)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?
        } else {
            let ids = parse_uuids(&req.notification_ids)?;
            db::notifications::mark_seen(&self.pool, &subject, &ids)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?
        };

        Ok(Response::new(MarkSeenResponse {
            marked_count: marked as i64,
        }))
    }

    async fn mark_read(
        &self,
        request: Request<MarkReadRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();
        let ids = parse_uuids(&req.notification_ids)?;

        db::notifications::mark_read(&self.pool, &subject, &ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn mark_unread(
        &self,
        request: Request<MarkUnreadRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();
        let ids = parse_uuids(&req.notification_ids)?;

        db::notifications::mark_unread(&self.pool, &subject, &ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn snooze(
        &self,
        request: Request<SnoozeRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let id: uuid::Uuid = req
            .notification_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid notification_id"))?;

        let until = req
            .snooze_until
            .ok_or_else(|| Status::invalid_argument("snooze_until is required"))?;

        let until_dt = chrono::DateTime::from_timestamp(until.seconds, until.nanos as u32)
            .ok_or_else(|| Status::invalid_argument("invalid snooze_until timestamp"))?;

        db::notifications::snooze(&self.pool, &subject, id, until_dt)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn archive(
        &self,
        request: Request<ArchiveRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();
        let ids = parse_uuids(&req.notification_ids)?;

        db::notifications::archive(&self.pool, &subject, &ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn unarchive(
        &self,
        request: Request<UnarchiveRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();
        let ids = parse_uuids(&req.notification_ids)?;

        db::notifications::unarchive(&self.pool, &subject, &ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn archive_all_in_group(
        &self,
        request: Request<ArchiveAllInGroupRequest>,
    ) -> Result<Response<ArchiveAllInGroupResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let archived = db::notifications::archive_all_in_group(&self.pool, &subject, service.id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ArchiveAllInGroupResponse {
            archived_count: archived as i64,
        }))
    }

    async fn producer_mark_read(
        &self,
        request: Request<ProducerMarkReadRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        // Authorize: caller must have send permission for the service
        self.keto
            .require_service_send(&req.service_slug, &subject)
            .await?;

        let ids = parse_uuids(&req.notification_ids)?;

        db::notifications::producer_mark_read(&self.pool, &req.recipient_user_id, &ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn get_unseen_count(
        &self,
        request: Request<GetUnseenCountRequest>,
    ) -> Result<Response<UnseenCountResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let mut service_ids = Vec::new();
        for slug in &req.service_slugs {
            if let Some(svc) = db::services::get_service_by_slug(&self.pool, slug)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?
            {
                service_ids.push(svc.id);
            }
        }

        let total = db::notifications::count_unseen(&self.pool, &subject, &service_ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let by_service = db::notifications::count_unseen_by_service(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let count_map: std::collections::HashMap<String, i64> = by_service
            .into_iter()
            .map(|(id, count)| (id.to_string(), count))
            .collect();

        Ok(Response::new(UnseenCountResponse {
            total_count: total,
            count_by_service: count_map,
        }))
    }

    async fn get_unread_count(
        &self,
        request: Request<GetUnreadCountRequest>,
    ) -> Result<Response<UnreadCountResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let mut service_ids = Vec::new();
        for slug in &req.service_slugs {
            if let Some(svc) = db::services::get_service_by_slug(&self.pool, slug)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?
            {
                service_ids.push(svc.id);
            }
        }

        let total = db::notifications::count_unread(&self.pool, &subject, &service_ids)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let by_service = db::notifications::count_unread_by_service(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let count_map: std::collections::HashMap<String, i64> = by_service
            .into_iter()
            .map(|(id, count)| (id.to_string(), count))
            .collect();

        Ok(Response::new(UnreadCountResponse {
            total_count: total,
            count_by_service: count_map,
        }))
    }
}

fn parse_uuids(ids: &[String]) -> Result<Vec<uuid::Uuid>, Status> {
    ids.iter()
        .map(|s| {
            s.parse::<uuid::Uuid>()
                .map_err(|_| Status::invalid_argument(format!("invalid UUID: {s}")))
        })
        .collect()
}
