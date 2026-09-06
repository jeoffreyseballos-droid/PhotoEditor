CREATE TABLE trained_style_results (
    job_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    style_id TEXT NOT NULL,
    style_version TEXT NOT NULL,
    model_version TEXT NOT NULL,
    package_identity TEXT NOT NULL,
    feature_schema TEXT NOT NULL,
    input_identity TEXT,
    analysis_id TEXT,
    batch_context_id TEXT,
    status TEXT NOT NULL,
    prediction_json TEXT,
    feature_summary_json TEXT,
    recipe_hash TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (job_id, asset_id),
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (job_id, asset_id) REFERENCES assets(job_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_trained_style_results_identity
    ON trained_style_results(job_id, style_id, package_identity, batch_context_id, analysis_id);

CREATE TABLE trained_style_runs (
    job_id TEXT NOT NULL,
    photo_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (job_id, photo_type),
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);
