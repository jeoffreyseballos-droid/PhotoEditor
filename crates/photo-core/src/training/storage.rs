use crate::{rendering::internal, repository::JobRepository};
use photo_contracts::{
    training::{TargetRecipeResult, TrainingDataset, TrainingRun},
    ProcessingResult,
};
use rusqlite::{params, OptionalExtension};

impl JobRepository {
    pub(super) fn save_training_dataset(&self, dataset: &TrainingDataset) -> ProcessingResult<()> {
        dataset.validate_shape().map_err(internal)?;
        let payload = serde_json::to_string(dataset).map_err(internal)?;
        self.connect().map_err(internal)?.execute(
            "INSERT INTO training_datasets(dataset_id,job_id,photo_type,dataset_fingerprint,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(dataset_id) DO UPDATE SET dataset_fingerprint=excluded.dataset_fingerprint,payload=excluded.payload,updated_at=excluded.updated_at",
            params![dataset.dataset_id,dataset.job_id,dataset.photo_type.as_str(),dataset.dataset_fingerprint,payload,dataset.created_at,dataset.updated_at],
        ).map_err(internal)?;
        Ok(())
    }

    pub(super) fn training_dataset(&self, id: &str) -> ProcessingResult<TrainingDataset> {
        let payload: Option<String> = self
            .connect()
            .map_err(internal)?
            .query_row(
                "SELECT payload FROM training_datasets WHERE dataset_id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        let dataset = payload
            .ok_or_else(|| internal("Training dataset was not found"))
            .and_then(|json| serde_json::from_str::<TrainingDataset>(&json).map_err(internal))?;
        dataset.validate_shape().map_err(internal)?;
        Ok(dataset)
    }

    pub(super) fn training_datasets(&self, job: &str) -> ProcessingResult<Vec<TrainingDataset>> {
        let connection = self.connect().map_err(internal)?;
        let mut statement = connection
            .prepare(
                "SELECT payload FROM training_datasets WHERE job_id=?1 ORDER BY updated_at DESC",
            )
            .map_err(internal)?;
        let rows = statement
            .query_map([job], |row| row.get::<_, String>(0))
            .map_err(internal)?;
        let mut datasets = Vec::new();
        for row in rows {
            let dataset = serde_json::from_str::<TrainingDataset>(&row.map_err(internal)?)
                .map_err(internal)?;
            dataset.validate_shape().map_err(internal)?;
            datasets.push(dataset);
        }
        Ok(datasets)
    }

    pub(super) fn training_datasets_all(&self) -> ProcessingResult<Vec<TrainingDataset>> {
        let connection = self.connect().map_err(internal)?;
        let mut statement = connection
            .prepare("SELECT payload FROM training_datasets ORDER BY updated_at DESC")
            .map_err(internal)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(internal)?;
        let mut datasets = Vec::new();
        for row in rows {
            let dataset = serde_json::from_str::<TrainingDataset>(&row.map_err(internal)?)
                .map_err(internal)?;
            dataset.validate_shape().map_err(internal)?;
            datasets.push(dataset);
        }
        Ok(datasets)
    }

    pub(super) fn cached_target(
        &self,
        identity: &str,
    ) -> ProcessingResult<Option<TargetRecipeResult>> {
        let connection = self.connect().map_err(internal)?;
        let payload: Option<String> = connection
            .query_row(
                "SELECT payload FROM training_target_cache WHERE cache_identity=?1",
                [identity],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        if payload.is_some() {
            connection
                .execute(
                    "UPDATE training_target_cache SET last_accessed_at=?2 WHERE cache_identity=?1",
                    params![identity, chrono::Utc::now().to_rfc3339()],
                )
                .map_err(internal)?;
        }
        payload
            .map(|json| serde_json::from_str(&json).map_err(internal))
            .transpose()
    }

    pub(super) fn save_target(
        &self,
        pair_id: &str,
        target: &TargetRecipeResult,
    ) -> ProcessingResult<()> {
        let payload = serde_json::to_string(target).map_err(internal)?;
        let now = chrono::Utc::now().to_rfc3339();
        self.connect().map_err(internal)?.execute(
            "INSERT INTO training_target_cache(cache_identity,pair_id,payload,created_at,last_accessed_at) VALUES(?1,?2,?3,?4,?4) ON CONFLICT(cache_identity) DO UPDATE SET payload=excluded.payload,last_accessed_at=excluded.last_accessed_at",
            params![target.cache_identity,pair_id,payload,now],
        ).map_err(internal)?;
        Ok(())
    }

    pub(super) fn save_training_run(&self, run: &TrainingRun) -> ProcessingResult<()> {
        let payload = serde_json::to_string(run).map_err(internal)?;
        self.connect().map_err(internal)?.execute(
            "INSERT INTO training_runs(run_id,dataset_id,status,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(run_id) DO UPDATE SET status=excluded.status,payload=excluded.payload,updated_at=excluded.updated_at",
            params![run.run_id,run.dataset_id,format!("{:?}",run.status).to_ascii_lowercase(),payload,run.started_at,run.updated_at],
        ).map_err(internal)?;
        Ok(())
    }

    pub(super) fn training_run(&self, run_id: &str) -> ProcessingResult<Option<TrainingRun>> {
        let payload: Option<String> = self
            .connect()
            .map_err(internal)?
            .query_row(
                "SELECT payload FROM training_runs WHERE run_id=?1",
                [run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        payload
            .map(|json| serde_json::from_str(&json).map_err(internal))
            .transpose()
    }

    pub(super) fn latest_training_run(
        &self,
        dataset: &str,
    ) -> ProcessingResult<Option<TrainingRun>> {
        let payload: Option<String> = self.connect().map_err(internal)?.query_row(
            "SELECT payload FROM training_runs WHERE dataset_id=?1 ORDER BY updated_at DESC LIMIT 1",
            [dataset],
            |row| row.get(0),
        ).optional().map_err(internal)?;
        payload
            .map(|json| serde_json::from_str(&json).map_err(internal))
            .transpose()
    }
}
