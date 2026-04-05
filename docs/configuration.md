# Configuration Reference

All configuration is provided through environment variables prefixed with
`ESTAFETA_`. Nested structs use double underscores as separators (e.g.,
`ESTAFETA_SMTP__HOST`). Configuration is loaded at startup via
[Figment](https://docs.rs/figment/) with `Env::prefixed("ESTAFETA_").split("__")`.

---

## Core Configuration

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_GRPC_PORT` | u16 | `50051` | No | Port for the gRPC server |
| `ESTAFETA_DATABASE_URL` | String | -- | Yes | PostgreSQL connection string (e.g., `postgres://user:pass@host/dbname`) |
| `ESTAFETA_DATABASE_MAX_CONNECTIONS` | u32 | `10` | No | Maximum connections in the SQLx pool |
| `ESTAFETA_NATS_URL` | String | -- | Yes | NATS server URL (e.g., `nats://localhost:4222`) |
| `ESTAFETA_HYDRA_JWKS_URL` | String | -- | Yes | Ory Hydra JWKS endpoint for JWT validation |
| `ESTAFETA_JWT_ISSUER` | String | None | No | If set, the `iss` claim in JWTs is validated against this value |
| `ESTAFETA_KETO_URL` | String | -- | Yes | Ory Keto read API base URL (e.g., `http://keto:4466`) |
| `ESTAFETA_LOG_LEVEL` | String | `info` | No | `tracing` env-filter directive. Supports per-module levels (e.g., `estafeta_server::delivery=debug,info`) |

---

## SMTP Configuration

Used by the email delivery channel. The email worker only starts if this
section is present. All variables use the `ESTAFETA_SMTP__` prefix.

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_SMTP__HOST` | String | -- | Yes | SMTP server hostname |
| `ESTAFETA_SMTP__PORT` | u16 | `587` | No | SMTP server port |
| `ESTAFETA_SMTP__USERNAME` | String | None | No | SMTP auth username |
| `ESTAFETA_SMTP__PASSWORD` | String | None | No | SMTP auth password |
| `ESTAFETA_SMTP__FROM_ADDRESS` | String | -- | Yes | Sender email address (e.g., `noreply@example.com`) |

If username and password are both provided, the SMTP transport authenticates
with those credentials. Otherwise it connects without authentication.

---

## SES Configuration

Reserved for future native Amazon SES API integration. Currently, SES can be
used through the SMTP channel by pointing `SMTP__HOST` at the SES SMTP
endpoint.

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_SES__REGION` | String | -- | Yes | AWS region (e.g., `us-east-1`) |
| `ESTAFETA_SES__FROM_ADDRESS` | String | -- | Yes | Verified SES sender address |

---

## FCM Configuration

Firebase Cloud Messaging for Android push notifications.

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_FCM__CREDENTIALS_PATH` | String | -- | Yes | Path to the FCM service account JSON file |

---

## APNs Configuration

Apple Push Notification service for iOS push notifications.

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_APNS__KEY_PATH` | String | -- | Yes | Path to the `.p8` authentication key file |
| `ESTAFETA_APNS__KEY_ID` | String | -- | Yes | Key ID from the Apple Developer portal |
| `ESTAFETA_APNS__TEAM_ID` | String | -- | Yes | Apple Developer team ID |
| `ESTAFETA_APNS__TOPIC` | String | -- | Yes | App bundle identifier (e.g., `com.example.app`) |
| `ESTAFETA_APNS__SANDBOX` | bool | `false` | No | Use the APNs sandbox environment for development |

---

## SNS Configuration

Amazon SNS for SMS delivery.

| Variable | Type | Default | Required | Description |
|---|---|---|---|---|
| `ESTAFETA_SNS__REGION` | String | -- | Yes | AWS region (e.g., `us-east-1`) |

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
| `default_channels` | DeliveryChannel[] | Channels used when a notification type does not specify its own |
| `rate_limit_per_service_per_second` | int32 | Rate limit per producer service per second |

Retrieve the current policy with `AdminService.GetGlobalPolicy`.

---

## User Preferences (Runtime)

Users control their own notification preferences through the
`UserConfigService` RPCs. These are stored in the database, not in
environment variables.

### Preference Resolution Order

When deciding whether and how to deliver a notification, the processor
applies preferences in this order:

1. **Global enabled** -- if `false`, all notifications are suppressed.
2. **Mute rules** -- if a matching active mute rule exists, the
   notification is suppressed.
3. **Service preference** -- per-service enabled flag, minimum severity
   filter, and channel overrides.
4. **Type preference** -- per notification type enabled flag and channel
   overrides (most specific wins).
5. **Type defaults** -- the channels and TTL configured on the
   notification type in the schema registry.
6. **Global policy defaults** -- fallback channels from the global policy.

### Channel Configuration

Users set their contact information via `UpdateChannelConfig`:

| Field | Description |
|---|---|
| `email_address` | Destination for email delivery |
| `phone_number` | Destination for SMS delivery |
| `webhook_url` | HTTP endpoint for webhook delivery |
| `webhook_secret` | Shared secret for HMAC webhook signing |

### Device Registration

Push notification devices are registered individually via `RegisterDevice`
with a `device_id`, `platform`, and `push_token`.

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

# SMTP (email delivery)
ESTAFETA_SMTP__HOST=smtp.mailgun.org
ESTAFETA_SMTP__PORT=587
ESTAFETA_SMTP__USERNAME=postmaster@mg.example.com
ESTAFETA_SMTP__PASSWORD=secret
ESTAFETA_SMTP__FROM_ADDRESS=notifications@example.com

# SES (future native integration)
# ESTAFETA_SES__REGION=us-east-1
# ESTAFETA_SES__FROM_ADDRESS=notifications@example.com

# FCM (Android push)
# ESTAFETA_FCM__CREDENTIALS_PATH=/etc/estafeta/fcm-service-account.json

# APNs (iOS push)
# ESTAFETA_APNS__KEY_PATH=/etc/estafeta/AuthKey.p8
# ESTAFETA_APNS__KEY_ID=ABC123
# ESTAFETA_APNS__TEAM_ID=DEF456
# ESTAFETA_APNS__TOPIC=com.example.app
# ESTAFETA_APNS__SANDBOX=false

# SNS (SMS)
# ESTAFETA_SNS__REGION=us-east-1
```
