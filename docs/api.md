---
title: "Estafeta gRPC API Reference"
description: "Reference for the five gRPC services exposed under the estafeta.v1 package, including authentication and authorization."
updated_at: "2026-07-28"
---

# Estafeta gRPC API Reference

Estafeta exposes five gRPC services under the `estafeta.v1` package. All
services require authentication and are served on a single port (default
50051).

## Authentication

Every RPC requires a valid JWT passed as gRPC metadata:

```
authorization: Bearer <token>
```

Tokens are validated against a JWKS endpoint (Ory Hydra). The interceptor
extracts the `sub` claim as the caller identity and the `scp` (or `scope`)
claim as a list of scopes.

### Keto Permissions

Fine-grained authorization is enforced through Ory Keto relation tuples in
the `estafeta` namespace:

| Check | Object | Relation | Used by |
|---|---|---|---|
| Admin access | `admin` | `access` | AdminService (all RPCs), SchemaRegistryService |
| Service send | `services/{slug}` | `send` | NotificationService.SendNotification |

If a permission check fails the RPC returns `PERMISSION_DENIED`.

---

## Services

### 1. NotificationService

Core service for ingesting and managing notifications in the unified inbox.

| RPC | Request | Response | Description |
|---|---|---|---|
| `SendNotification` | `SendNotificationRequest` | `SendNotificationResponse` | Send a single notification. Validates the payload against the registered JSON schema, then publishes to JetStream for async processing. Requires `send` permission on the service. |
| `SendNotificationBatch` | `SendNotificationBatchRequest` | `SendNotificationBatchResponse` | Send multiple notifications in one call. Each item is processed independently; `failed_count` reports how many failed. |
| `GetNotification` | `GetNotificationRequest` | `Notification` | Fetch a notification by ID. Users may only retrieve their own notifications. |
| `ListNotifications` | `ListNotificationsRequest` | `ListNotificationsResponse` | List notifications for the authenticated user with optional filters on state, service slugs, notification types, and group key. Supports pagination (max page size 100). |
| `MarkSeen` | `MarkSeenRequest` | `MarkSeenResponse` | Mark notifications as seen (transitions unseen to unread). Called when the notification dropdown opens. If no IDs are provided, marks all unseen notifications as seen. Returns the count of affected notifications. |
| `MarkRead` | `MarkReadRequest` | `google.protobuf.Empty` | Mark one or more notifications as read. Only the recipient can do this. |
| `MarkUnread` | `MarkUnreadRequest` | `google.protobuf.Empty` | Mark one or more notifications as unread. |
| `Snooze` | `SnoozeRequest` | `google.protobuf.Empty` | Snooze a notification until a given timestamp. The lifecycle scheduler wakes it automatically. |
| `Dismiss` | `DismissRequest` | `google.protobuf.Empty` | Dismiss one or more notifications. Dismissed is a terminal state. |
| `DismissAllInGroup` | `DismissAllInGroupRequest` | `DismissAllInGroupResponse` | Dismiss all notifications from a given service. Returns the count of dismissed notifications. |
| `GetUnseenCount` | `GetUnseenCountRequest` | `UnseenCountResponse` | Get the unseen count (drives the bell badge). Returns total and per-service breakdown. Optionally filtered by service slugs. |
| `GetUnreadCount` | `GetUnreadCountRequest` | `UnreadCountResponse` | Get the unread count. Returns total and per-service breakdown. Optionally filtered by service slugs. |

#### Key Messages

**SendNotificationRequest:**

| Field | Type | Description |
|---|---|---|
| `service_slug` | string | Registered producer service slug |
| `notification_type` | string | Type key registered in the schema registry |
| `recipient_user_id` | string | Target user ID |
| `level` | string | Severity level key (e.g., "critical", "info") |
| `payload` | google.protobuf.Struct | Notification payload, validated against the type's JSON schema |
| `idempotency_key` | string | Optional deduplication key |
| `group_key` | string | Optional grouping key for threading |
| `ttl_seconds` | int32 | Optional TTL override |
| `metadata` | map<string, string> | Arbitrary key-value metadata |
| `action_url` | string | Deep link URL navigated to when the notification is clicked |
| `icon` | string | Material Symbol icon name for display (overrides type default) |

**Notification:**

