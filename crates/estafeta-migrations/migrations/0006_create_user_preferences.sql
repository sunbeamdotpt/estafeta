CREATE TABLE user_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    user_id TEXT NOT NULL UNIQUE,
    global_enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE user_service_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    user_id TEXT NOT NULL,
    service_id UUID NOT NULL REFERENCES services(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT true,
    min_severity INT,
    channels TEXT[] NOT NULL DEFAULT '{}',
    UNIQUE (user_id, service_id)
);

CREATE TABLE user_type_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    user_id TEXT NOT NULL,
    notification_type_id UUID NOT NULL REFERENCES notification_types(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT true,
    channels TEXT[],
    UNIQUE (user_id, notification_type_id)
);
