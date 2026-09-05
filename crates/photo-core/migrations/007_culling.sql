-- Immutable AI evidence, independent photographer intent, append-only override feedback.
CREATE TABLE culling_assessments (
  assessment_id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL, asset_id TEXT NOT NULL, photo_type TEXT NOT NULL,
  schema_version INTEGER NOT NULL, ai_rating INTEGER CHECK(ai_rating BETWEEN 1 AND 5),
  confidence REAL NOT NULL, source_analysis_id TEXT, source_fingerprint TEXT NOT NULL,
  cache_key TEXT NOT NULL, engine_version TEXT NOT NULL, models_json TEXT NOT NULL,
  payload TEXT NOT NULL, created_at TEXT NOT NULL,
  FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE INDEX culling_history_asset ON culling_assessments(job_id,asset_id,photo_type,created_at);
CREATE TABLE culling_current (
  job_id TEXT NOT NULL, asset_id TEXT NOT NULL, photo_type TEXT NOT NULL,
  assessment_id TEXT NOT NULL REFERENCES culling_assessments(assessment_id),
  PRIMARY KEY(job_id,asset_id,photo_type),
  FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE TABLE culling_user_state (
  job_id TEXT NOT NULL, asset_id TEXT NOT NULL,
  user_rating INTEGER CHECK(user_rating BETWEEN 1 AND 5),
  selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0,1)), updated_at TEXT NOT NULL,
  PRIMARY KEY(job_id,asset_id),
  FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE TABLE culling_rating_events (
  event_id INTEGER PRIMARY KEY, job_id TEXT NOT NULL, asset_id TEXT NOT NULL,
  assessment_id TEXT REFERENCES culling_assessments(assessment_id),
  user_rating INTEGER CHECK(user_rating BETWEEN 1 AND 5), created_at TEXT NOT NULL,
  FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE TABLE culling_runs (
  job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
  payload TEXT NOT NULL
);
