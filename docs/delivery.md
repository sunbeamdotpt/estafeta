# Delivery Channels

Estafeta delivers notifications through four channels: Email, Push, SMS, and
Webhook. Each channel is implemented as a struct that satisfies the
`DeliveryChannel` trait. Delivery is asynchronous -- notifications are
published to a NATS JetStream subject and processed by per-channel worker
tasks.

## Channel Abstraction

All channels implement the trait defined in
`crates/estafeta-server/src/delivery/channel.rs`:

```rust
#[async_trait]
pub trait DeliveryChannel: Send + Sync {
    fn channel_type(&self) -> ChannelType;

    async fn deliver(
        &self,
        notification: &NotificationRow,
        recipient: &RecipientInfo,
    ) -> Result<DeliveryResult, DeliveryError>;
}
```

`RecipientInfo` is assembled at delivery time from the user's channel config
and device registrations stored in the database.

### Error Types

| Variant | Meaning | Worker behavior |
|---|---|---|
| `Transient` | Temporary failure (timeout, 5xx) | Retry with exponential backoff |
| `Permanent` | Unrecoverable (bad address, 4xx) | Mark attempt as `failed` immediately |
| `RateLimited(Duration)` | Provider rate limit | Retry after the indicated duration |
| `MissingRecipientInfo` | No contact info for channel | Mark attempt as `skipped` |

---

## Email Channel

**Implementation:** `crates/estafeta-server/src/delivery/email.rs`

Uses `lettre` with `AsyncSmtpTransport` for SMTP delivery. Optionally
authenticable with username/password credentials.

### Payload Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `subject` | string | `"Notification"` | Email subject line |
| `body` | string | `""` (empty) | Plain-text email body |

### Configuration

Requires `ESTAFETA_SMTP__*` environment variables (see
[configuration.md](configuration.md)). The email worker only starts if SMTP
configuration is present.

### SES Support

Amazon SES can be used by pointing the SMTP host at the SES SMTP endpoint
(e.g., `email-smtp.us-east-1.amazonaws.com`) and setting the SES SMTP
credentials as username/password. A dedicated `SesConfig` section exists in
the config struct for future native SES API integration.

---

## Push Channel

**Implementation:** `crates/estafeta-server/src/delivery/push.rs`

Stub implementation that will be wired to Firebase Cloud Messaging (FCM) and
Apple Push Notification service (APNs).

### Payload Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `title` | string | `"Notification"` | Push notification title |
| `body` | string | `""` (empty) | Push notification body |

### Device Registration

Users register devices through the `UserConfigService.RegisterDevice` RPC,
providing:

- `device_id` -- unique identifier for the device
- `platform` -- `"ios"`, `"android"`, or other platform string
- `push_token` -- the FCM registration token or APNs device token

The push worker iterates over all registered devices for the recipient and
sends to each. If no devices are registered, the attempt is marked `skipped`.

### Configuration

FCM requires `ESTAFETA_FCM__CREDENTIALS_PATH` pointing to a service account
JSON file. APNs requires key path, key ID, team ID, topic, and an optional
sandbox flag. See [configuration.md](configuration.md).

---

## SMS Channel

**Implementation:** `crates/estafeta-server/src/delivery/sms.rs`

Stub implementation to be wired to AWS SNS, Twilio, or another SMS provider.

### Payload Fields

| Field | Type | Default | Description |
|---|---|---|---|
| `sms_body` | string | falls back to `body` | SMS message text |
| `body` | string | `"You have a new notification"` | Fallback if `sms_body` is not present |

The `sms_body` field lets you provide a shorter message tailored to SMS
character limits while keeping a longer `body` for other channels.

### Recipient Requirement

The user must have a `phone_number` set in their channel config. If missing,
the attempt is marked `skipped`.

### Configuration

Requires `ESTAFETA_SNS__REGION` for AWS SNS. See
[configuration.md](configuration.md).

---

## Webhook Channel

**Implementation:** `crates/estafeta-server/src/delivery/webhook.rs`

Sends an HTTP POST request with a JSON body to the user's configured
`webhook_url`.

### Request Body

```json
{
  "notification_id": "uuid",
  "service_id": "uuid",
  "notification_type_id": "uuid",
  "state": "unread",
  "payload": { ... },
  "metadata": { ... },
  "created_at": "2026-04-04T12:00:00Z"
}
```

### HMAC Signing

If the user has set a `webhook_secret` in their channel config, the request
includes an `X-Estafeta-Signature` header containing a hash-based signature
of the JSON body. Receivers should validate this header to verify that the
request originated from Estafeta.

### Response Handling

| HTTP Status | Interpretation |
|---|---|
| 2xx | Success |
| 429 or 5xx | Transient failure -- will retry |
| Other 4xx | Permanent failure -- no retry |

Timeout and connection errors are also treated as transient.

---

## Retry Strategy

Transient failures trigger exponential backoff retries managed by the delivery
worker (`crates/estafeta-server/src/delivery/worker.rs`).

| Attempt | Backoff |
|---|---|
| 1 | 30 seconds |
| 2 | 2 minutes |
| 3 | 10 minutes |
| 4 | 1 hour |
| 5 | 2 hours |

After 5 failed attempts the delivery is marked as permanently `failed`.

Rate-limited errors (`DeliveryError::RateLimited`) use the duration returned
by the provider rather than the fixed backoff schedule.

---

## Delivery Tracking

Every delivery attempt is recorded in the `delivery_attempts` table.

### Statuses

| Status | Meaning |
|---|---|
| `pending` | Attempt created, awaiting worker pickup |
| `sent` | Channel accepted the message |
| `delivered` | Confirmed delivery (when provider supports it) |
| `failed` | Permanent failure or max retries exceeded |
| `skipped` | Missing recipient info for this channel |

Each attempt row stores:

- `attempt_number` -- incremented on each retry
- `channel` -- which delivery channel
- `status` -- one of the above
- `external_id` -- provider-assigned ID (if returned)
- `error_message` -- reason for failure or skip
- `next_retry_at` -- when the next retry is scheduled

---

## Delivery Flow

1. The notification processor resolves which channels to deliver through
   (based on type defaults, global policy, and user preferences).
2. For each channel, a `delivery_attempts` row is inserted with status
   `pending`.
3. A `DeliveryDispatchMessage` is published to
   `delivery.dispatch.{channel}` on the DELIVERY JetStream stream.
4. The per-channel delivery worker picks up the message, loads recipient
   info from the database, and calls the channel's `deliver` method.
5. On success the attempt status is updated to `sent`. On transient failure
   a retry is scheduled. On permanent failure or missing info the attempt
   is marked `failed` or `skipped`.
6. The JetStream message is ACKed on success or terminal failure, and
   NAKed on transient failure to trigger redelivery.

---

## Adding a New Delivery Channel

1. Create a new file in `crates/estafeta-server/src/delivery/` (e.g.,
   `slack.rs`).
2. Implement the `DeliveryChannel` trait. Add a new variant to `ChannelType`
   in `channel.rs`.
3. Export the module from `delivery/mod.rs`.
4. Add a corresponding `DeliveryChannel` enum value in `common.proto` and
   add a filter subject to the DELIVERY stream in `nats/setup.rs`.
5. In `main.rs`, instantiate the channel and spawn a `DeliveryWorker` with
   a JetStream consumer filtered on `delivery.dispatch.{new_channel}`.
6. If the channel needs configuration, add a config struct to `config.rs`
   and corresponding `ESTAFETA_` environment variables.
