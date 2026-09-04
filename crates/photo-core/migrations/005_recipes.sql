-- Additive schema migration. Legacy adjustment JSON remains preserved; conversion is lazy per asset.
CREATE TABLE asset_recipes (
 job_id TEXT NOT NULL, asset_id TEXT NOT NULL, recipe_json TEXT NOT NULL,
 schema_version INTEGER NOT NULL, recipe_hash TEXT NOT NULL, origin TEXT NOT NULL,
 generation INTEGER NOT NULL DEFAULT 1, current_revision INTEGER NOT NULL DEFAULT 1,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL, error_json TEXT,
 PRIMARY KEY(job_id,asset_id), FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE TABLE recipe_revisions (
 revision_id TEXT PRIMARY KEY, job_id TEXT NOT NULL, asset_id TEXT NOT NULL,
 revision_number INTEGER NOT NULL, recipe_json TEXT NOT NULL, recipe_hash TEXT NOT NULL,
 origin TEXT NOT NULL, reason TEXT NOT NULL, created_at TEXT NOT NULL,
 UNIQUE(job_id,asset_id,revision_number),
 FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE INDEX recipe_revisions_asset ON recipe_revisions(job_id,asset_id,revision_number DESC);
CREATE TABLE recipe_recovery (
 recovery_id TEXT PRIMARY KEY, job_id TEXT NOT NULL, asset_id TEXT NOT NULL,
 payload TEXT NOT NULL, error_json TEXT NOT NULL, created_at TEXT NOT NULL,
 FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);

