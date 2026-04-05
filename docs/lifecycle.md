# Notification Lifecycle

## State Machine

Every notification has a `state` column constrained to one of five values. The
state machine is implemented in `lifecycle/state_machine.rs` and enforced both in
Rust (via the `NotificationState::apply` method) and at the database level (via a
CHECK constraint and conditional UPDATE WHERE clauses).

```
                          +------------------+
                          |                  |
            +------------>|     Unread       |<-----------+
            |             |                  |            |
            |             +--+----+-----+----+            |
            |                |    |     |                  |
            |     MarkRead   |    |     |  Snooze          |  Wake
            |                v    |     v                  |
            |         +------+    |  +--------+           |
            |         |           |  |        |-----------+
            |         | Read      |  |Snoozed |
            |         |           |  |        |--+
            |         +---+--+----+  +---+----+  |
            |             |  |           |       |
            |  MarkUnread |  |           |       |
            +-------------+  |           |       |
                             |  Dismiss  |Expire |Expire
                      Dismiss|           |       |
                             v           v       |
                      +------+-----+  +--+------+|
                      |            |  |          ||
                      | Dismissed  |  | Expired  |+
                      | (terminal) |  | (terminal)|
                      +------------+  +-----------+
```

### Valid Transitions

| From State | Allowed Transitions                          |
|------------|----------------------------------------------|
| Unread     | MarkRead, Snooze, Dismiss, Expire            |
| Read       | MarkUnread, Dismiss, Expire                  |
| Snoozed    | Wake, Expire                                 |
| Dismissed  | (none -- terminal)                           |
| Expired    | (none -- terminal)                           |

Any transition not listed above returns an `InvalidTransition` error. The
`Transition::target()` method maps each transition to its destination state:

| Transition | Target State |
|------------|-------------|
| MarkRead   | Read        |
| MarkUnread | Unread      |
| Snooze     | Snoozed     |
| Wake       | Unread      |
| Dismiss    | Dismissed   |
| Expire     | Expired     |

Note that both `MarkUnread` and `Wake` transition to `Unread`, but from different
source states (Read and Snoozed, respectively).

## State Descriptions

### Unread

The initial state for all delivered notifications. The notification is visible and
has not been acknowledged by the user. This is also the state a notification returns
to after waking from snooze or being marked unread.

### Read

The notification has been acknowledged. Transition occurs when the user calls
`NotificationService.MarkRead`. Sets `read_at` to the current timestamp.

### Snoozed

The notification is temporarily hidden until `snoozed_until`. Transition occurs
when the user calls `NotificationService.Snooze` with a `snooze_until` timestamp.
The database SQL allows snoozing from either `unread` or `read` states:

```sql
UPDATE notifications
SET state = 'snoozed', snoozed_until = $3, updated_at = now()
WHERE recipient_user_id = $1 AND id = $2 AND state IN ('unread', 'read')
```

Note: the Rust state machine only allows Snooze from Unread, but the database
query also accepts Read. The database is the source of truth for this transition.

### Dismissed

The user has explicitly dismissed the notification. This is a terminal state with
no outbound transitions. Dismissal is allowed from Unread, Read, or Snoozed:

```sql
WHERE state IN ('unread', 'read', 'snoozed')
```

### Expired

The notification's TTL has elapsed. This is a terminal state set by the background
scheduler. Expiration applies to notifications in Unread or Read states.

## Lifecycle Operations

### Mark Read / Mark Unread

Batch operations that accept a list of notification IDs. The SQL uses conditional
WHERE clauses to only affect rows in the expected source state:

- **MarkRead**: `WHERE state = 'unread'` -- sets `state = 'read'` and `read_at = now()`.
- **MarkUnread**: `WHERE state = 'read'` -- sets `state = 'unread'` and `read_at = NULL`.

Both are scoped to `recipient_user_id = {JWT subject}`, preventing users from
modifying other users' notifications.

### Snooze with Timed Wake-up

The snooze operation:

