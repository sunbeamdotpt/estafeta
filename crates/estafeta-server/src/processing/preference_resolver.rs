use chrono::Utc;

use crate::cache::{
    CachedNotificationType, CachedServicePref,
    CachedUserPrefs,
};

/// The resolved delivery configuration for a single notification.
#[derive(Debug, Clone)]
pub struct ResolvedDeliveryConfig {
    pub should_deliver: bool,
    pub channels: Vec<String>,
    pub ttl_seconds: Option<i32>,
    pub escalation_interval_seconds: Option<i32>,
    pub max_escalations: i32,
    /// If not delivered, the reason (for logging/audit).
    pub skip_reason: Option<String>,
}

/// Resolve the effective delivery config by applying the preference hierarchy:
/// Global Policy → Service/Type Defaults → User Service Pref → User Type Pref → Mute Rules
pub fn resolve(
    user_prefs: &CachedUserPrefs,
    notif_type: &CachedNotificationType,
    level_severity: Option<i32>,
    default_channels: &[String],
) -> ResolvedDeliveryConfig {
    // 1. Global user toggle
    if !user_prefs.global_enabled {
        return ResolvedDeliveryConfig {
            should_deliver: false,
            channels: vec![],
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
            return ResolvedDeliveryConfig {
                should_deliver: false,
                channels: vec![],
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
            return ResolvedDeliveryConfig {
                should_deliver: false,
                channels: vec![],
                ttl_seconds: notif_type.default_ttl_seconds,
                escalation_interval_seconds: notif_type.escalation_interval_seconds,
                max_escalations: notif_type.max_escalations,
                skip_reason: Some("service disabled by user preference".into()),
            };
        }

        // Check min severity threshold
        if let (Some(min_sev), Some(level_sev)) = (sp.min_severity, level_severity) {
            if level_sev < min_sev {
                return ResolvedDeliveryConfig {
                    should_deliver: false,
                    channels: vec![],
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
            return ResolvedDeliveryConfig {
                should_deliver: false,
                channels: vec![],
                ttl_seconds: notif_type.default_ttl_seconds,
                escalation_interval_seconds: notif_type.escalation_interval_seconds,
                max_escalations: notif_type.max_escalations,
                skip_reason: Some("notification type disabled by user preference".into()),
            };
        }
    }

    // 5. Resolve channels: type pref > service pref > notification type defaults > global defaults
    let channels = if let Some(tp) = type_pref {
        if let Some(ref ch) = tp.channels {
            if !ch.is_empty() {
                ch.clone()
            } else {
                resolve_channels_from_service(service_pref, notif_type, default_channels)
            }
        } else {
            resolve_channels_from_service(service_pref, notif_type, default_channels)
        }
    } else {
        resolve_channels_from_service(service_pref, notif_type, default_channels)
    };

    ResolvedDeliveryConfig {
        should_deliver: true,
        channels,
        ttl_seconds: notif_type.default_ttl_seconds,
        escalation_interval_seconds: notif_type.escalation_interval_seconds,
        max_escalations: notif_type.max_escalations,
        skip_reason: None,
    }
}

fn resolve_channels_from_service(
    service_pref: Option<&CachedServicePref>,
    notif_type: &CachedNotificationType,
    default_channels: &[String],
) -> Vec<String> {
    if let Some(sp) = service_pref {
        if !sp.channels.is_empty() {
            return sp.channels.clone();
        }
    }
    if !notif_type.default_channels.is_empty() {
        return notif_type.default_channels.clone();
    }
    default_channels.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CachedMuteRule, CachedTypePref};
    use uuid::Uuid;

    fn make_notif_type() -> CachedNotificationType {
        CachedNotificationType {
            id: Uuid::new_v4(),
            service_id: Uuid::new_v4(),
            type_key: "test".into(),
            json_schema: serde_json::json!({}),
            default_channels: vec!["email".into()],
            default_ttl_seconds: Some(3600),
            escalation_interval_seconds: None,
            max_escalations: 0,
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
        let result = resolve(&prefs, &make_notif_type(), None, &["email".into()]);
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
        let result = resolve(&prefs, &nt, None, &["email".into()]);
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
            channels: vec![],
        });
        let result = resolve(&prefs, &nt, Some(2), &["email".into()]);
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
            channels: vec![],
        });
        let result = resolve(&prefs, &nt, Some(5), &["email".into()]);
        assert!(result.should_deliver);
    }

    #[test]
    fn test_type_pref_channels_override() {
        let nt = make_notif_type();
        let mut prefs = make_user_prefs();
        prefs.type_prefs.push(CachedTypePref {
            notification_type_id: nt.id,
            enabled: true,
            channels: Some(vec!["push".into(), "sms".into()]),
        });
        let result = resolve(&prefs, &nt, None, &["email".into()]);
        assert!(result.should_deliver);
        assert_eq!(result.channels, vec!["push", "sms"]);
    }

    #[test]
    fn test_default_channels_fallback() {
        let prefs = make_user_prefs();
        let mut nt = make_notif_type();
        nt.default_channels = vec![];
        let result = resolve(&prefs, &nt, None, &["webhook".into()]);
        assert!(result.should_deliver);
        assert_eq!(result.channels, vec!["webhook"]);
    }
}
