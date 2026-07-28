---
title: "Configuration Reference"
description: "All configuration is provided through environment variables prefixed with ESTAFETA_, loaded at startup via Figment."
updated_at: "2026-07-28"
---

# Configuration Reference

All configuration is provided through environment variables prefixed with
`ESTAFETA_`. Configuration is loaded at startup via
[Figment](https://docs.rs/figment/) with `Env::prefixed("ESTAFETA_").split("__")`.

---

## Environment Variables

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_GRPC_PORT` | u16 | `50051` | No | Port for the gRPC server |
| `ESTAFETA_DATABASE_URL` | String | -- | Yes | PostgreSQL connection string (e.g., `postgres://user:pass@host/dbname`) |
| `ESTAFETA_DATABASE_MAX_CONNECTIONS` | u32 | `10` | No | Maximum connections in the SQLx pool |
| `ESTAFETA_NATS_URL` | String | -- | Yes | NATS server URL (e.g., `nats://localhost:4222`) |
| `ESTAFETA_HYDRA_JWKS_URL` | String | -- | Yes | Ory Hydra JWKS endpoint for JWT validation |
| `ESTAFETA_JWT_ISSUER` | String | None | No | If set, the `iss` claim in JWTs is validated against this value |
| `ESTAFETA_KETO_URL` | String | -- | Yes | Ory Keto read API base URL (e.g., `http://keto:4466`) |
| `ESTAFETA_LOG_LEVEL` | String | `info` | No | `tracing` env-filter directive. Supports per-module levels (e.g., `estafeta_server::processor=debug,info`) |

---

## Global Policy (Runtime)

Global policies are not set through environment variables. They are managed
at runtime through the `AdminService.SetGlobalPolicy` RPC and stored in the
database. These settings apply platform-wide.

| Field | Type | Description |
|---|---|---|
| `max_notifications_per_user_per_hour` | int32 | Rate limit on notifications per user per hour |
| `max_ttl_seconds` | int32 | Upper bound on notification TTL |
| `max_escalations` | int32 | Maximum number of escalation cycles |
| `rate_limit_per_service_per_second` | int32 | Rate limit per producer service per second |

Retrieve the current policy with `AdminService.GetGlobalPolicy`.

---

## User Preferences (Runtime)

Users control their own notification preferences through the
`UserConfigService` RPCs. These are stored in the database, not in
environment variables.

### Preference Resolution Order

When deciding whether to accept a notification into the user's inbox, the
processor applies preferences in this order:

1. **Global enabled** -- if `false`, all notifications are suppressed.
2. **Mute rules** -- if a matching active mute rule exists, the notification
   is suppressed.
3. **Service preference** -- per-service enabled flag and minimum severity
   filter.
4. **Type preference** -- per notification type enabled flag (most specific
   wins).

### Global Preference Fields

| Field | Type | Description |
|---|---|---|
| `global_enabled` | bool | Master toggle. When false, all notifications are suppressed. |
| `catch_up_mode` | string | How to handle reconnect: `"since_last_seen"` or `"all_unseen"` |
| `sort_mode` | string | Default inbox sort: `"chronological"` or `"priority"` |

### Service Preference Fields

| Field | Type | Description |
|---|---|---|
| `service_slug` | string | Service to configure |
| `enabled` | bool | Enable/disable notifications from this service |
| `min_severity` | int32 | Minimum severity threshold (notifications with lower severity are suppressed) |

### Type Preference Fields

| Field | Type | Description |
|---|---|---|
| `service_slug` | string | Parent service |
| `type_key` | string | Notification type to configure |
| `enabled` | bool | Enable/disable this notification type |

### Mute Rules

Mute rules temporarily suppress notifications from a service or type:

| Field | Type | Description |
|---|---|---|
| `service_slug` | string | Service to mute |
| `notification_type` | string | Optional type within the service to mute |
| `muted_until` | Timestamp | When the mute expires (NULL = indefinite) |

---

## Example .env File

```bash
# Core
ESTAFETA_GRPC_PORT=50051
ESTAFETA_DATABASE_URL=postgres://estafeta:estafeta@localhost/estafeta
ESTAFETA_DATABASE_MAX_CONNECTIONS=20
ESTAFETA_NATS_URL=nats://localhost:4222
ESTAFETA_HYDRA_JWKS_URL=http://localhost:4444/.well-known/jwks.json
ESTAFETA_JWT_ISSUER=http://hydra:4444
ESTAFETA_KETO_URL=http://localhost:4466
ESTAFETA_LOG_LEVEL=info
```
