use super::*;
use rusqlite::{params, OptionalExtension};
impl JobRepository {
    pub fn culling_state(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
    ) -> ProcessingResult<CullingState> {
        self.asset(job, asset).map_err(internal)?;
        let db = self.connect().map_err(internal)?;
        let json:Option<String>=db.query_row("SELECT a.payload FROM culling_current c JOIN culling_assessments a USING(assessment_id) WHERE c.job_id=?1 AND c.asset_id=?2 AND c.photo_type=?3",params![job,asset,kind.as_str()],|r|r.get(0)).optional().map_err(internal)?;
        let assessment = json
            .map(|s| CullingAssessment::parse(&s).map_err(internal))
            .transpose()?;
        if assessment
            .as_ref()
            .is_some_and(|a| a.asset_id != asset || a.photo_type != kind)
        {
            return Err(internal("Stored culling identity mismatch"));
        }
        let user:Option<(Option<u8>,bool,String)>=db.query_row("SELECT user_rating,selected,updated_at FROM culling_user_state WHERE job_id=?1 AND asset_id=?2",params![job,asset],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(internal)?;
        let (rating, selected, updated) = user
            .map(|(r, s, u)| (r, s, Some(u)))
            .unwrap_or((None, false, None));
        let user_rating = rating.map(Stars::new).transpose().map_err(internal)?;
        Ok(CullingState {
            effective_rating: CullingState::effective(
                assessment.as_ref().and_then(|a| a.ai_rating),
                user_rating,
            ),
            assessment,
            user_rating,
            selected_for_editing: selected,
            stale: false,
            updated_at: updated,
        })
    }
    /// Whole supplied group swaps atomically; previously completed groups survive cancellation.
    pub fn persist_culling(
        &self,
        job: &str,
        assessments: &[CullingAssessment],
        cancel: &CancellationToken,
    ) -> ProcessingResult<()> {
        let mut db = self.connect().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        for a in assessments {
            cancel.check()?;
            let json = a.canonical_json().map_err(internal)?;
            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT job_id,payload FROM culling_assessments WHERE assessment_id=?1",
                    [&a.assessment_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(internal)?;
            if existing.is_some_and(|(j, p)| j != job || p != json) {
                return Err(internal(
                    "Immutable assessment ID cannot be reused for different evidence",
                ));
            }
            tx.execute("INSERT OR IGNORE INTO culling_assessments(assessment_id,job_id,asset_id,photo_type,schema_version,ai_rating,confidence,source_analysis_id,source_fingerprint,cache_key,engine_version,models_json,payload,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",params![a.assessment_id,job,a.asset_id,a.photo_type.as_str(),a.schema_version,a.ai_rating.map(Stars::get),a.confidence,a.source_analysis_id,a.source_fingerprint,a.cache_key,a.culling_engine_version,serde_json::to_string(&a.model_versions).map_err(internal)?,json,a.created_at]).map_err(internal)?;
            tx.execute("INSERT INTO culling_current(job_id,asset_id,photo_type,assessment_id) VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,asset_id,photo_type) DO UPDATE SET assessment_id=excluded.assessment_id",params![job,a.asset_id,a.photo_type.as_str(),a.assessment_id]).map_err(internal)?;
        }
        cancel.check()?;
        tx.commit().map_err(internal)?;
        Ok(())
    }
    pub fn culling_override(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
        rating: Option<Stars>,
    ) -> ProcessingResult<()> {
        self.asset(job, asset).map_err(internal)?;
        let mut db = self.connect().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute("INSERT INTO culling_rating_events(job_id,asset_id,assessment_id,user_rating,created_at) VALUES(?1,?2,(SELECT assessment_id FROM culling_current WHERE job_id=?1 AND asset_id=?2 AND photo_type=?3),?4,?5)",params![job,asset,kind.as_str(),rating.map(Stars::get),now]).map_err(internal)?;
        tx.execute("INSERT INTO culling_user_state(job_id,asset_id,user_rating,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,asset_id) DO UPDATE SET user_rating=excluded.user_rating,updated_at=excluded.updated_at",params![job,asset,rating.map(Stars::get),now]).map_err(internal)?;
        tx.commit().map_err(internal)?;
        Ok(())
    }
    pub fn culling_select(&self, job: &str, selected: &[(String, bool)]) -> ProcessingResult<()> {
        if selected.len() > MAX_BATCH {
            return Err(internal("Selection exceeds batch limit"));
        }
        let mut db = self.connect().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        for (asset, value) in selected {
            tx.execute("INSERT INTO culling_user_state(job_id,asset_id,selected,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,asset_id) DO UPDATE SET selected=excluded.selected,updated_at=excluded.updated_at",params![job,asset,value,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
        Ok(())
    }
    pub fn culling_progress(&self, job: &str) -> ProcessingResult<Option<CullingProgress>> {
        let json: Option<String> = self
            .connect()
            .map_err(internal)?
            .query_row(
                "SELECT payload FROM culling_runs WHERE job_id=?1",
                [job],
                |r| r.get(0),
            )
            .optional()
            .map_err(internal)?;
        json.map(|s| serde_json::from_str(&s).map_err(internal))
            .transpose()
    }
    pub(super) fn save_culling_progress(&self, p: &CullingProgress) -> ProcessingResult<()> {
        self.connect().map_err(internal)?.execute("INSERT INTO culling_runs(job_id,payload) VALUES(?1,?2) ON CONFLICT(job_id) DO UPDATE SET payload=excluded.payload",params![p.job_id,serde_json::to_string(p).map_err(internal)?]).map_err(internal)?;
        Ok(())
    }
}
