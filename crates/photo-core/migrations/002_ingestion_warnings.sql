ALTER TABLE assets ADD COLUMN warnings_json TEXT NOT NULL DEFAULT '[]';
CREATE TABLE ingestion_warnings (
    id INTEGER PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    asset_id TEXT,
    category TEXT NOT NULL CHECK(category IN ('metadata','preview','unreadable','access','traversal')),
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    path TEXT,
    FOREIGN KEY (job_id, asset_id) REFERENCES assets(job_id, id) ON DELETE CASCADE
);
CREATE INDEX warnings_job_category ON ingestion_warnings(job_id, category);
CREATE INDEX warnings_asset ON ingestion_warnings(job_id, asset_id);
-- Old Phase 1 messages combined metadata and preview problems. Do not infer a more
-- precise cause; preserve the original message with an explicit legacy code.
UPDATE assets SET warnings_json = json_array(json_object('category','metadata','code','legacy_inspection','message','Legacy inspection warning (rescan for category-specific diagnostics): ' || metadata_warning,'path',original_path)) WHERE metadata_warning IS NOT NULL;
INSERT INTO ingestion_warnings(job_id,asset_id,category,code,message,path)
SELECT job_id,id,'metadata','legacy_inspection','Legacy inspection warning (rescan for category-specific diagnostics): ' || metadata_warning,original_path FROM assets WHERE metadata_warning IS NOT NULL;
