use prost_types::Timestamp;
use sqlx::PgPool;
use tonic::{Request, Response, Status};

use estafeta_proto::estafeta::v1::{
    schema_registry_service_server::SchemaRegistryService as SchemaRegistryServiceTrait,
    GetTypeRequest, ListLevelsRequest, ListLevelsResponse, ListTypesRequest, ListTypesResponse,
    NotificationLevel, NotificationType, RegisterLevelRequest, RegisterTypeRequest,
    UpdateLevelRequest, UpdateTypeRequest, ValidatePayloadRequest, ValidatePayloadResponse,
};

use crate::auth::{AuthClaims, KetoClient};
use crate::cache::AppCaches;
use crate::db;
use crate::processing::schema_validator;

use super::proto_convert;

pub struct SchemaRegistryServiceImpl {
    pool: PgPool,
    keto: KetoClient,
    caches: AppCaches,
}

impl SchemaRegistryServiceImpl {
    pub fn new(pool: PgPool, keto: KetoClient, caches: AppCaches) -> Self {
        Self { pool, keto, caches }
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
impl SchemaRegistryServiceTrait for SchemaRegistryServiceImpl {
    async fn register_type(
        &self,
        request: Request<RegisterTypeRequest>,
    ) -> Result<Response<NotificationType>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("service '{}' not found", req.service_slug)))?;

        let schema_value = req
            .json_schema
            .as_ref()
            .map(|s| proto_convert::proto_struct_to_value(s))
            .unwrap_or(serde_json::json!({"type": "object"}));

        // Validate the schema itself is valid JSON Schema
        if jsonschema::validator_for(&schema_value).is_err() {
            return Err(Status::invalid_argument("invalid JSON schema"));
        }

        let channels: Vec<String> = req.default_channels.iter().map(|c| channel_from_proto(*c)).collect();

        let row = db::schemas::insert_notification_type(
            &self.pool,
            service.id,
            &req.type_key,
            &req.display_name,
            if req.description.is_empty() { None } else { Some(&req.description) },
            &schema_value,
            &channels,
            if req.default_ttl_seconds > 0 { Some(req.default_ttl_seconds) } else { None },
            if req.escalation_interval_seconds > 0 { Some(req.escalation_interval_seconds) } else { None },
            req.max_escalations,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        // Invalidate cache
        let cache_key = AppCaches::notification_type_key(&req.service_slug, &req.type_key);
        self.caches.notification_types.invalidate(&cache_key).await;

        Ok(Response::new(row_to_proto(&row, &req.service_slug)))
    }

    async fn update_type(
        &self,
        request: Request<UpdateTypeRequest>,
    ) -> Result<Response<NotificationType>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let schema_value = req
            .json_schema
            .as_ref()
            .map(|s| proto_convert::proto_struct_to_value(s))
            .unwrap_or(serde_json::json!({"type": "object"}));

        let channels: Vec<String> = req.default_channels.iter().map(|c| channel_from_proto(*c)).collect();

        let row = db::schemas::update_notification_type(
            &self.pool,
            service.id,
            &req.type_key,
            &req.display_name,
            if req.description.is_empty() { None } else { Some(&req.description) },
            &schema_value,
            &channels,
            if req.default_ttl_seconds > 0 { Some(req.default_ttl_seconds) } else { None },
            if req.escalation_interval_seconds > 0 { Some(req.escalation_interval_seconds) } else { None },
            req.max_escalations,
            req.enabled,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let cache_key = AppCaches::notification_type_key(&req.service_slug, &req.type_key);
        self.caches.notification_types.invalidate(&cache_key).await;

        Ok(Response::new(row_to_proto(&row, &req.service_slug)))
    }

    async fn get_type(
        &self,
        request: Request<GetTypeRequest>,
    ) -> Result<Response<NotificationType>, Status> {
        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let row = db::schemas::get_notification_type(&self.pool, service.id, &req.type_key)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification type not found"))?;

        Ok(Response::new(row_to_proto(&row, &req.service_slug)))
    }

