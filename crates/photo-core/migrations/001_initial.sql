CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    input_path TEXT NOT NULL,
    output_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('scanning', 'ready', 'interrupted', 'failed')),
    warning_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT
);

CREATE TABLE assets (
    id TEXT NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    original_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    file_type TEXT NOT NULL,
    file_size INTEGER NOT NULL CHECK(file_size >= 0),
    modified_at TEXT,
    fingerprint TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    thumbnail_path TEXT,
    preview_status TEXT NOT NULL CHECK(preview_status IN ('ready', 'unavailable', 'failed')),
    metadata_warning TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (job_id, id),
    UNIQUE (job_id, original_path)
);
CREATE INDEX assets_job_order ON assets(job_id, filename COLLATE NOCASE, id);

CREATE TABLE processing_state (
    job_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    stage TEXT NOT NULL DEFAULT 'discovered' CHECK(stage IN (
        'discovered', 'preview_generated', 'analyzed', 'style_inferred',
        'recipe_created', 'rendered', 'exported', 'complete', 'failed'
    )),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    lease_owner TEXT,
    lease_expires_at TEXT,
    last_error_json TEXT,
    recipe_json TEXT,
    analysis_json TEXT,
    style_id TEXT,
    engine_version TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (job_id, asset_id),
    FOREIGN KEY (job_id, asset_id) REFERENCES assets(job_id, id) ON DELETE CASCADE
);
CREATE INDEX processing_stage_queue ON processing_state(job_id, stage, lease_expires_at);
