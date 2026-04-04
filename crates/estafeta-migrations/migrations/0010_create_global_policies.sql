CREATE TABLE global_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    max_notifications_per_user_per_hour INT NOT NULL DEFAULT 100,
    max_ttl_seconds INT NOT NULL DEFAULT 2592000,
    max_escalations INT NOT NULL DEFAULT 5,
    default_channels TEXT[] NOT NULL DEFAULT '{email}',
    rate_limit_per_service_per_second INT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed with default policy
INSERT INTO global_policies (id) VALUES (gen_random_uuid());
