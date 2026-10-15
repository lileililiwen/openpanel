CREATE TABLE IF NOT EXISTS software_components_v4(id TEXT NOT NULL,version TEXT NOT NULL DEFAULT '',status TEXT NOT NULL,managed INTEGER NOT NULL,state_digest TEXT NOT NULL,updated_at TEXT NOT NULL,PRIMARY KEY(id,version));
INSERT INTO software_components_v4(id,version,status,managed,state_digest,updated_at) SELECT id,COALESCE(version,''),status,managed,state_digest,updated_at FROM software_components;
DROP TABLE software_components;
ALTER TABLE software_components_v4 RENAME TO software_components;
