-- Hosting plans v0.1 initial schema.
-- Owned by the hosting-plans bounded context. The single-row
-- `hosting_plans` table stores plan metadata as columns for the
-- searchable fields (name, status) and JSON blobs for the
-- small heterogeneous collections (prices, features, allowed
-- apps, allowed PHP runtimes). The `user_plan_assignments` table
-- is append-only: re-assignment inserts a new row and marks the
-- previous one as `replaced_at`.

CREATE TABLE IF NOT EXISTS hosting_plans (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    prices_json TEXT NOT NULL,
    features_json TEXT NOT NULL,
    quota_caps_json TEXT NOT NULL,
    allowed_apps_json TEXT NOT NULL,
    allowed_php_runtimes_json TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_hosting_plans_name ON hosting_plans(name);
CREATE INDEX IF NOT EXISTS idx_hosting_plans_status ON hosting_plans(status);

CREATE TABLE IF NOT EXISTS user_plan_assignments (
    user_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    assigned_at TEXT NOT NULL,
    assigned_by TEXT NOT NULL,
    replaced_at TEXT,
    PRIMARY KEY (user_id, assigned_at),
    FOREIGN KEY (plan_id) REFERENCES hosting_plans(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_user_plan_assignments_plan
    ON user_plan_assignments(plan_id);
CREATE INDEX IF NOT EXISTS idx_user_plan_assignments_current
    ON user_plan_assignments(user_id, replaced_at);
