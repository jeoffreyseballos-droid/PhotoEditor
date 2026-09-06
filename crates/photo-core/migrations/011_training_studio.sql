CREATE TABLE training_datasets (
    dataset_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    photo_type TEXT NOT NULL,
    dataset_fingerprint TEXT,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
);

CREATE INDEX idx_training_datasets_job
    ON training_datasets(job_id, updated_at DESC);

CREATE TABLE training_target_cache (
    cache_identity TEXT PRIMARY KEY,
    pair_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL
);

CREATE INDEX idx_training_target_pair
    ON training_target_cache(pair_id, last_accessed_at DESC);

CREATE TABLE training_runs (
    run_id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    status TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(dataset_id) ON DELETE CASCADE
);

CREATE INDEX idx_training_runs_dataset
    ON training_runs(dataset_id, updated_at DESC);

CREATE TABLE training_feedback (
    dataset_id TEXT NOT NULL,
    pair_id TEXT NOT NULL,
    feedback TEXT NOT NULL,
    predicted_recipe_json TEXT,
    corrected_recipe_json TEXT,
    difference_json TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (dataset_id, pair_id),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(dataset_id) ON DELETE CASCADE
);
