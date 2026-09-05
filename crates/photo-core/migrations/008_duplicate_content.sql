-- Full-file hashes are independent of path-based source/recipe identities.
CREATE TABLE duplicate_content_cache (
  job_id TEXT NOT NULL, asset_id TEXT NOT NULL,
  file_stamp TEXT NOT NULL, algorithm TEXT NOT NULL,
  sha256 TEXT NOT NULL CHECK(length(sha256)=64), byte_length INTEGER NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(job_id,asset_id),
  FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
