CREATE TABLE notification_types (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    service_id UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    type_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    json_schema JSONB NOT NULL,
    default_channels TEXT[] NOT NULL DEFAULT '{}',
    default_ttl_seconds INT,
    escalation_interval_seconds INT,
    max_escalations INT NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (service_id, type_key)
);

CREATE INDEX idx_notification_types_service ON notification_types (service_id);