    async fn list_types(
        &self,
        request: Request<ListTypesRequest>,
    ) -> Result<Response<ListTypesResponse>, Status> {
        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let page_size = req.pagination.as_ref().map(|p| p.page_size as i64).unwrap_or(50).min(100);
        let offset = req.pagination.as_ref().and_then(|p| p.page_token.parse::<i64>().ok()).unwrap_or(0);

        let rows = db::schemas::list_notification_types(&self.pool, service.id, page_size, offset)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ListTypesResponse {
            types: rows.iter().map(|r| row_to_proto(r, &req.service_slug)).collect(),
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

    async fn register_level(
        &self,
        request: Request<RegisterLevelRequest>,
    ) -> Result<Response<NotificationLevel>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let row = db::schemas::insert_notification_level(
            &self.pool,
            service.id,
            &req.key,
            &req.display_name,
            req.severity,
            if req.color.is_empty() { None } else { Some(&req.color) },
            if req.icon.is_empty() { None } else { Some(&req.icon) },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(NotificationLevel {
            id: row.id.to_string(),
            service_slug: req.service_slug,
            key: row.key,
            display_name: row.display_name,
            severity: row.severity,
            color: row.color.unwrap_or_default(),
            icon: row.icon.unwrap_or_default(),
        }))
    }

    async fn update_level(
        &self,
        request: Request<UpdateLevelRequest>,
    ) -> Result<Response<NotificationLevel>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        self.keto.require_admin(&subject).await?;

        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let row = db::schemas::update_notification_level(
            &self.pool,
            service.id,
            &req.key,
            &req.display_name,
            req.severity,
            if req.color.is_empty() { None } else { Some(&req.color) },
            if req.icon.is_empty() { None } else { Some(&req.icon) },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(NotificationLevel {
            id: row.id.to_string(),
            service_slug: req.service_slug,
            key: row.key,
            display_name: row.display_name,
            severity: row.severity,
            color: row.color.unwrap_or_default(),
            icon: row.icon.unwrap_or_default(),
        }))
    }

    async fn list_levels(
        &self,
        request: Request<ListLevelsRequest>,
    ) -> Result<Response<ListLevelsResponse>, Status> {
        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let rows = db::schemas::list_notification_levels(&self.pool, service.id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ListLevelsResponse {
            levels: rows
                .into_iter()
                .map(|r| NotificationLevel {
                    id: r.id.to_string(),
                    service_slug: req.service_slug.clone(),
                    key: r.key,
                    display_name: r.display_name,
                    severity: r.severity,
                    color: r.color.unwrap_or_default(),
                    icon: r.icon.unwrap_or_default(),
                })
                .collect(),
        }))
    }

    async fn validate_payload(
        &self,
        request: Request<ValidatePayloadRequest>,
    ) -> Result<Response<ValidatePayloadResponse>, Status> {
        let req = request.into_inner();
        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let nt = db::schemas::get_notification_type(&self.pool, service.id, &req.notification_type)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification type not found"))?;

        let payload_value = req
            .payload
            .as_ref()
            .map(|s| proto_convert::proto_struct_to_value(s))
            .unwrap_or(serde_json::json!({}));

        match schema_validator::validate_payload(&nt.json_schema, &payload_value) {
            Ok(()) => Ok(Response::new(ValidatePayloadResponse {
                valid: true,
                errors: vec![],
            })),
            Err(errors) => Ok(Response::new(ValidatePayloadResponse {
                valid: false,
                errors,
            })),
        }
    }
}

fn row_to_proto(
    row: &db::schemas::NotificationTypeRow,
    service_slug: &str,
) -> NotificationType {
    NotificationType {
        id: row.id.to_string(),
        service_slug: service_slug.to_string(),
        type_key: row.type_key.clone(),
        display_name: row.display_name.clone(),
        description: row.description.clone().unwrap_or_default(),
        json_schema: proto_convert::value_to_proto_struct(&row.json_schema),
        default_channels: row.default_channels.iter().map(|c| channel_to_proto(c)).collect(),
        default_ttl_seconds: row.default_ttl_seconds.unwrap_or(0),
        escalation_interval_seconds: row.escalation_interval_seconds.unwrap_or(0),
        max_escalations: row.max_escalations,
        enabled: row.enabled,
        created_at: to_timestamp(row.created_at),
        updated_at: to_timestamp(row.updated_at),
    }
}