| Field | Type | Description |
|---|---|---|
| `id` | string | Notification UUID |
| `service_slug` | string | Producer service |
| `notification_type` | string | Type key |
| `level` | string | Severity level key |
| `state` | NotificationState | Current lifecycle state |
| `payload` | google.protobuf.Struct | Notification data |
| `group_key` | string | Grouping key |
| `metadata` | map<string, string> | Arbitrary metadata |
| `created_at` | Timestamp | When the notification was created |
| `seen_at` | Timestamp | When the notification was first seen (appeared in viewport) |
| `read_at` | Timestamp | When the notification was marked as read |
| `snoozed_until` | Timestamp | Snooze wake-up time (if snoozed) |
| `expires_at` | Timestamp | TTL expiration time |
| `escalation_count` | int32 | Number of times the notification has been escalated |
| `recipient_user_id` | string | Target user |
| `action_url` | string | Deep link URL |
| `icon` | string | Material Symbol icon name |

### 2. SchemaRegistryService

Manages notification types and severity levels. Most RPCs require admin
access via Keto.

| RPC | Request | Response | Description |
|---|---|---|---|
| `RegisterType` | `RegisterTypeRequest` | `NotificationType` | Register a new notification type with a JSON schema, TTL, escalation settings, escalation action, and default icon. |
| `UpdateType` | `UpdateTypeRequest` | `NotificationType` | Update an existing type. Can toggle `enabled`. |
| `GetType` | `GetTypeRequest` | `NotificationType` | Retrieve a type by service slug and type key. |
| `ListTypes` | `ListTypesRequest` | `ListTypesResponse` | List types, optionally filtered by service slug. Paginated. |
| `RegisterLevel` | `RegisterLevelRequest` | `NotificationLevel` | Register a severity level with a key, severity integer, color, and icon. |
| `UpdateLevel` | `UpdateLevelRequest` | `NotificationLevel` | Update an existing level. |
| `ListLevels` | `ListLevelsRequest` | `ListLevelsResponse` | List all levels for a service. |
| `ValidatePayload` | `ValidatePayloadRequest` | `ValidatePayloadResponse` | Dry-run payload validation against a type's JSON schema without sending. Returns `valid` and a list of `errors`. |

#### RegisterTypeRequest Fields

| Field | Type | Description |
|---|---|---|
| `service_slug` | string | Parent service slug |
| `type_key` | string | Unique type identifier within the service |
| `display_name` | string | Human-readable name |
| `description` | string | Optional description |
| `json_schema` | google.protobuf.Struct | JSON Schema for payload validation |
| `default_ttl_seconds` | int32 | Default TTL (0 = no expiry) |
| `escalation_interval_seconds` | int32 | Time between escalations (0 = no escalation) |
| `max_escalations` | int32 | Maximum escalation cycles |
| `escalation_action` | EscalationAction | What happens on escalation (resurface, bump, or elevate) |
| `default_icon` | string | Default Material Symbol icon name for this type |

### 3. UserConfigService

Lets authenticated users manage their own notification preferences. No admin
permissions required -- the user ID is taken from the JWT `sub` claim.

| RPC | Request | Response | Description |
|---|---|---|---|
| `GetPreferences` | `GetPreferencesRequest` | `UserPreferences` | Get the user's global preferences plus all service and type overrides. |
| `UpdatePreferences` | `UpdatePreferencesRequest` | `UserPreferences` | Set global preferences: enabled toggle, catch-up mode, and sort mode. |
| `SetServicePreference` | `SetServicePreferenceRequest` | `ServicePreference` | Override per-service settings: enabled flag and minimum severity threshold. |
| `DeleteServicePreference` | `DeleteServicePreferenceRequest` | `google.protobuf.Empty` | Remove a per-service override, reverting to defaults. |
| `SetTypePreference` | `SetTypePreferenceRequest` | `TypePreference` | Override per notification type: enabled flag. |
| `DeleteTypePreference` | `DeleteTypePreferenceRequest` | `google.protobuf.Empty` | Remove a per-type override. |
| `ListMuteRules` | `ListMuteRulesRequest` | `ListMuteRulesResponse` | List all active mute rules. |
| `CreateMuteRule` | `CreateMuteRuleRequest` | `MuteRule` | Mute a service or type until a given timestamp. |
| `DeleteMuteRule` | `DeleteMuteRuleRequest` | `google.protobuf.Empty` | Delete a mute rule. |

#### UserPreferences Fields

| Field | Type | Description |
|---|---|---|
| `global_enabled` | bool | Master toggle. When false, all notifications are suppressed. |
| `catch_up_mode` | string | How to handle reconnect: `"since_last_seen"` or `"all_unseen"` |
| `sort_mode` | string | Default inbox sort: `"chronological"` or `"priority"` |
| `service_preferences` | repeated ServicePreference | Per-service overrides |
| `type_preferences` | repeated TypePreference | Per-type overrides |

### 4. AdminService

Platform administration. Every RPC requires Keto admin access.

