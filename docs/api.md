# Estafeta gRPC API Reference

Estafeta exposes five gRPC services under the `estafeta.v1` package. All services
require authentication and are served on a single port (default 50051).

## Authentication

Every RPC requires a valid JWT passed as gRPC metadata:

```
authorization: Bearer <token>
```

Tokens are validated against a JWKS endpoint (Ory Hydra). The interceptor
extracts the `sub` claim as the caller identity and the `scp` (or `scope`)
claim as a list of scopes.

### Keto Permissions

Fine-grained authorization is enforced through Ory Keto relation tuples in the
`estafeta` namespace:

| Check | Object | Relation | Used by |
|---|---|---|---|
| Admin access | `admin` | `access` | AdminService (all RPCs), SchemaRegistryService |
| Service send | `services/{slug}` | `send` | NotificationService.SendNotification |

If a permission check fails the RPC returns `PERMISSION_DENIED`.

---

## Services

### 1. NotificationService

Core service for sending and managing notifications.

| RPC | Request | Response | Description |
|---|---|---|---|
| `SendNotification` | `SendNotificationRequest` | `SendNotificationResponse` | Send a single notification. Validates the payload against the registered JSON schema, then publishes to JetStream for async processing. Requires `send` permission on the service. |
| `SendNotificationBatch` | `SendNotificationBatchRequest` | `SendNotificationBatchResponse` | Send multiple notifications in one call. Each item is processed independently; `failed_count` reports how many failed. |
| `GetNotification` | `GetNotificationRequest` | `Notification` | Fetch a notification by ID. Users may only retrieve their own notifications. |
| `ListNotifications` | `ListNotificationsRequest` | `ListNotificationsResponse` | List notifications for the authenticated user with optional filters on state, service slugs, notification types, and group key. Supports pagination (max page size 100). |
| `MarkRead` | `MarkReadRequest` | `google.protobuf.Empty` | Mark one or more notifications as read. Only the recipient can do this. |
| `MarkUnread` | `MarkUnreadRequest` | `google.protobuf.Empty` | Mark one or more notifications as unread. |
| `Snooze` | `SnoozeRequest` | `google.protobuf.Empty` | Snooze a notification until a given timestamp. The lifecycle scheduler wakes it automatically. |
| `Dismiss` | `DismissRequest` | `google.protobuf.Empty` | Dismiss one or more notifications. Dismissed is a terminal state. |
| `GetUnreadCount` | `GetUnreadCountRequest` | `UnreadCountResponse` | Get the total unread count and per-service breakdown. |

### 2. SchemaRegistryService

Manages notification types and severity levels. Most RPCs require admin access
via Keto.

| RPC | Request | Response | Description |
|---|---|---|---|
| `RegisterType` | `RegisterTypeRequest` | `NotificationType` | Register a new notification type with a JSON schema, default channels, TTL, and escalation settings. |
| `UpdateType` | `UpdateTypeRequest` | `NotificationType` | Update an existing type. Can toggle `enabled`. |
| `GetType` | `GetTypeRequest` | `NotificationType` | Retrieve a type by service slug and type key. |
| `ListTypes` | `ListTypesRequest` | `ListTypesResponse` | List types, optionally filtered by service slug. Paginated. |
| `RegisterLevel` | `RegisterLevelRequest` | `NotificationLevel` | Register a severity level with a key, severity integer, color, and icon. |
| `UpdateLevel` | `UpdateLevelRequest` | `NotificationLevel` | Update an existing level. |
| `ListLevels` | `ListLevelsRequest` | `ListLevelsResponse` | List all levels for a service. |
| `ValidatePayload` | `ValidatePayloadRequest` | `ValidatePayloadResponse` | Dry-run payload validation against a type's JSON schema without sending. Returns `valid` and a list of `errors`. |

### 3. UserConfigService

Lets authenticated users manage their own notification preferences, devices,
and channel contact information. No admin permissions required -- the user ID
is taken from the JWT `sub` claim.

