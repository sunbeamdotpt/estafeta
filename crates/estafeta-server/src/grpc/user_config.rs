use prost_types::Timestamp;
use sqlx::PgPool;
use tonic::{Request, Response, Status};

use estafeta_proto::estafeta::v1::{
    user_config_service_server::UserConfigService as UserConfigServiceTrait,
    CreateMuteRuleRequest, DeleteMuteRuleRequest,
    DeleteServicePreferenceRequest, DeleteTypePreferenceRequest,
    GetPreferencesRequest, ListMuteRulesRequest,
    ListMuteRulesResponse, MuteRule, ServicePreference,
    SetServicePreferenceRequest, SetTypePreferenceRequest, TypePreference,
    UpdatePreferencesRequest, UserPreferences,
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
            catch_up_mode: pref.catch_up_mode,
            sort_mode: pref.sort_mode,
            service_preferences: svc_prefs
                .into_iter()
                .map(|sp| ServicePreference {
                    service_slug: String::new(), // would need reverse lookup
                    enabled: sp.enabled,
                    min_severity: sp.min_severity.unwrap_or(0),
                })
                .collect(),
            type_preferences: type_prefs
                .into_iter()
                .map(|tp| TypePreference {
                    service_slug: String::new(),
                    type_key: String::new(),
                    enabled: tp.enabled,
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

        let catch_up_mode = if req.catch_up_mode.is_empty() {
            "all_unseen"
        } else {
            &req.catch_up_mode
        };
        let sort_mode = if req.sort_mode.is_empty() {
            "chronological"
        } else {
            &req.sort_mode
        };

        let pref = db::preferences::update_user_preference(
            &self.pool,
            &subject,
            req.global_enabled,
            catch_up_mode,
            sort_mode,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(UserPreferences {
            global_enabled: pref.global_enabled,
            catch_up_mode: pref.catch_up_mode,
            sort_mode: pref.sort_mode,
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

        let row = db::preferences::upsert_service_preference(
            &self.pool,
            &subject,
            service.id,
            req.enabled,
            if req.min_severity > 0 { Some(req.min_severity) } else { None },
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(ServicePreference {
            service_slug: req.service_slug,
            enabled: row.enabled,
            min_severity: row.min_severity.unwrap_or(0),
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

        let row = db::preferences::upsert_type_preference(
            &self.pool,
            &subject,
            nt.id,
            req.enabled,
        )
        .await
        .map_err(|e| Status::internal(format!("db error: {e}")))?;

        Ok(Response::new(TypePreference {
            service_slug: req.service_slug,
            type_key: req.type_key,
            enabled: row.enabled,
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
}
