-- Orders belong to a user. With Clerk, users live in Clerk's cloud, not in our
-- database, so user_id is just the Clerk user id string (e.g. "user_2abc…") —
-- there is no local users table to reference.
ALTER TABLE orders ADD COLUMN user_id TEXT;

-- This is a fresh tutorial database, so `orders` is empty here — no rows to
-- backfill. In a real app with existing data you'd backfill user_id on
-- existing rows before adding the NOT NULL constraint.
ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
