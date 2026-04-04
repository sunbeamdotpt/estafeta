CREATE TABLE user_channel_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID,
    user_id TEXT NOT NULL UNIQUE,
    email_address TEXT,
    phone_number TEXT,
    webhook_url TEXT,
    webhook_secret TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
