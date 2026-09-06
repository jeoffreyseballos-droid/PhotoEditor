use crate::{
    error::{AppError, AppResult, ErrorCode},
    models::{Asset, Job, NewJob, Page},
    warnings::{IngestionWarning, WarningSummary},
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const JOB_SELECT: &str = "SELECT j.id, j.name, j.input_path, j.output_path, j.created_at, j.updated_at, j.status, (SELECT COUNT(*) FROM assets a WHERE a.job_id = j.id), (SELECT COUNT(*) FROM ingestion_warnings w WHERE w.job_id=j.id), j.last_error,
 (SELECT COUNT(*) FROM ingestion_warnings w WHERE w.job_id=j.id AND category='metadata'),
 (SELECT COUNT(*) FROM ingestion_warnings w WHERE w.job_id=j.id AND category='preview'),
 (SELECT COUNT(*) FROM ingestion_warnings w WHERE w.job_id=j.id AND category='unreadable'),
 (SELECT COUNT(*) FROM ingestion_warnings w WHERE w.job_id=j.id AND category='access'),
 (SELECT COUNT(*) FROM ingestion_warnings w WHERE w.job_id=j.id AND category='traversal') FROM jobs j";
const ASSET_SELECT: &str = "SELECT id, job_id, original_path, filename, file_type, file_size, modified_at, fingerprint, metadata_json, thumbnail_path, preview_status, metadata_warning, created_at, warnings_json FROM assets";
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/001_initial.sql"),
    include_str!("../migrations/002_ingestion_warnings.sql"),
    include_str!("../migrations/003_development.sql"),
    include_str!("../migrations/004_toolkit.sql"),
    include_str!("../migrations/005_recipes.sql"),
    include_str!("../migrations/006_photo_analysis.sql"),
    include_str!("../migrations/007_culling.sql"),
    include_str!("../migrations/008_duplicate_content.sql"),
    include_str!("../migrations/009_batch_context.sql"),
    include_str!("../migrations/010_trained_styles.sql"),
    include_str!("../migrations/011_training_studio.sql"),
];

#[derive(Clone)]
pub struct JobRepository {
    path: PathBuf,
}

fn path_text(path: &Path) -> AppResult<&str> {
    path.to_str()
        .ok_or_else(|| AppError::new(ErrorCode::InvalidInput, "A file path is not valid Unicode."))
}

