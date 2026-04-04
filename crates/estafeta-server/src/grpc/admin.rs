use prost_types::Timestamp;
use sqlx::PgPool;
use tonic::{Request, Response, Status};

use estafeta_proto::estafeta::v1::{
    admin_service_server::AdminService as AdminServiceTrait, DisableServiceRequest,
    EnableServiceRequest, GetGlobalPolicyRequest, GlobalPolicy, ListServicesRequest,
    ListServicesResponse, RegisterServiceRequest, ReplayNotificationRequest, Service,
    SetGlobalPolicyRequest, UpdateServiceRequest,
};

use crate::auth::{AuthClaims, KetoClient};
use crate::db;
use crate::nats::NatsPublisher;

pub struct AdminServiceImpl {
    pool: PgPool,
    keto: KetoClient,
    publisher: NatsPublisher,
}

impl AdminServiceImpl {
    pub fn new(pool: PgPool, keto: KetoClient, publisher: NatsPublisher) -> Self {
        Self {
            pool,
            keto,
            publisher,
        }
    }
}

fn to_timestamp(dt: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
    Some(Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    })
}

fn channel_from_proto(ch: i32) -> String {
    match ch {
        1 => "email".into(),
        2 => "push".into(),
        3 => "sms".into(),
        4 => "webhook".into(),
        _ => "email".into(),
    }
}

fn channel_to_proto(ch: &str) -> i32 {
    match ch {
        "email" => 1,
        "push" => 2,
        "sms" => 3,
        "webhook" => 4,
        _ => 0,
    }
}

#[tonic::async_trait]
impl AdminServiceTrait for AdminServiceImpl {
    async fn register_service(
        &self,
        request: Request<RegisterServiceRequest>,
    ) -> Result<Response<Service>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let row = db::services::insert_service(
            &self.pool,
            &req.slug,
            &req.display_name,
            if req.description.is_empty() {
                None
            } else {
                Some(&req.description)
            },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(Service {
            id: row.id.to_string(),
            slug: row.slug,
            display_name: row.display_name,
            description: row.description.unwrap_or_default(),
            enabled: row.enabled,
            created_at: to_timestamp(row.created_at),
            updated_at: to_timestamp(row.updated_at),
        }))
    }

    async fn update_service(
        &self,
        request: Request<UpdateServiceRequest>,
    ) -> Result<Response<Service>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let row = db::services::update_service(
            &self.pool,
            &req.slug,
            &req.display_name,
            if req.description.is_empty() {
                None
            } else {
                Some(&req.description)
            },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(Service {
            id: row.id.to_string(),
            slug: row.slug,
            display_name: row.display_name,
            description: row.description.unwrap_or_default(),
            enabled: row.enabled,
            created_at: to_timestamp(row.created_at),
            updated_at: to_timestamp(row.updated_at),
        }))
    }

    async fn list_services(
        &self,
        request: Request<ListServicesRequest>,
    ) -> Result<Response<ListServicesResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let page_size = req
            .pagination
            .as_ref()
            .map(|p| p.page_size as i64)
            .unwrap_or(50)
            .min(100);
        let offset = req
            .pagination
            .as_ref()
            .and_then(|p| p.page_token.parse::<i64>().ok())
            .unwrap_or(0);

        let rows = db::services::list_services(&self.pool, page_size, offset)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let total = db::services::count_services(&self.pool)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let next_token = if (offset + page_size) < total {
            (offset + page_size).to_string()
        } else {
            String::new()
        };

        Ok(Response::new(ListServicesResponse {
            services: rows
                .into_iter()
                .map(|r| Service {
                    id: r.id.to_string(),
                    slug: r.slug,
                    display_name: r.display_name,
                    description: r.description.unwrap_or_default(),
                    enabled: r.enabled,
                    created_at: to_timestamp(r.created_at),
                    updated_at: to_timestamp(r.updated_at),
                })
                .collect(),
            pagination: Some(estafeta_proto::estafeta::v1::PaginatedResponse {
                next_page_token: next_token,
                total_count: total,
            }),
        }))
    }

    async fn disable_service(
        &self,
        request: Request<DisableServiceRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        db::services::set_service_enabled(&self.pool, &req.slug, false)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn enable_service(
        &self,
        request: Request<EnableServiceRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        db::services::set_service_enabled(&self.pool, &req.slug, true)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn set_global_policy(
        &self,
        request: Request<SetGlobalPolicyRequest>,
    ) -> Result<Response<GlobalPolicy>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let channels: Vec<String> = req.default_channels.iter().map(|c| channel_from_proto(*c)).collect();

        let row = db::preferences::update_global_policy(
            &self.pool,
            req.max_notifications_per_user_per_hour,
            req.max_ttl_seconds,
            req.max_escalations,
            &channels,
            req.rate_limit_per_service_per_second,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(GlobalPolicy {
            max_notifications_per_user_per_hour: row.max_notifications_per_user_per_hour,
            max_ttl_seconds: row.max_ttl_seconds,
            max_escalations: row.max_escalations,
            default_channels: row.default_channels.iter().map(|c| channel_to_proto(c)).collect(),
            rate_limit_per_service_per_second: row.rate_limit_per_service_per_second,
            updated_at: to_timestamp(row.updated_at),
        }))
    }

    async fn get_global_policy(
        &self,
        request: Request<GetGlobalPolicyRequest>,
    ) -> Result<Response<GlobalPolicy>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let _req = request.into_inner();

        let row = db::preferences::get_global_policy(&self.pool)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(GlobalPolicy {
            max_notifications_per_user_per_hour: row.max_notifications_per_user_per_hour,
            max_ttl_seconds: row.max_ttl_seconds,
            max_escalations: row.max_escalations,
            default_channels: row.default_channels.iter().map(|c| channel_to_proto(c)).collect(),
            rate_limit_per_service_per_second: row.rate_limit_per_service_per_second,
            updated_at: to_timestamp(row.updated_at),
        }))
    }

    async fn replay_notification(
        &self,
        request: Request<ReplayNotificationRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let notif_id: uuid::Uuid = req
            .notification_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid notification_id"))?;

        let notif = db::notifications::get_notification(&self.pool, notif_id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification not found"))?;

        let channels: Vec<String> = if req.channels.is_empty() {
            // Re-deliver to all original channels
            let attempts =
                db::delivery::list_delivery_attempts_for_notification(&self.pool, notif_id)
                    .await
                    .map_err(|e| Status::internal(format!("db error: {e}")))?;
            attempts.into_iter().map(|a| a.channel).collect()
        } else {
            req.channels.iter().map(|c| channel_from_proto(*c)).collect()
        };

        for channel in &channels {
            let attempt = db::delivery::insert_delivery_attempt(&self.pool, notif_id, channel)
                .await
                .map_err(|e| Status::internal(format!("db error: {e}")))?;

            self.publisher
                .publish_delivery(
                    channel,
                    &crate::nats::DeliveryDispatchMessage {
                        notification_id: notif_id,
                        delivery_attempt_id: attempt.id,
                        channel: channel.clone(),
                        recipient_user_id: notif.recipient_user_id.clone(),
                    },
                )
                .await
                .map_err(|e| Status::internal(format!("nats error: {e}")))?;
        }

        Ok(Response::new(()))
    }
}
