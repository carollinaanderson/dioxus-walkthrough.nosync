CREATE TABLE IF NOT EXISTS orders (
    instance_id TEXT PRIMARY KEY,
    item        TEXT        NOT NULL,
    amount      BIGINT      NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