impl JobRepository {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let repository = Self { path };
        let mut connection = repository.connect()?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let version: usize =
            connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > MIGRATIONS.len() {
            return Err(AppError::new(
                ErrorCode::Database,
                "This database was created by a newer application. Update Photo Editor to open it.",
            ));
        }
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(version) {
            let transaction = connection.transaction()?;
            transaction.execute_batch(migration)?;
            transaction.pragma_update(None, "user_version", index + 1)?;
            transaction.commit()?;
        }
        Ok(repository)
    }

    pub(crate) fn connect(&self) -> AppResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }

    pub fn recover_interrupted(&self) -> AppResult<()> {
        self.connect()?.execute("UPDATE training_runs SET status='interrupted', payload=json_set(payload,'$.status','interrupted','$.stage','stopped','$.error','Training was interrupted; cached pair work remains available.') WHERE status IN ('queued','running')", [])?;
        self.connect()?.execute("UPDATE trained_style_runs SET payload=json_set(payload,'$.status','interrupted','$.stage','Interrupted; completed recipes remain available.') WHERE json_extract(payload,'$.status') IN ('queued','running')", [])?;
        self.connect()?.execute("UPDATE batch_context_runs SET payload=json_set(payload,'$.status','interrupted','$.stage','Interrupted; cached context remains available.') WHERE json_extract(payload,'$.status') IN ('queued','running')", [])?;
        self.connect()?.execute("UPDATE culling_runs SET payload=json_set(payload,'$.status','interrupted','$.stage','Interrupted; completed ratings preserved. Resume culling.') WHERE json_extract(payload,'$.status') IN ('queued','running')", [])?;
        self.connect()?.execute("UPDATE analysis_status SET state='interrupted', request_id=NULL, error='Analysis interrupted; rerun safely.' WHERE state IN ('queued','analyzing')", [])?;
        self.connect()?.execute("UPDATE development_state SET state='interrupted', request_id=NULL WHERE state IN ('rendering_preview','rendering_export')", [])?;
        self.connect()?.execute("UPDATE jobs SET status = 'interrupted', updated_at = ?1, last_error = 'Scanning was interrupted. Resume to continue.' WHERE status = 'scanning'", [Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn create_job(&self, input: &NewJob) -> AppResult<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.connect()?.execute("INSERT INTO jobs (id, name, input_path, output_path, created_at, updated_at, status) VALUES (?1, ?2, ?3, ?4, ?5, ?5, 'scanning')", params![id, input.name.trim(), path_text(&input.input_path)?, path_text(&input.output_path)?, now])?;
        self.get_job(&id)
    }

    pub fn get_job(&self, id: &str) -> AppResult<Job> {
        self.connect()?
            .query_row(&format!("{JOB_SELECT} WHERE j.id = ?1"), [id], job_row)
            .optional()?
            .ok_or_else(|| {
                AppError::new(
                    ErrorCode::NotFound,
                    "This job was not found on this computer.",
                )
            })
    }

    pub fn list_jobs(&self, offset: u32, limit: u32) -> AppResult<Page<Job>> {
        let limit = limit.clamp(1, 100);
        let connection = self.connect()?;
        let total = connection.query_row(
            "SELECT COUNT(*) FROM jobs WHERE name NOT LIKE '__training__%'",
            [],
            |row| row.get(0),
        )?;
        let mut statement = connection.prepare(&format!(
            "{JOB_SELECT} WHERE j.name NOT LIKE '__training__%' ORDER BY j.updated_at DESC, j.id LIMIT ?1 OFFSET ?2"
        ))?;
        let items = statement
            .query_map(params![limit, offset], job_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Page {
            items,
            total,
            offset,
            limit,
        })
    }

    pub fn set_status(
        &self,
        id: &str,
        status: &str,
        warnings: u64,
        error: Option<&str>,
    ) -> AppResult<()> {
        self.connect()?.execute("UPDATE jobs SET status = ?2, warning_count = ?3, last_error = ?4, updated_at = ?5 WHERE id = ?1", params![id, status, warnings, error, Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn save_assets(&self, assets: &[Asset]) -> AppResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        for asset in assets {
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT fingerprint FROM assets WHERE job_id = ?1 AND id = ?2",
                    params![asset.job_id, asset.id],
                    |row| row.get(0),
                )
                .optional()?;
            let changed = existing
                .as_ref()
                .is_some_and(|value| value != &asset.fingerprint);
            transaction.execute("INSERT INTO assets (id, job_id, original_path, filename, file_type, file_size, modified_at, fingerprint, metadata_json, thumbnail_path, preview_status, metadata_warning, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) ON CONFLICT(job_id, id) DO UPDATE SET original_path=excluded.original_path, filename=excluded.filename, file_type=excluded.file_type, file_size=excluded.file_size, modified_at=excluded.modified_at, fingerprint=excluded.fingerprint, metadata_json=excluded.metadata_json, thumbnail_path=excluded.thumbnail_path, preview_status=excluded.preview_status, metadata_warning=excluded.metadata_warning", params![asset.id, asset.job_id, path_text(&asset.original_path)?, asset.filename, asset.file_type.extension(), asset.file_size, asset.modified_at, asset.fingerprint, serde_json::to_string(&asset.metadata)?, asset.thumbnail_path.as_deref().map(path_text).transpose()?, asset.preview_status, asset.metadata_warning, asset.created_at])?;
            transaction.execute(
                "UPDATE assets SET warnings_json=?3 WHERE job_id=?1 AND id=?2",
                params![
                    asset.job_id,
                    asset.id,
                    serde_json::to_string(&asset.warnings)?
                ],
            )?;
            transaction.execute(
                "DELETE FROM ingestion_warnings WHERE job_id=?1 AND asset_id=?2",
                params![asset.job_id, asset.id],
            )?;
            for warning in &asset.warnings {
                transaction.execute("INSERT INTO ingestion_warnings(job_id,asset_id,category,code,message,path) VALUES (?1,?2,?3,?4,?5,?6)", params![asset.job_id,asset.id,warning.category.as_str(),warning.code,warning.message,warning.path.as_deref().map(path_text).transpose()?])?;
            }
            let stage = if asset.preview_status == "ready" {
                "preview_generated"
            } else {
                "discovered"
            };
            transaction.execute("INSERT INTO processing_state (job_id, asset_id, stage, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(job_id, asset_id) DO NOTHING", params![asset.job_id, asset.id, stage, Utc::now().to_rfc3339()])?;
            // Only Phase 1 stages are written. Preserve future progress for unchanged originals.
            if changed {
                transaction.execute("UPDATE processing_state SET stage=?3, attempt_count=0, lease_owner=NULL, lease_expires_at=NULL, last_error_json=NULL, recipe_json=NULL, analysis_json=NULL, style_id=NULL, engine_version=NULL, updated_at=?4 WHERE job_id=?1 AND asset_id=?2", params![asset.job_id, asset.id, stage, Utc::now().to_rfc3339()])?;
            } else {
                transaction.execute("UPDATE processing_state SET stage=?3, updated_at=?4 WHERE job_id=?1 AND asset_id=?2 AND stage IN ('discovered', 'preview_generated')", params![asset.job_id, asset.id, stage, Utc::now().to_rfc3339()])?;
            }
        }
        for job_id in assets
            .iter()
            .map(|asset| &asset.job_id)
            .collect::<std::collections::BTreeSet<_>>()
        {
            transaction.execute(
                "UPDATE jobs SET updated_at=?2 WHERE id=?1",
                params![job_id, Utc::now().to_rfc3339()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn assets(&self, job_id: &str, offset: u32, limit: u32) -> AppResult<Page<Asset>> {
        self.get_job(job_id)?;
        let limit = limit.clamp(1, 100);
        let connection = self.connect()?;
        let total = connection.query_row(
            "SELECT COUNT(*) FROM assets WHERE job_id = ?1",
            [job_id],
            |row| row.get(0),
        )?;
        let mut statement = connection.prepare(&format!("{ASSET_SELECT} WHERE job_id = ?1 ORDER BY filename COLLATE NOCASE, id LIMIT ?2 OFFSET ?3"))?;
        let items = statement
            .query_map(params![job_id, limit, offset], asset_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Page {
            items,
            total,
            offset,
            limit,
        })
    }

    pub fn asset(&self, job_id: &str, id: &str) -> AppResult<Asset> {
        self.connect()?
            .query_row(
                &format!("{ASSET_SELECT} WHERE job_id = ?1 AND id = ?2"),
                params![job_id, id],
                asset_row,
            )
            .optional()?
            .ok_or_else(|| {
                AppError::new(ErrorCode::NotFound, "This photo was not found in the job.")
            })
    }

    pub fn clear_traversal_warnings(&self, job_id: &str) -> AppResult<()> {
        self.connect()?.execute(
            "DELETE FROM ingestion_warnings WHERE job_id=?1 AND asset_id IS NULL",
            [job_id],
        )?;
        Ok(())
    }
    pub fn save_scan_warning(&self, job_id: &str, warning: &IngestionWarning) -> AppResult<()> {
        self.connect()?.execute("INSERT INTO ingestion_warnings(job_id,category,code,message,path) VALUES (?1,?2,?3,?4,?5)", params![job_id,warning.category.as_str(),warning.code,warning.message,warning.path.as_deref().map(path_text).transpose()?])?;
        Ok(())
    }
    pub fn warnings(
        &self,
        job_id: &str,
        offset: u32,
        limit: u32,
    ) -> AppResult<Page<IngestionWarning>> {
        self.get_job(job_id)?;
        let limit = limit.clamp(1, 100);
        let connection = self.connect()?;
        let total = connection.query_row(
            "SELECT COUNT(*) FROM ingestion_warnings WHERE job_id=?1",
            [job_id],
            |r| r.get(0),
        )?;
        let mut statement = connection.prepare("SELECT category,code,message,path FROM ingestion_warnings WHERE job_id=?1 ORDER BY id LIMIT ?2 OFFSET ?3")?;
        let items = statement
            .query_map(params![job_id, limit, offset], |row| {
                let category: String = row.get(0)?;
                let category = serde_json::from_value(serde_json::Value::String(category))
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                Ok(IngestionWarning {
                    category,
                    code: row.get(1)?,
                    message: row.get(2)?,
                    path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Page {
            items,
            total,
            offset,
            limit,
        })
    }
}

fn job_row(row: &Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get(0)?,
        name: row.get(1)?,
        input_path: PathBuf::from(row.get::<_, String>(2)?),
        output_path: PathBuf::from(row.get::<_, String>(3)?),
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        status: row.get(6)?,
        asset_count: row.get(7)?,
        warning_count: row.get(8)?,
        last_error: row.get(9)?,
        warnings: WarningSummary {
            metadata: row.get(10)?,
            preview: row.get(11)?,
            unreadable: row.get(12)?,
            access: row.get(13)?,
            traversal: row.get(14)?,
        },
    })
}

fn asset_row(row: &Row<'_>) -> rusqlite::Result<Asset> {
    let json: String = row.get(8)?;
    let metadata = serde_json::from_str(&json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let extension: String = row.get(4)?;
    let file_type = crate::discovery::supported_type(&PathBuf::from(format!("image.{extension}")))
        .ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(4, "file_type".into(), rusqlite::types::Type::Text)
        })?;
    Ok(Asset {
        id: row.get(0)?,
        job_id: row.get(1)?,
        original_path: PathBuf::from(row.get::<_, String>(2)?),
        filename: row.get(3)?,
        file_type,
        file_size: row.get(5)?,
        modified_at: row.get(6)?,
        fingerprint: row.get(7)?,
        metadata,
        thumbnail_path: row.get::<_, Option<String>>(9)?.map(PathBuf::from),
        preview_status: row.get(10)?,
        metadata_warning: row.get(11)?,
        created_at: row.get(12)?,
        warnings: serde_json::from_str(&row.get::<_, String>(13)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(13, rusqlite::types::Type::Text, Box::new(e))
        })?,
    })
}
