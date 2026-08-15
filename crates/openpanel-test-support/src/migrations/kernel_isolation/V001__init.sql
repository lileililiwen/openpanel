-- Kernel resource isolation: cgroup limits, namespace config, and
-- the singleton default policy.

CREATE TABLE IF NOT EXISTS cgroup_limits (
    user_id            TEXT PRIMARY KEY NOT NULL,
    cpu_millicores     INTEGER NOT NULL,
    memory_high_mib    INTEGER NOT NULL,
    memory_max_mib     INTEGER NOT NULL,
    pids_max           INTEGER NOT NULL,
    updated_at         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_namespaces (
    user_id            TEXT PRIMARY KEY NOT NULL,
    enabled            INTEGER NOT NULL DEFAULT 1,
    cgroup_namespace   INTEGER NOT NULL DEFAULT 1,
    pid_namespace      INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS isolation_policy (
    id                           INTEGER PRIMARY KEY CHECK (id = 1),
    updated_at                   TEXT NOT NULL,
    updated_by                   TEXT NOT NULL,
    default_cpu_millicores        INTEGER NOT NULL,
    default_memory_high_mib       INTEGER NOT NULL,
    default_memory_max_mib        INTEGER NOT NULL,
    default_pids_max              INTEGER NOT NULL
);