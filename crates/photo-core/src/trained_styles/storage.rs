use super::{StyleApplyProgress, StyleAssetInference};
use crate::{rendering::internal, repository::JobRepository};
use photo_contracts::{analysis::PhotoType, ProcessingResult};
use rusqlite::{params, OptionalExtension};

type StoredStyleInferenceRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

impl JobRepository {
    pub(super) fn save_style_progress(
        &self,
        progress: &StyleApplyProgress,
    ) -> ProcessingResult<()> {
        let payload = serde_json::to_string(progress).map_err(internal)?;
        self.connect()
            .map_err(internal)?
            .execute(
                "INSERT INTO trained_style_runs(job_id,photo_type,payload) VALUES(?1,?2,?3) ON CONFLICT(job_id,photo_type) DO UPDATE SET payload=excluded.payload",
                params![progress.job_id, progress.photo_type.as_str(), payload],
            )
            .map_err(internal)?;
        Ok(())
    }

    pub(super) fn style_progress(
        &self,
        job: &str,
        photo_type: PhotoType,
    ) -> ProcessingResult<Option<StyleApplyProgress>> {
        let payload: Option<String> = self
            .connect()
            .map_err(internal)?
            .query_row(
                "SELECT payload FROM trained_style_runs WHERE job_id=?1 AND photo_type=?2",
                params![job, photo_type.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        payload
            .map(|json| serde_json::from_str(&json).map_err(internal))
            .transpose()
    }

    pub(super) fn save_style_inference(
        &self,
        result: &StyleAssetInference,
    ) -> ProcessingResult<()> {
        let prediction = result
            .prediction
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(internal)?;
        let summary = result
            .feature_summary
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(internal)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.connect().map_err(internal)?.execute(
            "INSERT INTO trained_style_results(job_id,asset_id,style_id,style_version,model_version,package_identity,feature_schema,input_identity,analysis_id,batch_context_id,status,prediction_json,feature_summary_json,recipe_hash,error,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?16) ON CONFLICT(job_id,asset_id) DO UPDATE SET style_id=excluded.style_id,style_version=excluded.style_version,model_version=excluded.model_version,package_identity=excluded.package_identity,feature_schema=excluded.feature_schema,input_identity=excluded.input_identity,analysis_id=excluded.analysis_id,batch_context_id=excluded.batch_context_id,status=excluded.status,prediction_json=excluded.prediction_json,feature_summary_json=excluded.feature_summary_json,recipe_hash=excluded.recipe_hash,error=excluded.error,updated_at=excluded.updated_at",
            params![result.job_id,result.asset_id,result.style_id,result.style_version,result.model_version,result.package_identity,result.feature_schema,result.input_identity,result.analysis_id,result.batch_context_id,result.status,prediction,summary,result.recipe_hash,result.error,now],
        ).map_err(internal)?;
        Ok(())
    }

    pub(super) fn style_inferences(
        &self,
        job: &str,
        assets: &[String],
    ) -> ProcessingResult<Vec<StyleAssetInference>> {
        let db = self.connect().map_err(internal)?;
        let mut statement = db.prepare(
            "SELECT asset_id,style_id,style_version,model_version,package_identity,feature_schema,input_identity,analysis_id,batch_context_id,status,prediction_json,feature_summary_json,recipe_hash,error FROM trained_style_results WHERE job_id=?1 AND asset_id=?2",
        ).map_err(internal)?;
        let mut results = Vec::new();
        for asset in assets {
            let row: Option<StoredStyleInferenceRow> = statement
                .query_row(params![job, asset], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                    ))
                })
                .optional()
                .map_err(internal)?;
            if let Some((
                asset_id,
                style_id,
                style_version,
                model_version,
                package_identity,
                feature_schema,
                input_identity,
                analysis_id,
                batch_context_id,
                status,
                prediction,
                summary,
                recipe_hash,
                error,
            )) = row
            {
                results.push(StyleAssetInference {
                    job_id: job.into(),
                    asset_id,
                    style_id,
                    style_version,
                    model_version,
                    package_identity,
                    feature_schema,
                    input_identity,
                    analysis_id,
                    batch_context_id,
                    status,
                    prediction: prediction
                        .map(|json| serde_json::from_str(&json).map_err(internal))
                        .transpose()?,
                    feature_summary: summary
                        .map(|json| serde_json::from_str(&json).map_err(internal))
                        .transpose()?,
                    recipe_hash,
                    error,
                    stale: false,
                });
            }
        }
        Ok(results)
    }
}
