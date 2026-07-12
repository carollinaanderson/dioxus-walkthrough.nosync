-- Orders belong to a user. With Clerk, users live in Clerk's cloud, not in our
-- database, so user_id is just the Clerk user id string (e.g. "user_2abc…") —
-- there is no local users table to reference.
ALTER TABLE orders ADD COLUMN user_id TEXT;

ALTER TABLE orders ALTER COLUMN user_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS orders_user_created_idx ON orders (user_id, created_at DESC);