1. The user calls `NotificationService.Snooze` with a notification ID and a
   `snooze_until` timestamp (protobuf `google.protobuf.Timestamp`).
2. The database sets `state = 'snoozed'` and `snoozed_until = {timestamp}`.
3. The background scheduler's snooze wake-up loop (every 30 seconds) runs:

```sql
UPDATE notifications
SET state = 'unread', snoozed_until = NULL, updated_at = now()
WHERE id IN (
    SELECT id FROM notifications
    WHERE state = 'snoozed' AND snoozed_until <= now()
    ORDER BY snoozed_until
    LIMIT 100
    FOR UPDATE SKIP LOCKED
)
RETURNING *
```

4. For each woken notification, a `StateChange` real-time event is published
   to `rt.user.{user_id}.state` via NATS Core.

### Dismiss

Batch operation accepting multiple notification IDs. Sets `state = 'dismissed'`
for all matching notifications in non-terminal states. No further lifecycle
transitions are possible after dismissal.

## TTL-based Expiration

### Computing `expires_at`

When the processor creates a notification, it computes:

```rust
let expires_at = config.ttl_seconds.map(|ttl| Utc::now() + Duration::seconds(ttl as i64));
```

The `ttl_seconds` value comes from the `ResolvedDeliveryConfig`, which inherits
`default_ttl_seconds` from the `notification_types` table. This is set when
registering the notification type via `SchemaRegistryService.RegisterType`.

If `default_ttl_seconds` is NULL (not set on the notification type), then
`expires_at` is NULL and the notification never expires automatically.

### Expiry Scheduler

The TTL expiry loop runs every 60 seconds:

```sql
UPDATE notifications
SET state = 'expired', updated_at = now()
WHERE id IN (
    SELECT id FROM notifications
    WHERE expires_at <= now() AND state IN ('unread', 'read')
    ORDER BY expires_at
    LIMIT 100
    FOR UPDATE SKIP LOCKED
)
RETURNING *
```

Only `unread` and `read` notifications are expired. Snoozed notifications are not
expired by this query (they have a separate index path). Dismissed notifications
are already terminal. A partial index `idx_notifications_expiry` supports this query.

After expiring each batch, `StateChange` events are published via NATS Core.

## Priority Escalation

### Configuration

Each notification type can define escalation behavior:

- `escalation_interval_seconds` (INT, nullable): time between escalations.
- `max_escalations` (INT, default 0): maximum number of escalation cycles.

These are set on the `notification_types` table via `RegisterType` or `UpdateType`.

### How Escalation Works

1. **Initial setup**: During processing, if `escalation_interval_seconds` is set
   and `max_escalations > 0`, the processor computes:

   ```rust
   let next_escalation_at = Some(Utc::now() + Duration::seconds(interval as i64));
   ```

   This is stored on the notification row.

2. **Escalation loop**: The scheduler runs every 60 seconds and selects unread
   notifications past their `next_escalation_at`:

   ```sql
   SELECT * FROM notifications
   WHERE state = 'unread'
     AND next_escalation_at IS NOT NULL
     AND next_escalation_at <= now()
   ORDER BY next_escalation_at
   LIMIT 100
   FOR UPDATE SKIP LOCKED
   ```

3. **Re-delivery**: For each escalated notification:
   - `escalation_count` is incremented.
   - If `new_count < max_escalations`, `next_escalation_at` is set to
     `now() + escalation_interval_seconds`. Otherwise it is set to NULL (no more
     escalations).
   - The scheduler looks up previous delivery attempts that had status `sent` or
     `delivered`, creates new `delivery_attempts` rows, and publishes
     `DeliveryDispatchMessage` to JetStream for each channel.

4. **Effect**: The user receives the same notification again via their delivery
   channels (email, push, etc.) as a reminder, up to `max_escalations` times.

### Example Timeline

Given `escalation_interval_seconds = 300` and `max_escalations = 3`:

