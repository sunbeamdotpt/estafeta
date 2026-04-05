use chrono::Utc;

use crate::cache::{
    CachedNotificationType,
    CachedUserPrefs,
};

/// The resolved configuration for a single notification.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub should_deliver: bool,
    pub ttl_seconds: Option<i32>,
    pub escalation_interval_seconds: Option<i32>,
    pub max_escalations: i32,
    /// If not delivered, the reason (for logging/audit).
    pub skip_reason: Option<String>,
}

/// Resolve the effective config by applying the preference hierarchy:
/// Global Policy -> Service/Type Defaults -> User Service Pref -> User Type Pref -> Mute Rules
pub fn resolve(
    user_prefs: &CachedUserPrefs,
    notif_type: &CachedNotificationType,
    level_severity: Option<i32>,
) -> ResolvedConfig {
    // 1. Global user toggle
    if !user_prefs.global_enabled {
        return ResolvedConfig {
            should_deliver: false,
            ttl_seconds: notif_type.default_ttl_seconds,
            escalation_interval_seconds: notif_type.escalation_interval_seconds,
            max_escalations: notif_type.max_escalations,
            skip_reason: Some("user has notifications globally disabled".into()),
        };
    }

    // 2. Check mute rules
    let now = Utc::now();
    for rule in &user_prefs.mute_rules {
        let matches_service = rule.service_id.is_none()
            || rule.service_id == Some(notif_type.service_id);
        let matches_type = rule.notification_type_id.is_none()
            || rule.notification_type_id == Some(notif_type.id);
        let is_active =
            rule.muted_until.is_none() || rule.muted_until.is_some_and(|t| t > now);

        if matches_service && matches_type && is_active {
            return ResolvedConfig {
                should_deliver: false,
                ttl_seconds: notif_type.default_ttl_seconds,
                escalation_interval_seconds: notif_type.escalation_interval_seconds,
                max_escalations: notif_type.max_escalations,
                skip_reason: Some("muted by user rule".into()),
            };
        }
    }

    // 3. Service-level preference
    let service_pref = user_prefs
        .service_prefs
        .iter()
        .find(|p| p.service_id == notif_type.service_id);

    if let Some(sp) = service_pref {
        if !sp.enabled {
            return ResolvedConfig {
                should_deliver: false,
                ttl_seconds: notif_type.default_ttl_seconds,
                escalation_interval_seconds: notif_type.escalation_interval_seconds,
                max_escalations: notif_type.max_escalations,
                skip_reason: Some("service disabled by user preference".into()),
            };
        }

        // Check min severity threshold
        if let (Some(min_sev), Some(level_sev)) = (sp.min_severity, level_severity) {
            if level_sev < min_sev {
                return ResolvedConfig {
                    should_deliver: false,
                    ttl_seconds: notif_type.default_ttl_seconds,
                    escalation_interval_seconds: notif_type.escalation_interval_seconds,
                    max_escalations: notif_type.max_escalations,
                    skip_reason: Some(format!(
                        "level severity {level_sev} below user threshold {min_sev}"
                    )),
                };
            }
        }
    }

    // 4. Type-level preference
    let type_pref = user_prefs
        .type_prefs
        .iter()
        .find(|p| p.notification_type_id == notif_type.id);

    if let Some(tp) = type_pref {
        if !tp.enabled {
            return ResolvedConfig {
                should_deliver: false,
                ttl_seconds: notif_type.default_ttl_seconds,
                escalation_interval_seconds: notif_type.escalation_interval_seconds,
                max_escalations: notif_type.max_escalations,
                skip_reason: Some("notification type disabled by user preference".into()),
            };
        }
    }

    ResolvedConfig {
        should_deliver: true,
        ttl_seconds: notif_type.default_ttl_seconds,
        escalation_interval_seconds: notif_type.escalation_interval_seconds,
        max_escalations: notif_type.max_escalations,
        skip_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CachedMuteRule, CachedServicePref, CachedTypePref};
    use uuid::Uuid;

    fn make_notif_type() -> CachedNotificationType {
        CachedNotificationType {
            id: Uuid::new_v4(),
            service_id: Uuid::new_v4(),
            type_key: "test".into(),
            json_schema: serde_json::json!({}),
            default_ttl_seconds: Some(3600),
            escalation_interval_seconds: None,
            max_escalations: 0,
            escalation_action: "resurface".into(),
            default_icon: None,
            enabled: true,
        }
    }

    fn make_user_prefs() -> CachedUserPrefs {
        CachedUserPrefs {
            global_enabled: true,
            service_prefs: vec![],
            type_prefs: vec![],
            mute_rules: vec![],
        }
    }

    #[test]
    fn test_global_disabled() {
        let mut prefs = make_user_prefs();
        prefs.global_enabled = false;
        let result = resolve(&prefs, &make_notif_type(), None);
        assert!(!result.should_deliver);
    }

    #[test]
    fn test_muted_permanently() {
        let nt = make_notif_type();
        let mut prefs = make_user_prefs();
        prefs.mute_rules.push(CachedMuteRule {
            service_id: Some(nt.service_id),
            notification_type_id: None,
            muted_until: None,
        });
        let result = resolve(&prefs, &nt, None);
        assert!(!result.should_deliver);
    }

    #[test]
    fn test_min_severity_blocks() {
        let nt = make_notif_type();
        let mut prefs = make_user_prefs();
        prefs.service_prefs.push(CachedServicePref {
            service_id: nt.service_id,
            enabled: true,
            min_severity: Some(5),
        });
        let result = resolve(&prefs, &nt, Some(2));
        assert!(!result.should_deliver);
    }

    #[test]
    fn test_min_severity_passes() {
        let nt = make_notif_type();
        let mut prefs = make_user_prefs();
        prefs.service_prefs.push(CachedServicePref {
            service_id: nt.service_id,
            enabled: true,
            min_severity: Some(3),
        });
        let result = resolve(&prefs, &nt, Some(5));
        assert!(result.should_deliver);
    }

    #[test]
    fn test_type_pref_disabled() {
        let nt = make_notif_type();
        let mut prefs = make_user_prefs();
        prefs.type_prefs.push(CachedTypePref {
            notification_type_id: nt.id,
            enabled: false,
        });
        let result = resolve(&prefs, &nt, None);
        assert!(!result.should_deliver);
    }

    #[test]
    fn test_default_deliver() {
        let prefs = make_user_prefs();
        let nt = make_notif_type();
        let result = resolve(&prefs, &nt, None);
        assert!(result.should_deliver);
    }
}
