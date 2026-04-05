mod common;

use estafeta_proto::estafeta::v1::*;
use std::collections::BTreeMap;

fn simple_schema() -> prost_types::Struct {
    prost_types::Struct {
        fields: BTreeMap::from([(
            "type".to_string(),
            prost_types::Value {
                kind: Some(prost_types::value::Kind::StringValue("object".into())),
            },
        )]),
    }
}

async fn setup_svc(env: &mut common::TestEnv, slug: &str) {
    env.admin_client
        .register_service(RegisterServiceRequest {
            slug: slug.into(),
            display_name: slug.into(),
            description: String::new(),
        })
        .await
        .unwrap();

    env.schema_client
        .register_type(RegisterTypeRequest {
            service_slug: slug.into(),
            type_key: "notif".into(),
            display_name: "Notification".into(),
            description: String::new(),
            json_schema: Some(simple_schema()),
            default_ttl_seconds: 0,
            escalation_interval_seconds: 0,
            max_escalations: 0,
            escalation_action: EscalationAction::Unspecified as i32,
            default_icon: String::new(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_full_preferences_with_service_and_type() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "pref-svc").await;

    // Set service pref (no channels)
    env.user_config_client
        .set_service_preference(SetServicePreferenceRequest {
            service_slug: "pref-svc".into(),
            enabled: true,
            min_severity: 3,
        })
        .await
        .unwrap();

    // Set type pref (no channels)
    env.user_config_client
        .set_type_preference(SetTypePreferenceRequest {
            service_slug: "pref-svc".into(),
            type_key: "notif".into(),
            enabled: true,
        })
        .await
        .unwrap();

    // Get full preferences — should include both
    let prefs = env
        .user_config_client
        .get_preferences(GetPreferencesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(prefs.global_enabled);
    assert!(!prefs.service_preferences.is_empty());
    assert!(!prefs.type_preferences.is_empty());

    // Delete type pref
    env.user_config_client
        .delete_type_preference(DeleteTypePreferenceRequest {
            service_slug: "pref-svc".into(),
            type_key: "notif".into(),
        })
        .await
        .unwrap();

    // Delete service pref
    env.user_config_client
        .delete_service_preference(DeleteServicePreferenceRequest {
            service_slug: "pref-svc".into(),
        })
        .await
        .unwrap();

    // Verify cleaned up
    let prefs = env
        .user_config_client
        .get_preferences(GetPreferencesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(prefs.service_preferences.is_empty());
    assert!(prefs.type_preferences.is_empty());
}

#[tokio::test]
async fn test_preferences_catch_up_and_sort_mode() {
    let mut env = common::TestEnv::new().await;

    // Get defaults
    let prefs = env
        .user_config_client
        .get_preferences(GetPreferencesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(prefs.global_enabled);

    // Update with catch_up_mode and sort_mode
    env.user_config_client
        .update_preferences(UpdatePreferencesRequest {
            global_enabled: true,
            catch_up_mode: "all_unseen".into(),
            sort_mode: "priority".into(),
        })
        .await
        .unwrap();

    let prefs = env
        .user_config_client
        .get_preferences(GetPreferencesRequest {})
        .await
        .unwrap()
        .into_inner();

    assert_eq!(prefs.catch_up_mode, "all_unseen");
    assert_eq!(prefs.sort_mode, "priority");
}

#[tokio::test]
async fn test_mute_rule_with_type_filter() {
    let mut env = common::TestEnv::new().await;
    setup_svc(&mut env, "mute-type").await;

    // Mute only a specific notification type
    let rule = env
        .user_config_client
        .create_mute_rule(CreateMuteRuleRequest {
            service_slug: "mute-type".into(),
            notification_type: "notif".into(),
            muted_until: None,
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(rule.service_slug, "mute-type");
    assert_eq!(rule.notification_type, "notif");

    // List should show it
    let rules = env
        .user_config_client
        .list_mute_rules(ListMuteRulesRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(rules.rules.len(), 1);

    // Delete
    env.user_config_client
        .delete_mute_rule(DeleteMuteRuleRequest {
            rule_id: rule.id,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_mute_rule_with_expiration() {
    let mut env = common::TestEnv::new().await;

    let future = chrono::Utc::now() + chrono::Duration::hours(2);
    let rule = env
        .user_config_client
        .create_mute_rule(CreateMuteRuleRequest {
            service_slug: String::new(), // global mute
            notification_type: String::new(),
            muted_until: Some(prost_types::Timestamp {
                seconds: future.timestamp(),
                nanos: 0,
            }),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(rule.muted_until.is_some());

    env.user_config_client
        .delete_mute_rule(DeleteMuteRuleRequest {
            rule_id: rule.id,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_delete_nonexistent_mute_rule() {
    let mut env = common::TestEnv::new().await;

    let result = env
        .user_config_client
        .delete_mute_rule(DeleteMuteRuleRequest {
            rule_id: uuid::Uuid::new_v4().to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_service_preference_nonexistent_service() {
    let mut env = common::TestEnv::new().await;

    let result = env
        .user_config_client
        .set_service_preference(SetServicePreferenceRequest {
            service_slug: "nope".into(),
            enabled: true,
            min_severity: 0,
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}
