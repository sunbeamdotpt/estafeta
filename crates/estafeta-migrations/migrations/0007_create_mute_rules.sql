CREATE TABLE mute_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    user_id TEXT NOT NULL,
    service_id UUID REFERENCES services(id) ON DELETE CASCADE,
    notification_type_id UUID REFERENCES notification_types(id) ON DELETE CASCADE,
    muted_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_mute_rules_user ON mute_rules (user_id);
CREATE INDEX idx_mute_rules_active ON mute_rules (user_id, muted_until)
    WHERE muted_until IS NULL;
