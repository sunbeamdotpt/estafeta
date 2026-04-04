use prost_types::Timestamp;
use sqlx::PgPool;
use tonic::{Request, Response, Status};

use estafeta_proto::estafeta::v1::{
    user_config_service_server::UserConfigService as UserConfigServiceTrait,
    ChannelConfig, CreateMuteRuleRequest, DeleteMuteRuleRequest,
    DeleteServicePreferenceRequest, DeleteTypePreferenceRequest, Device,
    GetChannelConfigRequest, GetPreferencesRequest, ListMuteRulesRequest,
    ListMuteRulesResponse, MuteRule, RegisterDeviceRequest, ServicePreference,
    SetServicePreferenceRequest, SetTypePreferenceRequest, TypePreference,
    UnregisterDeviceRequest, UpdateChannelConfigRequest, UpdatePreferencesRequest,
    UserPreferences,
};

use crate::auth::AuthClaims;
use crate::db;

pub struct UserConfigServiceImpl {
    pool: PgPool,
}

impl UserConfigServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
impl UserConfigServiceTrait for UserConfigServiceImpl {
    async fn get_preferences(
        &self,
        request: Request<GetPreferencesRequest>,
    ) -> Result<Response<UserPreferences>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();

        let pref = db::preferences::get_or_create_user_preference(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let svc_prefs = db::preferences::list_service_preferences(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        let type_prefs = db::preferences::list_type_preferences(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(UserPreferences {
            global_enabled: pref.global_enabled,
            service_preferences: svc_prefs
                .into_iter()
                .map(|sp| ServicePreference {
                    service_slug: String::new(), // would need reverse lookup
                    enabled: sp.enabled,
                    min_severity: sp.min_severity.unwrap_or(0),
                    channels: sp.channels.iter().map(|c| channel_to_proto(c)).collect(),
                })
                .collect(),
            type_preferences: type_prefs
                .into_iter()
                .map(|tp| TypePreference {
                    service_slug: String::new(),
                    type_key: String::new(),
                    enabled: tp.enabled,
                    channels: tp
                        .channels
                        .unwrap_or_default()
                        .iter()
                        .map(|c| channel_to_proto(c))
                        .collect(),
                })
                .collect(),
        }))
    }

    async fn update_preferences(
        &self,
        request: Request<UpdatePreferencesRequest>,
    ) -> Result<Response<UserPreferences>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let pref = db::preferences::update_user_preference(
            &self.pool,
            &subject,
            req.global_enabled,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(UserPreferences {
            global_enabled: pref.global_enabled,
            service_preferences: vec![],
            type_preferences: vec![],
        }))
    }

    async fn set_service_preference(
        &self,
        request: Request<SetServicePreferenceRequest>,
    ) -> Result<Response<ServicePreference>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let channels: Vec<String> = req.channels.iter().map(|c| channel_from_proto(*c)).collect();

