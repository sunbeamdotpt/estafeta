CREATE TABLE notification_levels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    service_id UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    severity INT NOT NULL,
    color TEXT,
    icon TEXT,
    UNIQUE (service_id, key)
);

CREATE INDEX idx_notification_levels_service ON notification_levels (service_id);