```
T+0s     Notification created, initial delivery, next_escalation_at = T+300s
T+300s   Escalation #1 (escalation_count=1), re-delivered, next_at = T+600s
T+600s   Escalation #2 (escalation_count=2), re-delivered, next_at = T+900s
T+900s   Escalation #3 (escalation_count=3), re-delivered, next_at = NULL (max reached)
```

If the user marks the notification as read before an escalation fires, the state
changes to `read` and the escalation query (which filters `state = 'unread'`) will
no longer select it.

## Background Scheduler

The `Scheduler` struct spawns three independent tokio tasks:

| Loop            | Interval | Batch Size | Query Pattern                |
|-----------------|----------|------------|------------------------------|
| Snooze wake-up  | 30s      | 100        | `FOR UPDATE SKIP LOCKED`     |
| TTL expiry      | 60s      | 100        | `FOR UPDATE SKIP LOCKED`     |
| Escalation      | 60s      | 100        | `FOR UPDATE SKIP LOCKED`     |

### Multi-instance Safety

Every scheduler query uses `FOR UPDATE SKIP LOCKED`, which means:

- Row-level locks are acquired on the selected rows.
- If another instance has already locked a row, it is skipped (not waited on).
- Each instance processes a disjoint subset of work in each cycle.
- No external coordination (e.g., distributed locks) is required.

The batch size of 100 limits the work per cycle. If there are more than 100 rows
due for processing, they will be picked up in subsequent cycles (or by other
instances running concurrently).

## Preference Resolution Hierarchy

When a notification is processed, the `preference_resolver::resolve` function
determines whether to deliver it and which channels to use. The hierarchy is
evaluated top-down; the first matching rule that suppresses delivery wins.

### Resolution Order

```
1. Global User Toggle (user_preferences.global_enabled)
   |
   +-- If false: skip delivery ("user has notifications globally disabled")
   |
2. Mute Rules (mute_rules table)
   |
   +-- If any active rule matches: skip ("muted by user rule")
   |   - Matches by service_id (NULL = all services)
   |   - Matches by notification_type_id (NULL = all types)
   |   - Active if muted_until is NULL (permanent) or > now()
   |
3. User Service Preference (user_service_preferences)
   |
   +-- If enabled = false: skip ("service disabled by user preference")
   +-- If min_severity is set and level severity < threshold: skip
   |
4. User Type Preference (user_type_preferences)
   |
   +-- If enabled = false: skip ("notification type disabled by user preference")
   |
5. Channel Resolution (if delivering)
      Priority: type_pref.channels > service_pref.channels >
                notification_type.default_channels > global_policy.default_channels
```

### Channel Resolution Detail

Channels are resolved with the following fallback chain:

1. If the user has a type-level preference with non-empty `channels`, use those.
2. Otherwise, if the user has a service-level preference with non-empty `channels`, use those.
3. Otherwise, use the notification type's `default_channels`.
4. If the notification type has no default channels, use the global policy's `default_channels`.

## Threading and Grouping via `group_key`

Notifications can be grouped by setting the `group_key` field when sending.
Notifications with the same `group_key` for a given user form a logical thread.

The database has a partial index to support group lookups:

```sql
CREATE INDEX idx_notifications_user_group
    ON notifications (recipient_user_id, group_key) WHERE group_key IS NOT NULL;
```

The `ListNotifications` RPC accepts a `group_key` filter parameter, allowing
clients to fetch all notifications in a thread. Grouping is purely a query-time
concept -- it does not affect delivery or lifecycle behavior.

## Idempotency via `idempotency_key`

Producers can include an `idempotency_key` when sending a notification. The
database enforces uniqueness via a unique partial index:

```sql
CREATE UNIQUE INDEX idx_notifications_idempotency
    ON notifications (idempotency_key) WHERE idempotency_key IS NOT NULL;
```

If a notification with the same `idempotency_key` already exists, the INSERT
will fail with a unique constraint violation. This prevents duplicate notifications
from being created if a producer retries a send (e.g., after a timeout).

The key is optional. Notifications without an `idempotency_key` (NULL) are not
subject to deduplication, and the partial index excludes NULL values so they do
not affect index performance.
