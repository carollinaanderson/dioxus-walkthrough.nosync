CREATE TABLE IF NOT EXISTS orders (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    item       TEXT        NOT NULL,
    amount     BIGINT      NOT NULL,
    status     TEXT        NOT NULL DEFAULT 'queued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
