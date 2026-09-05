use super::*;
use rusqlite::{params, OptionalExtension};

impl JobRepository {
    pub(super) fn batch_context(
        &self,
        job: &str,
        kind: PhotoType,
        identity: &str,
    ) -> ProcessingResult<Option<BatchContext>> {
        let db = self.connect().map_err(internal)?;
        let json: Option<String> = db
            .query_row(
                "SELECT payload FROM batch_contexts WHERE job_id=?1 AND photo_type=?2 AND selection_identity=?3",
                params![job, kind.as_str(), identity],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        if json.is_some() {
            db.execute(
                "UPDATE batch_contexts SET last_accessed_at=?4 WHERE job_id=?1 AND photo_type=?2 AND selection_identity=?3",
                params![job, kind.as_str(), identity, chrono::Utc::now().to_rfc3339()],
            )
            .map_err(internal)?;
        }
        json.map(|value| BatchContext::parse(&value).map_err(internal))
            .transpose()
    }

    pub(super) fn has_other_batch_context(
        &self,
        job: &str,
        kind: PhotoType,
        identity: Option<&str>,
    ) -> ProcessingResult<bool> {
        let count: u64 = self
            .connect()
            .map_err(internal)?
            .query_row(
                "SELECT COUNT(*) FROM batch_contexts WHERE job_id=?1 AND photo_type=?2 AND (?3 IS NULL OR selection_identity<>?3)",
                params![job, kind.as_str(), identity],
                |row| row.get(0),
            )
            .map_err(internal)?;
        Ok(count > 0)
    }

    pub fn persist_batch_context(&self, context: &BatchContext) -> ProcessingResult<()> {
        let json = context.canonical_json().map_err(internal)?;
        let ids = serde_json::to_string(&context.selected_asset_ids).map_err(internal)?;
        let mut db = self.connect().map_err(internal)?;
        let transaction = db.transaction().map_err(internal)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT payload FROM batch_contexts WHERE batch_id=?1",
                [&context.batch_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        if existing.as_ref().is_some_and(|payload| payload != &json) {
            // Timings are allowed to be finalized after the first atomic publication.
            let same_identity = existing
                .as_ref()
                .and_then(|payload| BatchContext::parse(payload).ok())
                .is_some_and(|stored| {
                    stored.selection_identity == context.selection_identity
                        && stored.selected_asset_ids == context.selected_asset_ids
                        && stored.grouping_version == context.grouping_version
                });
            if !same_identity {
                return Err(internal(
                    "Batch identity cannot be reused for different source evidence",
                ));
            }
        }
        transaction
            .execute(
                "INSERT INTO batch_contexts(batch_id,job_id,photo_type,selection_identity,schema_version,analysis_version,grouping_version,selected_asset_ids_json,payload,created_at,last_accessed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(batch_id) DO UPDATE SET payload=excluded.payload,last_accessed_at=excluded.last_accessed_at",
                params![
                    context.batch_id,
                    context.job_id,
                    context.photo_type.as_str(),
                    context.selection_identity,
                    context.schema_version,
                    context.analysis_version,
                    context.grouping_version,
                    ids,
                    json,
                    context.created_at,
                    chrono::Utc::now().to_rfc3339(),
                ],
            )
            .map_err(internal)?;
        transaction.commit().map_err(internal)?;
        Ok(())
    }

    pub(super) fn batch_context_progress(
        &self,
        job: &str,
        kind: PhotoType,
    ) -> ProcessingResult<Option<BatchContextProgress>> {
        let json: Option<String> = self
            .connect()
            .map_err(internal)?
            .query_row(
                "SELECT payload FROM batch_context_runs WHERE job_id=?1 AND photo_type=?2",
                params![job, kind.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        json.map(|value| serde_json::from_str(&value).map_err(internal))
            .transpose()
    }

    pub(super) fn save_batch_context_progress(
        &self,
        progress: &BatchContextProgress,
    ) -> ProcessingResult<()> {
        self.connect()
            .map_err(internal)?
            .execute(
                "INSERT INTO batch_context_runs(job_id,photo_type,payload) VALUES(?1,?2,?3) ON CONFLICT(job_id,photo_type) DO UPDATE SET payload=excluded.payload",
                params![
                    progress.job_id,
                    progress.photo_type.as_str(),
                    serde_json::to_string(progress).map_err(internal)?,
                ],
            )
            .map_err(internal)?;
        Ok(())
    }
}
