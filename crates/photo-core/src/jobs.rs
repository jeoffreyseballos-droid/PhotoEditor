use crate::{
    discovery::{canonical_directory, inspect_file, FileDiscovery},
    error::{AppError, AppResult, ErrorCode},
    external::ExifTool,
    metadata::{BasicMetadataExtractor, MetadataExtractor, MetadataResult},
    models::{Asset, DiscoveredFile, Job, NewJob, Page},
    paths::same_or_descendant,
    repository::JobRepository,
    thumbnails::{
        valid_cached_thumbnail, ThumbnailResult, ThumbnailService, CACHE_VERSION, MAX_CACHED_BYTES,
    },
    warnings::{IngestionWarning, WarningCategory},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use std::{
    fs,
    io::Read,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// RAII permit prevents concurrent scans and is released even after an error/panic.
pub struct ScanPermit {
    busy: Arc<AtomicBool>,
}
impl Drop for ScanPermit {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::Release);
    }
}

pub struct JobService {
    pub repository: JobRepository,
    metadata: Box<dyn MetadataExtractor>,
    thumbnails: ThumbnailService,
    thumbnail_worker: Mutex<()>,
    busy: Arc<AtomicBool>,
    excluded: Vec<PathBuf>,
}

impl JobService {
    pub fn new(data_root: PathBuf, cache_root: PathBuf) -> AppResult<Self> {
        Self::with_metadata(data_root, cache_root, Box::new(BasicMetadataExtractor))
    }

    pub fn with_exiftool(
        data_root: PathBuf,
        cache_root: PathBuf,
        helper_root: PathBuf,
    ) -> AppResult<Self> {
        let helper = ExifTool::new(helper_root);
        let mut service = Self::with_metadata(data_root, cache_root, Box::new(helper.clone()))?;
        service.thumbnails.set_preview_provider(Box::new(helper));
        Ok(service)
    }

    pub fn with_metadata(
        data_root: PathBuf,
        cache_root: PathBuf,
        metadata: Box<dyn MetadataExtractor>,
    ) -> AppResult<Self> {
        fs::create_dir_all(&data_root)?;
        let repository = JobRepository::open(data_root.join("jobs.sqlite3"))?;
        let thumbnails = ThumbnailService::new(cache_root.join("thumbnails"))?;
        let excluded = vec![
            fs::canonicalize(data_root)?,
            fs::canonicalize(thumbnails.root())?,
        ];
        Ok(Self {
            repository,
            metadata,
            thumbnails,
            thumbnail_worker: Mutex::new(()),
            busy: Arc::new(AtomicBool::new(false)),
            excluded,
        })
    }

