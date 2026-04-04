CREATE TABLE delivery_attempts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    notification_id UUID NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
    channel TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sent', 'delivered', 'failed', 'skipped')),
    attempt_number INT NOT NULL DEFAULT 1,
    next_retry_at TIMESTAMPTZ,
    last_error TEXT,
    external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_delivery_retry
    ON delivery_attempts (next_retry_at) WHERE status = 'pending';

CREATE INDEX idx_delivery_notification
    ON delivery_attempts (notification_id);