| RPC | Request | Response | Description |
|---|---|---|---|
| `RegisterService` | `RegisterServiceRequest` | `Service` | Register a new producer service with a unique slug. |
| `UpdateService` | `UpdateServiceRequest` | `Service` | Update display name and description. |
| `ListServices` | `ListServicesRequest` | `ListServicesResponse` | Paginated list of all registered services. |
| `DisableService` | `DisableServiceRequest` | `google.protobuf.Empty` | Disable a service. Disabled services reject new notifications with `FAILED_PRECONDITION`. |
| `EnableService` | `EnableServiceRequest` | `google.protobuf.Empty` | Re-enable a previously disabled service. |
| `SetGlobalPolicy` | `SetGlobalPolicyRequest` | `GlobalPolicy` | Set platform-wide policy: rate limits, max TTL, and max escalations. |
| `GetGlobalPolicy` | `GetGlobalPolicyRequest` | `GlobalPolicy` | Retrieve the current global policy. |
| `ResurfaceNotification` | `ResurfaceNotificationRequest` | `google.protobuf.Empty` | Manually trigger an escalation action on a notification. Specify the `EscalationAction` to apply (resurface, bump, or elevate). |

#### GlobalPolicy Fields

| Field | Type | Description |
|---|---|---|
| `max_notifications_per_user_per_hour` | int32 | Rate limit on notifications per user per hour |
| `max_ttl_seconds` | int32 | Upper bound on notification TTL |
| `max_escalations` | int32 | Maximum number of escalation cycles |
| `rate_limit_per_service_per_second` | int32 | Rate limit per producer service per second |

### 5. StreamingService

Real-time server-streaming for live UIs.

| RPC | Request | Response | Description |
|---|---|---|---|
| `Subscribe` | `SubscribeRequest` | `stream NotificationEvent` | Open a server-side stream of events. Filter by service slugs. Set `include_state_changes` to receive state transition events. |

#### SubscribeRequest Fields

| Field | Type | Description |
|---|---|---|
| `service_slugs` | repeated string | Filter to specific services (empty = all) |
| `include_state_changes` | bool | Include state change events (seen, read, dismissed, etc.) |

#### NotificationEvent

Events are delivered as a `NotificationEvent` oneof:

- **`NewNotification`** -- a new notification was created for the user. Includes `action_url` and `icon`.
- **`StateChange`** -- a notification moved between states (includes old state, new state, and timestamp).
- **`UnseenCountUpdate`** -- the user's unseen count changed. Includes total count and per-service breakdown. Sent after `MarkSeen` calls and when new notifications arrive.

Events are published over NATS Core to `rt.user.{user_id}` and bridged
into the gRPC stream.

---

## Common Types

### Pagination

```protobuf
message Pagination {
  int32 page_size = 1;    // max 100
  string page_token = 2;  // opaque; currently an integer offset
}

message PaginatedResponse {
  string next_page_token = 1;  // empty when no more pages
  int64 total_count = 2;
}
```

### NotificationState

| Value | Name | Description |
|---|---|---|
| 0 | `UNSPECIFIED` | Default / unknown |
| 1 | `UNSEEN` | Never appeared in the user's viewport. Drives the bell badge count. |
| 2 | `UNREAD` | Appeared in dropdown but not explicitly marked as read |
| 3 | `READ` | Marked as read by the user |
| 4 | `SNOOZED` | Snoozed until a future time |
| 5 | `DISMISSED` | Terminal -- user dismissed |
| 6 | `EXPIRED` | Terminal -- TTL elapsed |

### EscalationAction

| Value | Name | Description |
|---|---|---|
| 0 | `UNSPECIFIED` | Default / unknown |
| 1 | `RESURFACE` | Move the notification back to unseen (re-triggers bell badge) |
| 2 | `BUMP` | Bump the notification to the top of the list by updating its timestamp |
| 3 | `ELEVATE` | Elevate the notification to a higher visual priority level |

---

## Error Handling

Estafeta uses standard gRPC status codes:

| Code | When |
|---|---|
| `UNAUTHENTICATED` | Missing or invalid `authorization` metadata, expired JWT, unknown signing key |
| `PERMISSION_DENIED` | Keto check fails (not admin, no service send permission), or accessing another user's notification |
| `NOT_FOUND` | Service slug, notification type, or notification ID does not exist |
| `INVALID_ARGUMENT` | Malformed UUID, missing required field (`snooze_until`), or payload fails JSON schema validation |
| `FAILED_PRECONDITION` | Sending to a disabled service or disabled notification type |
| `INTERNAL` | Database errors, NATS publish failures, Keto service unavailable |
