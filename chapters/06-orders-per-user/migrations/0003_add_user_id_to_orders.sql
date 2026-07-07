ALTER TABLE orders
    ADD COLUMN user_id UUID REFERENCES users(id) ON DELETE CASCADE;

-- This is a fresh tutorial database, so `orders` is empty here — no rows to
-- backfill. In a real app with existing data you'd backfill user_id on
-- existing rows before adding the NOT NULL constraint.
ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
