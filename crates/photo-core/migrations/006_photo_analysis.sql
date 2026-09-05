-- Source observations are independent from asset_recipes and development_state.
CREATE TABLE photo_analysis (
    job_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    photo_type TEXT NOT NULL CHECK(photo_type IN ('portrait','real_estate','landscape')),
    analysis_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    source_fingerprint TEXT NOT NULL,
    cache_key TEXT NOT NULL,
    common_key TEXT NOT NULL,
    engine_version TEXT NOT NULL,
    providers_json TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    median_luminance REAL NOT NULL,
    highlight_clip_fraction REAL NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('complete','warning')),
    PRIMARY KEY(job_id,asset_id,photo_type),
    FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
CREATE INDEX analysis_filter ON photo_analysis(job_id,photo_type,median_luminance);
CREATE INDEX analysis_common ON photo_analysis(job_id,asset_id,common_key);
CREATE TABLE analysis_status (
    job_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    photo_type TEXT NOT NULL,
    state TEXT NOT NULL,
    request_id TEXT,
    error TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(job_id,asset_id,photo_type),
    FOREIGN KEY(job_id,asset_id) REFERENCES assets(job_id,id) ON DELETE CASCADE
);