| RPC | Request | Response | Description |
|---|---|---|---|
| `GetPreferences` | `GetPreferencesRequest` | `UserPreferences` | Get the user's global enabled flag plus all service and type overrides. |
| `UpdatePreferences` | `UpdatePreferencesRequest` | `UserPreferences` | Set the global `enabled` toggle. When false, all notifications are suppressed. |
| `SetServicePreference` | `SetServicePreferenceRequest` | `ServicePreference` | Override per-service: enabled, min severity, and channel selection. |
| `DeleteServicePreference` | `DeleteServicePreferenceRequest` | `google.protobuf.Empty` | Remove a per-service override, reverting to defaults. |
| `SetTypePreference` | `SetTypePreferenceRequest` | `TypePreference` | Override per notification type: enabled and channel selection. |
| `DeleteTypePreference` | `DeleteTypePreferenceRequest` | `google.protobuf.Empty` | Remove a per-type override. |
| `ListMuteRules` | `ListMuteRulesRequest` | `ListMuteRulesResponse` | List all active mute rules. |
| `CreateMuteRule` | `CreateMuteRuleRequest` | `MuteRule` | Mute a service or type until a given timestamp. |
| `DeleteMuteRule` | `DeleteMuteRuleRequest` | `google.protobuf.Empty` | Delete a mute rule. |
| `RegisterDevice` | `RegisterDeviceRequest` | `Device` | Register a device for push notifications (device_id, platform, push_token). |
| `UnregisterDevice` | `UnregisterDeviceRequest` | `google.protobuf.Empty` | Remove a device registration. |
| `UpdateChannelConfig` | `UpdateChannelConfigRequest` | `ChannelConfig` | Set contact info: email address, phone number, webhook URL, webhook secret. |
| `GetChannelConfig` | `GetChannelConfigRequest` | `ChannelConfig` | Retrieve current contact info. The webhook secret is never returned; only `webhook_secret_set` is exposed. |

### 4. AdminService

Platform administration. Every RPC requires Keto admin access.

| RPC | Request | Response | Description |
|---|---|---|---|
| `RegisterService` | `RegisterServiceRequest` | `Service` | Register a new producer service with a unique slug. |
| `UpdateService` | `UpdateServiceRequest` | `Service` | Update display name and description. |
| `ListServices` | `ListServicesRequest` | `ListServicesResponse` | Paginated list of all registered services. |
| `DisableService` | `DisableServiceRequest` | `google.protobuf.Empty` | Disable a service. Disabled services reject new notifications with `FAILED_PRECONDITION`. |
| `EnableService` | `EnableServiceRequest` | `google.protobuf.Empty` | Re-enable a previously disabled service. |
| `SetGlobalPolicy` | `SetGlobalPolicyRequest` | `GlobalPolicy` | Set platform-wide policy: rate limits, max TTL, max escalations, default channels. |
| `GetGlobalPolicy` | `GetGlobalPolicyRequest` | `GlobalPolicy` | Retrieve the current global policy. |
| `ReplayNotification` | `ReplayNotificationRequest` | `google.protobuf.Empty` | Re-trigger delivery for an existing notification. Optionally specify which channels to replay; defaults to all original channels. |

### 5. StreamingService

Real-time server-streaming for live UIs.

| RPC | Request | Response | Description |
|---|---|---|---|
| `Subscribe` | `SubscribeRequest` | `stream NotificationEvent` | Open a server-side stream of events. Filter by service slugs and states. Set `include_state_changes` to receive state transition events. |

Events are delivered as a `NotificationEvent` oneof:

- **`NewNotification`** -- a new notification was created for the user.
- **`StateChange`** -- a notification moved between states (includes old and new state).
- **`UnreadCountUpdate`** -- the user's unread count changed.

Events are published over NATS Core to `rt.user.{user_id}` and bridged into
the gRPC stream.

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
| 1 | `UNREAD` | Newly delivered |
| 2 | `READ` | Marked as read |
| 3 | `SNOOZED` | Snoozed until a future time |
| 4 | `DISMISSED` | Terminal -- user dismissed |
| 5 | `EXPIRED` | Terminal -- TTL elapsed |

### DeliveryChannel

| Value | Name |
|---|---|
| 0 | `UNSPECIFIED` |
| 1 | `EMAIL` |
| 2 | `PUSH` |
| 3 | `SMS` |
| 4 | `WEBHOOK` |

### DeliveryStatus

| Value | Name |
|---|---|
| 0 | `UNSPECIFIED` |
| 1 | `PENDING` |
| 2 | `SENT` |
| 3 | `DELIVERED` |
| 4 | `FAILED` |
| 5 | `SKIPPED` |

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
