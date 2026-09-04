ALTER TABLE development_state ADD COLUMN toolkit_json TEXT NOT NULL DEFAULT '{}';
CREATE TABLE mask_state (
  job_id TEXT NOT NULL,
  asset_id TEXT NOT NULL,
  diagnostic_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (job_id, asset_id),
  FOREIGN KEY (job_id, asset_id) REFERENCES assets(job_id, id) ON DELETE CASCADE
);
