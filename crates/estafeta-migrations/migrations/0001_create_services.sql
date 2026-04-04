CREATE TABLE services (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT,
    api_key_hash TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (slug)
);

CREATE INDEX idx_services_tenant ON services (tenant_id) WHERE tenant_id IS NOT NULL;
