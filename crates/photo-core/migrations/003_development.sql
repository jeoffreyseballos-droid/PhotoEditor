CREATE TABLE development_state (
    job_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    adjustments_json TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    source_identity TEXT,
    state TEXT NOT NULL DEFAULT 'source_ready',
    request_id TEXT,
    preview_path TEXT,
    export_path TEXT,
    error_json TEXT,
    warnings_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    PRIMARY KEY(job_id, asset_id),
    FOREIGN KEY(job_id, asset_id) REFERENCES assets(job_id, id) ON DELETE CASCADE
);
