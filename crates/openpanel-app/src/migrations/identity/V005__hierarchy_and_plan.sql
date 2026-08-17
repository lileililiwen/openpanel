-- Identity v0.5: hierarchy and hosting-plan references.
-- The `parent_account_id` column backs the reseller hierarchy change
-- (`add-account-hierarchy`). The `hosting_plan_id` column backs the
-- plan-driven quota change (`add-hosting-plans`). Both are nullable
-- and indexed so the follow-on changes can search by either field
-- without a table scan.
-- Self-parenting is enforced at the application layer because SQLite
-- CHECK constraints would require a subquery that the engine does not
-- support portably.

ALTER TABLE users ADD COLUMN parent_account_id TEXT;
ALTER TABLE users ADD COLUMN hosting_plan_id TEXT;

CREATE INDEX IF NOT EXISTS idx_users_parent_account_id ON users(parent_account_id);
CREATE INDEX IF NOT EXISTS idx_users_hosting_plan_id ON users(hosting_plan_id);