    fn reserve_scan(&self) -> AppResult<ScanPermit> {
        self.busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| AppError::new(ErrorCode::Busy, "A local scan or preview repair is running. Wait for it to finish before starting this job."))?;
        Ok(ScanPermit {
            busy: self.busy.clone(),
        })
    }

    pub fn create(&self, mut input: NewJob) -> AppResult<(Job, ScanPermit)> {
        let permit = self.reserve_scan()?;
        input.name = input.name.trim().to_owned();
        if input.name.is_empty() || input.name.chars().count() > 120 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Enter a job name between 1 and 120 characters.",
            ));
        }
        input.input_path = canonical_directory(&input.input_path)?;
        input.output_path = canonical_directory(&input.output_path)?;
        if same_or_descendant(&input.input_path, &input.output_path) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Input and output cannot be the same folder, and input cannot be inside output. Output may be inside input; its entire subtree will be excluded.",
            ));
        }
        if self.excluded.iter().any(|path| {
            same_or_descendant(&input.input_path, path)
                || same_or_descendant(&input.output_path, path)
        }) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Choose photo folders outside the application's internal storage.",
            ));
        }
        // Read-access check; Phase 1 deliberately never writes into the output directory.
        fs::read_dir(&input.input_path)?;
        fs::read_dir(&input.output_path)?;
        let job = self.repository.create_job(&input)?;
        tracing::info!(target: "application", job_id = %job.id, "Created local job");
        Ok((job, permit))
    }

    pub fn resume(&self, id: &str) -> AppResult<(Job, ScanPermit)> {
        let permit = self.reserve_scan()?;
        let job = self.repository.get_job(id)?;
        canonical_directory(&job.input_path)?;
        let output = canonical_directory(&job.output_path)?;
        if same_or_descendant(&canonical_directory(&job.input_path)?, &output) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "The input folder is now inside or identical to the output folder.",
            ));
        }
        self.repository.set_status(id, "scanning", 0, None)?;
        Ok((self.repository.get_job(id)?, permit))
    }

    /// Call on a background worker. Persistent checkpoints are committed every 32 assets.
    /// Retrying scans is idempotent. Removed source paths remain catalogued, never deleted.
    pub fn scan(&self, id: &str, _permit: ScanPermit) -> AppResult<()> {
        let result = catch_unwind(AssertUnwindSafe(|| self.scan_inner(id)));
        let result = result.unwrap_or_else(|_| {
            Err(AppError::new(
                ErrorCode::Internal,
                "Scanning stopped unexpectedly. You can resume this job.",
            ))
        });
        if let Err(error) = &result {
            tracing::error!(target: "scanning", job_id = id, error = %error, "Scan stopped");
            let warnings = self
                .repository
                .get_job(id)
                .map(|job| job.warning_count)
                .unwrap_or(0);
            self.repository
                .set_status(id, "failed", warnings, Some(&error.message))?;
        }
        result
    }

    fn scan_inner(&self, id: &str) -> AppResult<()> {
        let job = self.repository.get_job(id)?;
        let mut excluded = self.excluded.clone();
        excluded.push(canonical_directory(&job.output_path)?);
        if same_or_descendant(
            &canonical_directory(&job.input_path)?,
            excluded.last().unwrap(),
        ) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "The input folder is now inside or identical to the output folder.",
            ));
        }
        let files = FileDiscovery::new(&job.input_path, excluded)?;
        self.repository.clear_traversal_warnings(id)?;
        let mut batch = Vec::with_capacity(32);
        let mut warnings = 0;
        for file in files {
            match file {
                Ok(file) => {
                    // A decoder panic is isolated to this asset, just like a corrupt-file error.
                    let mut asset =
                        catch_unwind(AssertUnwindSafe(|| self.inspect_asset(id, &file)))
                            .unwrap_or_else(|_| self.failed_asset(id, &file));
                    // Detect an original being replaced while metadata/preview was read.
                    if file.discovery_warning.is_none()
                        && !inspect_file(&file.original_path)
                            .is_ok_and(|current| current.fingerprint == file.fingerprint)
                    {
                        asset.thumbnail_path = None;
                        asset.preview_status = "failed".into();
                        asset.metadata_warning = Some("The original changed or disappeared during scanning. Rescan to refresh it.".into());
                        asset.warnings.push(IngestionWarning::new(WarningCategory::Unreadable, "source_changed", "The original changed or disappeared during scanning. Rescan to refresh it.").at(file.original_path.clone()));
                    }
                    warnings += asset.warnings.len() as u64;
                    batch.push(asset);
                }
                Err(error) => {
                    warnings += 1;
                    tracing::warn!(target: "scanning", job_id = id, warning = ?error, "Entry skipped");
                    self.repository.save_scan_warning(id, &error)?;
                }
            }
            if batch.len() >= 32 {
                self.repository.save_assets(&batch)?;
                batch.clear();
                self.repository.set_status(id, "scanning", warnings, None)?;
            }
        }
        self.repository.save_assets(&batch)?;
        self.repository.set_status(id, "ready", warnings, None)?;
        tracing::info!(target: "scanning", job_id = id, warnings, "Scan finished");
        Ok(())
    }

    fn inspect_asset(&self, job_id: &str, file: &DiscoveredFile) -> Asset {
        let unreadable = file.discovery_warning.clone().or_else(|| {
            fs::File::open(&file.original_path).err().map(|e| {
                IngestionWarning::new(
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        WarningCategory::Access
                    } else {
                        WarningCategory::Unreadable
                    },
                    "file_open_failed",
                    format!("The original could not be opened: {e}"),
                )
                .at(file.original_path.clone())
            })
        });
        if let Some(warning) = unreadable {
            let mut asset = self.failed_asset(job_id, file);
            asset.metadata_warning = None;
            asset.warnings = vec![warning];
            return asset;
        }
        let metadata = catch_unwind(AssertUnwindSafe(|| self.metadata.extract(file))).unwrap_or_else(|_| MetadataResult { metadata: Default::default(), warning: Some("Metadata parser stopped unexpectedly; the asset and preview path remain available.".into()) });
        let _worker = self
            .thumbnail_worker
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let thumbnail = catch_unwind(AssertUnwindSafe(|| {
            self.thumbnails
                .generate(file, metadata.metadata.orientation)
        }))
        .unwrap_or_else(|_| ThumbnailResult {
            path: None,
            status: "failed",
            warning: Some(
                "Preview decoder stopped unexpectedly; available metadata is retained.".into(),
            ),
        });
        let mut warnings = Vec::new();
        if let Some(message) = &metadata.warning {
            warnings.push(
                IngestionWarning::new(
                    WarningCategory::Metadata,
                    "metadata_partial",
                    message.clone(),
                )
                .at(file.original_path.clone()),
            );
        }
        if let Some(message) = &thumbnail.warning {
            warnings.push(
                IngestionWarning::new(
                    WarningCategory::Preview,
                    if thumbnail.status == "unavailable" {
                        "preview_capability"
                    } else {
                        "preview_failed"
                    },
                    message.clone(),
                )
                .at(file.original_path.clone()),
            );
        }
        if metadata.metadata.width.is_none()
            && thumbnail.status == "failed"
            && matches!(
                file.file_type.format().family,
                photo_contracts::formats::FormatFamily::Jpeg
                    | photo_contracts::formats::FormatFamily::Png
                    | photo_contracts::formats::FormatFamily::Tiff
            )
        {
            warnings.push(IngestionWarning::new(WarningCategory::Unreadable, "invalid_image_header", "The still-image header could not be read; the file may be corrupt or use an unsupported encoding.").at(file.original_path.clone()));
        }
        Asset {
            id: file.id.clone(),
            job_id: job_id.into(),
            original_path: file.original_path.clone(),
            filename: file.filename.clone(),
            file_type: file.file_type,
            file_size: file.file_size,
            modified_at: file.modified_at.clone(),
            fingerprint: file.fingerprint.clone(),
            metadata: metadata.metadata,
            thumbnail_path: thumbnail.path,
            preview_status: thumbnail.status.into(),
            metadata_warning: metadata.warning,
            warnings,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    fn failed_asset(&self, job_id: &str, file: &DiscoveredFile) -> Asset {
        tracing::warn!(target: "metadata", asset_id = %file.id, "Image inspection failed; retaining placeholder");
        Asset {
            id: file.id.clone(),
            job_id: job_id.into(),
            original_path: file.original_path.clone(),
            filename: file.filename.clone(),
            file_type: file.file_type,
            file_size: file.file_size,
            modified_at: file.modified_at.clone(),
            fingerprint: file.fingerprint.clone(),
            metadata: Default::default(),
            thumbnail_path: None,
            preview_status: "failed".into(),
            metadata_warning: Some(
                "This image could not be inspected. Other photos were not affected.".into(),
            ),
            warnings: vec![IngestionWarning::new(
                WarningCategory::Metadata,
                "inspection_panic",
                "This image could not be inspected. Other photos were not affected.",
            )
            .at(file.original_path.clone())],
            created_at: Utc::now().to_rfc3339(),
        }
    }

    /// Rebuild only missing/corrupt cached previews on the requested page, not the full job.
    pub fn assets(&self, id: &str, offset: u32, limit: u32) -> AppResult<Page<Asset>> {
        let mut page = self.repository.assets(id, offset, limit)?;
        // A scan owns the authoritative snapshot. Avoid racing page repairs with scan writes.
        let _permit = match self.reserve_scan() {
            Ok(permit) => permit,
            Err(_) => return Ok(page),
        };
        let mut repairs = Vec::new();
        for asset in &mut page.items {
            if asset.preview_status == "ready"
                && !asset.thumbnail_path.as_deref().is_some_and(|path| {
                    valid_cached_thumbnail(path)
                        && path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with(&format!("{CACHE_VERSION}-")))
                })
            {
                match inspect_file(&asset.original_path) {
                    Ok(file) => {
                        let created_at = asset.created_at.clone();
                        *asset = catch_unwind(AssertUnwindSafe(|| self.inspect_asset(id, &file)))
                            .unwrap_or_else(|_| self.failed_asset(id, &file));
                        asset.created_at = created_at;
                    }
                    Err(_) => {
                        asset.thumbnail_path = None;
                        asset.preview_status = "failed".into();
                        asset.metadata_warning = Some("The original is unavailable. Reconnect its drive and rescan to rebuild this preview.".into());
                        asset
                            .warnings
                            .retain(|w| w.category != WarningCategory::Unreadable);
                        asset.warnings.push(IngestionWarning::new(WarningCategory::Unreadable, "source_missing", "The original is unavailable. Reconnect its drive and rescan to rebuild this preview.").at(asset.original_path.clone()));
                    }
                }
                repairs.push(asset.clone());
            }
        }
        if !repairs.is_empty() {
            self.repository.save_assets(&repairs)?;
        }
        Ok(page)
    }

    /// IPC accepts IDs, never arbitrary filesystem paths. No filesystem scope is given to React.
    pub fn thumbnail_data(&self, job_id: &str, asset_id: &str) -> AppResult<Option<String>> {
        let asset = self.repository.asset(job_id, asset_id)?;
        if asset.preview_status != "ready" {
            return Ok(None);
        }
        let file = DiscoveredFile {
            id: asset.id,
            original_path: asset.original_path,
            filename: asset.filename,
            file_type: asset.file_type,
            file_size: asset.file_size,
            modified_at: asset.modified_at,
            fingerprint: asset.fingerprint,
            discovery_warning: None,
        };
        // IDs are digests produced internally. Reject tampered database values before path joining.
        if [&file.id, &file.fingerprint]
            .iter()
            .any(|value| value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(AppError::new(
                ErrorCode::Database,
                "The cached photo identifier is invalid.",
            ));
        }
        let expected = self.thumbnails.cache_path(&file);
        let Ok(canonical) = fs::canonicalize(expected) else {
            return Ok(None);
        };
        let cache_root = fs::canonicalize(self.thumbnails.root())?;
        if !canonical.starts_with(cache_root) || !valid_cached_thumbnail(&canonical) {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        fs::File::open(canonical)?
            .take(MAX_CACHED_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_CACHED_BYTES {
            return Ok(None);
        }
        Ok(Some(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(bytes)
        )))
    }
}
