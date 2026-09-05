-- Source-batch relationships are independent from recipes, presets and exports.
CREATE TABLE batch_contexts (
  batch_id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  photo_type TEXT NOT NULL CHECK(photo_type IN ('portrait','real_estate','landscape')),
  selection_identity TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  analysis_version TEXT NOT NULL,
  grouping_version TEXT NOT NULL,
  selected_asset_ids_json TEXT NOT NULL,
  payload TEXT NOT NULL,
  created_at TEXT NOT NULL,
  last_accessed_at TEXT NOT NULL,
  UNIQUE(job_id,photo_type,selection_identity)
);
CREATE INDEX batch_context_history ON batch_contexts(job_id,photo_type,created_at);
CREATE TABLE batch_context_runs (
  job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  photo_type TEXT NOT NULL CHECK(photo_type IN ('portrait','real_estate','landscape')),
  payload TEXT NOT NULL,
  PRIMARY KEY(job_id,photo_type)
);
