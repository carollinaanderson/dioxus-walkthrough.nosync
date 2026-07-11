ALTER TABLE orders
    ADD COLUMN user_id TEXT REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