        let row = db::preferences::upsert_service_preference(
            &self.pool,
            &subject,
            service.id,
            req.enabled,
            if req.min_severity > 0 { Some(req.min_severity) } else { None },
            &channels,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ServicePreference {
            service_slug: req.service_slug,
            enabled: row.enabled,
            min_severity: row.min_severity.unwrap_or(0),
            channels: row.channels.iter().map(|c| channel_to_proto(c)).collect(),
        }))
    }

    async fn delete_service_preference(
        &self,
        request: Request<DeleteServicePreferenceRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        db::preferences::delete_service_preference(&self.pool, &subject, service.id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn set_type_preference(
        &self,
        request: Request<SetTypePreferenceRequest>,
    ) -> Result<Response<TypePreference>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let nt = db::schemas::get_notification_type(&self.pool, service.id, &req.type_key)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification type not found"))?;

        let channels: Vec<String> = req.channels.iter().map(|c| channel_from_proto(*c)).collect();

        let row = db::preferences::upsert_type_preference(
            &self.pool,
            &subject,
            nt.id,
            req.enabled,
            if channels.is_empty() { None } else { Some(&channels) },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(TypePreference {
            service_slug: req.service_slug,
            type_key: req.type_key,
            enabled: row.enabled,
            channels: row
                .channels
                .unwrap_or_default()
                .iter()
                .map(|c| channel_to_proto(c))
                .collect(),
        }))
    }

    async fn delete_type_preference(
        &self,
        request: Request<DeleteTypePreferenceRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let service = db::services::get_service_by_slug(&self.pool, &req.service_slug)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("service not found"))?;

        let nt = db::schemas::get_notification_type(&self.pool, service.id, &req.type_key)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?
            .ok_or_else(|| Status::not_found("notification type not found"))?;

        db::preferences::delete_type_preference(&self.pool, &subject, nt.id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn list_mute_rules(
        &self,
        request: Request<ListMuteRulesRequest>,
    ) -> Result<Response<ListMuteRulesResponse>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();

        let rows = db::mute_rules::list_mute_rules(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ListMuteRulesResponse {
            rules: rows
                .into_iter()
                .map(|r| MuteRule {
                    id: r.id.to_string(),
                    service_slug: String::new(), // reverse lookup needed
                    notification_type: String::new(),
                    muted_until: r.muted_until.and_then(|d| to_timestamp(d)),
                    created_at: to_timestamp(r.created_at),
                })
                .collect(),
        }))
    }

    async fn create_mute_rule(
        &self,
        request: Request<CreateMuteRuleRequest>,
    ) -> Result<Response<MuteRule>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let service_id = if !req.service_slug.is_empty() {
            Some(
                db::services::get_service_by_slug(&self.pool, &req.service_slug)
                    .await
                    .map_err(|e| Status::internal(format!("db error: {e}")))?
                    .ok_or_else(|| Status::not_found("service not found"))?
                    .id,
            )
        } else {
            None
        };

        let notif_type_id = if !req.notification_type.is_empty() && service_id.is_some() {
            Some(
                db::schemas::get_notification_type(&self.pool, service_id.unwrap(), &req.notification_type)
                    .await
                    .map_err(|e| Status::internal(format!("db error: {e}")))?
                    .ok_or_else(|| Status::not_found("notification type not found"))?
                    .id,
            )
        } else {
            None
        };

        let muted_until = req.muted_until.and_then(|t| {
            chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32)
        });

        let row = db::mute_rules::insert_mute_rule(
            &self.pool,
            &subject,
            service_id,
            notif_type_id,
            muted_until,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(MuteRule {
            id: row.id.to_string(),
            service_slug: req.service_slug,
            notification_type: req.notification_type,
            muted_until: row.muted_until.and_then(|d| to_timestamp(d)),
            created_at: to_timestamp(row.created_at),
        }))
    }

    async fn delete_mute_rule(
        &self,
        request: Request<DeleteMuteRuleRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let rule_id: uuid::Uuid = req
            .rule_id
            .parse()
            .map_err(|_| Status::invalid_argument("invalid rule_id"))?;

        let deleted = db::mute_rules::delete_mute_rule(&self.pool, &subject, rule_id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        if !deleted {
            return Err(Status::not_found("mute rule not found"));
        }

        Ok(Response::new(()))
    }

    async fn register_device(
        &self,
        request: Request<RegisterDeviceRequest>,
    ) -> Result<Response<Device>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let row = db::preferences::upsert_device(
            &self.pool,
            &subject,
            &req.device_id,
            &req.platform,
            &req.push_token,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(Device {
            id: row.id.to_string(),
            device_id: row.device_id,
            platform: row.platform,
            push_token: row.push_token,
            created_at: to_timestamp(row.created_at),
        }))
    }

    async fn unregister_device(
        &self,
        request: Request<UnregisterDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        db::preferences::delete_device(&self.pool, &subject, &req.device_id)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(()))
    }

    async fn update_channel_config(
        &self,
        request: Request<UpdateChannelConfigRequest>,
    ) -> Result<Response<ChannelConfig>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let req = request.into_inner();

        let row = db::preferences::upsert_channel_config(
            &self.pool,
            &subject,
            if req.email_address.is_empty() { None } else { Some(&req.email_address) },
            if req.phone_number.is_empty() { None } else { Some(&req.phone_number) },
            if req.webhook_url.is_empty() { None } else { Some(&req.webhook_url) },
            if req.webhook_secret.is_empty() { None } else { Some(&req.webhook_secret) },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ChannelConfig {
            email_address: row.email_address.unwrap_or_default(),
            phone_number: row.phone_number.unwrap_or_default(),
            webhook_url: row.webhook_url.unwrap_or_default(),
            webhook_secret_set: row.webhook_secret.is_some(),
        }))
    }

    async fn get_channel_config(
        &self,
        request: Request<GetChannelConfigRequest>,
    ) -> Result<Response<ChannelConfig>, Status> {
        let subject = AuthClaims::from_extensions(request.extensions())?.subject.clone();
        let _req = request.into_inner();

        let row = db::preferences::get_channel_config(&self.pool, &subject)
            .await
            .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(match row {
            Some(r) => ChannelConfig {
                email_address: r.email_address.unwrap_or_default(),
                phone_number: r.phone_number.unwrap_or_default(),
                webhook_url: r.webhook_url.unwrap_or_default(),
                webhook_secret_set: r.webhook_secret.is_some(),
            },
            None => ChannelConfig::default(),
        }))
    }
}
