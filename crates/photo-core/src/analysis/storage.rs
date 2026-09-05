use super::*;
use rusqlite::{params, OptionalExtension};
use std::io::Write;
impl JobRepository {
    pub(super) fn analysis_record(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
        key: &str,
    ) -> ProcessingResult<Option<PhotoAnalysis>> {
        let json:Option<String>=self.connect().map_err(internal)?.query_row("SELECT payload FROM photo_analysis WHERE job_id=?1 AND asset_id=?2 AND photo_type=?3 AND cache_key=?4",params![job,asset,kind.as_str(),key],|r|r.get(0)).optional().map_err(internal)?;
        json.map(|s| PhotoAnalysis::parse(&s).map_err(internal))
            .transpose()?
            .map(|a| {
                if a.asset_id != asset || a.photo_type != kind {
                    Err(internal("Stored analysis identity mismatch"))
                } else {
                    Ok(a)
                }
            })
            .transpose()
    }
    pub(super) fn common_analysis(
        &self,
        job: &str,
        asset: &str,
        key: &str,
    ) -> ProcessingResult<Option<CommonAnalysis>> {
        let json:Option<String>=self.connect().map_err(internal)?.query_row("SELECT payload FROM photo_analysis WHERE job_id=?1 AND asset_id=?2 AND common_key=?3 ORDER BY updated_at DESC LIMIT 1",params![job,asset,key],|r|r.get(0)).optional().map_err(internal)?;
        json.map(|s| PhotoAnalysis::parse(&s).map(|a| a.common).map_err(internal))
            .transpose()
    }
    pub(super) fn analysis_status(
        &self,
        request: &AnalysisRequest,
        status: AnalysisStatus,
        error: Option<&str>,
    ) -> ProcessingResult<()> {
        self.connect().map_err(internal)?.execute("INSERT INTO analysis_status(job_id,asset_id,photo_type,state,request_id,error,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(job_id,asset_id,photo_type) DO UPDATE SET state=excluded.state,request_id=excluded.request_id,error=excluded.error,updated_at=excluded.updated_at",params![request.job_id,request.asset_id,request.photo_type.as_str(),status.as_str(),request.request_id,error,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
        Ok(())
    }
    pub(super) fn last_analysis_status(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
    ) -> ProcessingResult<(AnalysisStatus, Option<String>)> {
        let row:Option<(String,Option<String>)>=self.connect().map_err(internal)?.query_row("SELECT state,error FROM analysis_status WHERE job_id=?1 AND asset_id=?2 AND photo_type=?3",params![job,asset,kind.as_str()],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(internal)?;
        Ok(row
            .map(|(s, e)| (AnalysisStatus::from_str(&s), e))
            .unwrap_or((AnalysisStatus::NotAnalyzed, None)))
    }
    pub(super) fn persist_analysis(
        &self,
        job: &str,
        a: &PhotoAnalysis,
        key: &str,
        common_key: &str,
        status: AnalysisStatus,
        cancel: &CancellationToken,
    ) -> ProcessingResult<()> {
        let json = a.canonical_json().map_err(internal)?;
        let mut db = self.connect().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        tx.execute("INSERT INTO photo_analysis(job_id,asset_id,photo_type,analysis_id,schema_version,source_fingerprint,cache_key,common_key,engine_version,providers_json,payload,created_at,updated_at,median_luminance,highlight_clip_fraction,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,?13,?14,?15) ON CONFLICT(job_id,asset_id,photo_type) DO UPDATE SET analysis_id=excluded.analysis_id,schema_version=excluded.schema_version,source_fingerprint=excluded.source_fingerprint,cache_key=excluded.cache_key,common_key=excluded.common_key,engine_version=excluded.engine_version,providers_json=excluded.providers_json,payload=excluded.payload,created_at=excluded.created_at,updated_at=excluded.updated_at,median_luminance=excluded.median_luminance,highlight_clip_fraction=excluded.highlight_clip_fraction,status=excluded.status",params![job,a.asset_id,a.photo_type.as_str(),a.analysis_id,a.schema_version,a.source_fingerprint,key,common_key,a.diagnostics.engine_version,serde_json::to_string(&a.diagnostics.providers).map_err(internal)?,json,a.created_at,a.common.exposure.median_luminance,a.common.exposure.highlight_clip_fraction,status.as_str()]).map_err(internal)?;
        tx.execute("UPDATE analysis_status SET state=?4,error=NULL,updated_at=?5 WHERE job_id=?1 AND asset_id=?2 AND photo_type=?3",params![job,a.asset_id,a.photo_type.as_str(),status.as_str(),a.created_at]).map_err(internal)?;
        cancel.check()?;
        tx.commit().map_err(internal)?;
        Ok(())
    }
    pub(super) fn clear_analysis(&self, job: &str, asset: &str) -> ProcessingResult<()> {
        self.asset(job, asset).map_err(internal)?;
        let mut db = self.connect().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        tx.execute(
            "DELETE FROM photo_analysis WHERE job_id=?1 AND asset_id=?2",
            params![job, asset],
        )
        .map_err(internal)?;
        tx.execute(
            "DELETE FROM analysis_status WHERE job_id=?1 AND asset_id=?2",
            params![job, asset],
        )
        .map_err(internal)?;
        tx.commit().map_err(internal)?;
        Ok(())
    }
    pub(super) fn export_analysis_json(
        &self,
        job: &str,
        asset: &str,
        json: &str,
    ) -> ProcessingResult<PathBuf> {
        let record = self.get_job(job).map_err(internal)?;
        let asset = self.asset(job, asset).map_err(internal)?;
        let output = record.output_path.canonicalize().map_err(io_error)?;
        let input = record.input_path.canonicalize().map_err(io_error)?;
        if crate::paths::same_or_descendant(&input, &output) {
            return Err(internal("Analysis output aliases input folder"));
        }
        let stem: String = asset
            .original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .chars()
            .take(100)
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let mut temp = tempfile::NamedTempFile::new_in(&output).map_err(io_error)?;
        temp.write_all(json.as_bytes()).map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        let dest = output.join(format!("{stem}-{}.analysis.json", uuid::Uuid::new_v4()));
        temp.persist_noclobber(&dest)
            .map_err(|e| io_error(e.error))?;
        Ok(dest)
    }
}
