CREATE TABLE notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    service_id UUID NOT NULL REFERENCES services(id),
    notification_type_id UUID NOT NULL REFERENCES notification_types(id),
    level_id UUID REFERENCES notification_levels(id),
    recipient_user_id TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'unseen'
        CHECK (state IN ('unseen', 'unread', 'read', 'snoozed', 'archived', 'expired')),
    payload JSONB NOT NULL,
    group_key TEXT,
    idempotency_key TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    snoozed_until TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    next_escalation_at TIMESTAMPTZ,
    escalation_count INT NOT NULL DEFAULT 0,
    seen_at TIMESTAMPTZ,
    read_at TIMESTAMPTZ,
    action_url TEXT,
    icon TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Primary query path: user's notifications by state
CREATE INDEX idx_notifications_user_state_created
    ON notifications (recipient_user_id, state, created_at DESC);

-- Idempotency dedup
CREATE UNIQUE INDEX idx_notifications_idempotency
    ON notifications (idempotency_key) WHERE idempotency_key IS NOT NULL;

-- Group/thread lookup
CREATE INDEX idx_notifications_user_group
    ON notifications (recipient_user_id, group_key) WHERE group_key IS NOT NULL;

-- Background scheduler: snooze wake-up
CREATE INDEX idx_notifications_snooze_wake
    ON notifications (snoozed_until) WHERE state = 'snoozed';

-- Background scheduler: TTL expiry
CREATE INDEX idx_notifications_expiry
    ON notifications (expires_at) WHERE state IN ('unseen', 'unread', 'read');

-- Background scheduler: escalation
CREATE INDEX idx_notifications_escalation
    ON notifications (next_escalation_at) WHERE state IN ('unseen', 'unread') AND next_escalation_at IS NOT NULL;

-- Fast unseen count for bell badge
CREATE INDEX idx_notifications_unseen
    ON notifications (recipient_user_id) WHERE state = 'unseen';
