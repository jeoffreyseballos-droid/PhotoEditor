//! Job orchestration only; pixels and decoder policy remain in rendering.
use crate::{
    external::ExifTool,
    paths::same_or_descendant,
    rendering::{self, CpuProcessingEngine},
    repository::JobRepository,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use photo_contracts::*;
use rendering::{internal, io_error};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DevelopmentState {
    #[serde(default)]
    pub diagnostics: ToolkitDiagnostics,
    pub adjustments: RenderAdjustments,
    pub revision: u64,
    pub state: String,
    pub source_identity: Option<String>,
    pub preview_path: Option<PathBuf>,
    pub export_path: Option<PathBuf>,
    pub error: Option<ProcessingError>,
    pub warnings: Vec<String>,
}
#[derive(Serialize)]
pub struct DevelopmentResult {
    pub state: DevelopmentState,
    pub preview_data: Option<String>,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Deserialize)]
pub struct DevelopmentRequest {
    pub job_id: String,
    pub asset_id: String,
    pub request_id: String,
    pub adjustments: RenderAdjustments,
    pub preview: bool,
    pub output_format: OutputFormat,
    pub jpeg_quality: u8,
}
#[derive(Clone, Deserialize)]
pub struct MaskRequest {
    pub job_id: String,
    pub asset_id: String,
    pub request_id: String,
    pub adjustments: RenderAdjustments,
    pub layer_id: Option<String>,
    pub generate: bool,
}
#[derive(Serialize)]
pub struct MaskResult {
    pub diagnostic: MaskDiagnostic,
    pub overlay_data: Option<String>,
}
struct Active {
    id: String,
    token: CancellationToken,
    export: bool,
}
pub struct RenderPermit {
    pub token: CancellationToken,
    count: Arc<AtomicUsize>,
}
impl Drop for RenderPermit {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}
pub struct DevelopmentService {
    repository: JobRepository,
    engine: Arc<CpuProcessingEngine>,
    cache: PathBuf,
    metadata: Option<ExifTool>,
    active: Mutex<Option<Active>>,
    count: Arc<AtomicUsize>,
    worker: Mutex<()>,
}
impl JobRepository {
    pub fn development(&self, job: &str, asset: &str) -> ProcessingResult<DevelopmentState> {
        self.asset(job, asset).map_err(internal)?;
        let db = self.connect().map_err(internal)?;
        let row=db.query_row("SELECT adjustments_json,revision,state,source_identity,preview_path,export_path,error_json,warnings_json FROM development_state WHERE job_id=?1 AND asset_id=?2",params![job,asset],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,String>(7)?))).optional().map_err(internal)?;
        if let Some((a, revision, state, identity, preview, export, error, warnings)) = row {
            let toolkit: String = db
                .query_row(
                    "SELECT toolkit_json FROM development_state WHERE job_id=?1 AND asset_id=?2",
                    params![job, asset],
                    |r| r.get(0),
                )
                .map_err(internal)?;
            let mut diagnostics: ToolkitDiagnostics =
                serde_json::from_str(&toolkit).map_err(internal)?;
            let mask: Option<String> = db
                .query_row(
                    "SELECT diagnostic_json FROM mask_state WHERE job_id=?1 AND asset_id=?2",
                    params![job, asset],
                    |r| r.get(0),
                )
                .optional()
                .map_err(internal)?;
            if let Some(mask) = mask {
                diagnostics.mask = serde_json::from_str(&mask).map_err(internal)?;
                if diagnostics.mask.status == MaskStatus::Generating {
                    diagnostics.mask.status = MaskStatus::Stale;
                }
            }
            Ok(DevelopmentState {
                diagnostics,
                adjustments: serde_json::from_str::<RenderAdjustments>(&a)
                    .map_err(internal)?
                    .validated()?,
                revision,
                state,
                source_identity: identity,
                preview_path: preview.map(PathBuf::from),
                export_path: export.map(PathBuf::from),
                error: error
                    .map(|s| serde_json::from_str(&s))
                    .transpose()
                    .map_err(internal)?,
                warnings: serde_json::from_str(&warnings).map_err(internal)?,
            })
        } else {
            Ok(DevelopmentState {
                state: "source_ready".into(),
                ..Default::default()
            })
        }
    }
    pub fn save_development(
        &self,
        job: &str,
        asset: &str,
        adjustments: &RenderAdjustments,
    ) -> ProcessingResult<DevelopmentState> {
        self.asset(job, asset).map_err(internal)?;
        let adjustments = serde_json::to_string(&adjustments.validated()?).map_err(internal)?;
        self.connect().map_err(internal)?.execute("INSERT INTO development_state(job_id,asset_id,adjustments_json,updated_at) VALUES (?1,?2,?3,?4) ON CONFLICT(job_id,asset_id) DO UPDATE SET adjustments_json=excluded.adjustments_json,revision=development_state.revision+1,state='source_ready',request_id=NULL,preview_path=NULL,error_json=NULL,updated_at=excluded.updated_at",params![job,asset,adjustments,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
        self.development(job, asset)
    }
}
impl DevelopmentService {
    pub fn mask(&self, request: MaskRequest, permit: RenderPermit) -> ProcessingResult<MaskResult> {
        let _guard = self.worker.lock().map_err(internal)?;
        permit.token.check()?;
        let a = request.adjustments.validated()?;
        let asset = self
            .repository
            .asset(&request.job_id, &request.asset_id)
            .map_err(internal)?;
        let layer = request
            .layer_id
            .as_ref()
            .map(|id| {
                a.local_layers
                    .iter()
                    .find(|l| &l.id == id)
                    .ok_or_else(|| internal("Unknown local layer"))
            })
            .transpose()?;
        let persist = |diag: &MaskDiagnostic| -> ProcessingResult<()> {
            self.repository.connect().map_err(internal)?.execute("INSERT INTO mask_state(job_id,asset_id,diagnostic_json,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,asset_id) DO UPDATE SET diagnostic_json=excluded.diagnostic_json,updated_at=excluded.updated_at",params![request.job_id,request.asset_id,serde_json::to_string(diag).map_err(internal)?,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
            Ok(())
        };
        if request.generate {
            persist(&MaskDiagnostic {
                status: MaskStatus::Generating,
                ..Default::default()
            })?;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.engine.mask_preview(
                &asset.original_path,
                &optics_metadata(&asset.metadata),
                &a,
                layer,
                request.generate,
                &permit.token,
            )
        }))
        .unwrap_or_else(|_| Err(internal("Mask worker stopped unexpectedly")));
        match result {
            Ok((diagnostic, overlay)) => {
                persist(&diagnostic)?;
                Ok(MaskResult {
                    diagnostic,
                    overlay_data: overlay
                        .map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes))),
                })
            }
            Err(e) => {
                persist(&MaskDiagnostic {
                    status: if e.code == ProcessingErrorCode::Cancelled {
                        MaskStatus::Stale
                    } else {
                        MaskStatus::Failed
                    },
                    warnings: vec![e.message.clone()],
                    ..Default::default()
                })?;
                Err(e)
            }
        }
    }
    pub fn new(
        repository: JobRepository,
        engine: Arc<CpuProcessingEngine>,
        cache: PathBuf,
        metadata: Option<ExifTool>,
    ) -> ProcessingResult<Self> {
        std::fs::create_dir_all(&cache).map_err(io_error)?;
        Ok(Self {
            repository,
            engine,
            cache: cache.canonicalize().map_err(io_error)?,
            metadata,
            active: Mutex::new(None),
            count: Arc::new(AtomicUsize::new(0)),
            worker: Mutex::new(()),
        })
    }
    pub fn load(&self, job: &str, asset: &str) -> ProcessingResult<DevelopmentState> {
        let mut state = self.repository.development(job, asset)?;
        if state.diagnostics.mask.status == MaskStatus::Ready {
            let source = self.repository.asset(job, asset).map_err(internal)?;
            state.diagnostics.mask = match self.engine.cached_mask_status(&source.original_path) {
                Ok(d) => d,
                Err(e) => MaskDiagnostic {
                    status: MaskStatus::Stale,
                    warnings: vec![e.message],
                    ..Default::default()
                },
            };
        }
        Ok(state)
    }
    pub fn save(
        &self,
        job: &str,
        asset: &str,
        a: &RenderAdjustments,
    ) -> ProcessingResult<DevelopmentState> {
        let _guard = self.worker.lock().map_err(internal)?;
        self.repository.save_development(job, asset, a)
    }
    /// At most one executing render plus one pending replacement. Exports are never implicitly cancelled.
    pub fn reserve(&self, id: &str, preview: bool) -> ProcessingResult<RenderPermit> {
        if id.is_empty() || id.len() > 128 {
            return Err(internal("Invalid render request ID"));
        }
        let mut active = self.active.lock().map_err(internal)?;
        let count = self.count.load(Ordering::Acquire);
        if count >= 2 || (count > 0 && (!preview || active.as_ref().is_some_and(|a| a.export))) {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "A render is already running; cancel or wait for completion",
            ));
        }
        if let Some(previous) = active.as_ref() {
            previous.token.cancel();
        }
        let token = CancellationToken::default();
        *active = Some(Active {
            id: id.into(),
            token: token.clone(),
            export: !preview,
        });
        self.count.fetch_add(1, Ordering::AcqRel);
        Ok(RenderPermit {
            token,
            count: self.count.clone(),
        })
    }
    pub fn cancel(&self, id: &str) -> ProcessingResult<()> {
        if let Some(active) = self.active.lock().map_err(internal)?.as_ref() {
            if active.id == id {
                active.token.cancel();
            }
        }
        Ok(())
    }
    pub fn run(
        &self,
        request: DevelopmentRequest,
        permit: RenderPermit,
    ) -> ProcessingResult<DevelopmentResult> {
        let _guard = self.worker.lock().map_err(internal)?;
        permit.token.check()?;
        let job = self.repository.get_job(&request.job_id).map_err(internal)?;
        let asset = self
            .repository
            .asset(&request.job_id, &request.asset_id)
            .map_err(internal)?;
        let saved = self.repository.save_development(
            &request.job_id,
            &request.asset_id,
            &request.adjustments,
        )?;
        let identity = rendering::source_identity(&asset.original_path)?;
        let db = self.repository.connect().map_err(internal)?;
        db.execute("UPDATE development_state SET state=?3,request_id=?4,source_identity=?5 WHERE job_id=?1 AND asset_id=?2",params![request.job_id,request.asset_id,if request.preview{"rendering_preview"}else{"rendering_export"},request.request_id,identity]).map_err(internal)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.render_job(&request, &saved, &asset, &job, &identity, &permit.token)
        }))
        .unwrap_or_else(|_| {
            Err(internal(
                "Renderer stopped unexpectedly; original source is unchanged",
            ))
        });
        if let Err(error) = &result {
            db.execute("UPDATE development_state SET state=?3,error_json=?4,updated_at=?5 WHERE job_id=?1 AND asset_id=?2 AND revision=?6 AND request_id=?7",params![request.job_id,request.asset_id,if error.code==ProcessingErrorCode::Cancelled{"cancelled"}else{"failed"},serde_json::to_string(error).map_err(internal)?,chrono::Utc::now().to_rfc3339(),saved.revision,request.request_id]).map_err(internal)?;
        }
        result
    }
    fn render_job(
        &self,
        request: &DevelopmentRequest,
        saved: &DevelopmentState,
        asset: &crate::models::Asset,
        job: &crate::models::Job,
        identity: &str,
        cancel: &CancellationToken,
    ) -> ProcessingResult<DevelopmentResult> {
        let key = rendering::preview_key(
            &format!("{}:{}", asset.fingerprint, identity),
            &saved.adjustments,
            self.engine.backend_id(),
        )?;
        let cached = self.cache.join(format!("{key}.jpg"));
        let mut output = None;
        // Re-evaluate availability for optics/local stages; never reuse a fallback-only JPEG
        // after a model/database/cache becomes available. Source/mask caches still apply.
        let dynamic_tools = saved.adjustments.optics.enabled
            || saved
                .adjustments
                .local_layers
                .iter()
                .any(|l| l.enabled && l.strength > 0.);
        let (width, height, warnings, path, diagnostics) = if request.preview
            && !dynamic_tools
            && valid_preview(&cached)
        {
            let (w, h) = image::image_dimensions(&cached).map_err(internal)?;
            let mut warnings = saved.warnings.clone();
            let note = "Cached reduced preview; assess fine detail in export".to_owned();
            if !warnings.contains(&note) {
                warnings.push(note);
            }
            cancel.check()?;
            let mut diagnostics = saved.diagnostics.clone();
            diagnostics.lens = self.engine.optics_diagnostic(
                &optics_metadata(&asset.metadata),
                saved.adjustments.optics,
                w,
                h,
            );
            (w, h, warnings, cached, diagnostics)
        } else {
            let folder = if request.preview {
                self.cache.clone()
            } else {
                let input = job.input_path.canonicalize().map_err(io_error)?;
                let out = job.output_path.canonicalize().map_err(io_error)?;
                if same_or_descendant(&input, &out) || same_or_descendant(&out, &self.cache) {
                    return Err(ProcessingError::new(
                        ProcessingErrorCode::ExportFailed,
                        "Output folder aliases the input or renderer cache",
                    ));
                }
                out
            };
            let temp = tempfile::Builder::new()
                .prefix(".photo-render-")
                .tempdir_in(&folder)
                .map_err(io_error)?;
            let format = if request.preview {
                OutputFormat::Jpeg
            } else {
                request.output_format
            };
            let render_path = temp.path().join(format!("pixels.{}", format.extension()));
            let rendered = self.engine.render_with_metadata(
                &RenderRequest {
                    asset_id: asset.id.clone(),
                    original: asset.original_path.clone(),
                    adjustments: saved.adjustments.clone(),
                    source_metadata: optics_metadata(&asset.metadata),
                    destination: render_path.clone(),
                    output_format: format,
                    preview: request.preview,
                    jpeg_quality: if request.preview {
                        90
                    } else {
                        request.jpeg_quality
                    },
                },
                &optics_metadata(&asset.metadata),
                cancel,
            )?;
            let mut warnings = rendered.warnings;
            let mut publish_source = render_path.clone();
            if !request.preview {
                let tagged = temp.path().join(format!("metadata.{}", format.extension()));
                if let Some(writer) = &self.metadata {
                    let metadata_result = std::fs::copy(&render_path, &tagged)
                        .map_err(|e| e.to_string())
                        .and_then(|_| writer.copy_export_metadata(&asset.original_path, &tagged));
                    match metadata_result {
                        Ok(()) => publish_source = tagged,
                        Err(e) => {
                            warnings.push(format!("Photographic metadata was not preserved: {e}"))
                        }
                    }
                } else {
                    warnings.push("Photographic metadata writer unavailable; pixels and sRGB profile exported".into());
                }
            }
            cancel.check()?;
            if rendering::source_identity(&asset.original_path)? != identity {
                return Err(ProcessingError::new(
                    ProcessingErrorCode::SourceChanged,
                    "Source changed before publication; retry",
                ));
            }
            let publish = rendering::copy_to_publishable(&publish_source, &folder)?;
            cancel.check()?;
            let path = if request.preview {
                if cached.exists() {
                    std::fs::remove_file(&cached).map_err(io_error)?;
                }
                publish
                    .persist_noclobber(&cached)
                    .map_err(|e| io_error(e.error))?;
                cached
            } else {
                let path =
                    rendering::publish_unique(publish, &folder, &asset.original_path, format)?;
                output = Some(path.clone());
                path
            };
            (
                rendered.width,
                rendered.height,
                warnings,
                path,
                rendered.diagnostics,
            )
        };
        let data = if request.preview {
            Some(preview_data(&path)?)
        } else {
            None
        };
        // A published export is authoritative even if cancel arrives immediately afterwards.
        let stage = if request.preview {
            "preview_rendered"
        } else {
            "exported"
        };
        let mut db = self.repository.connect().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        let changed=tx.execute("UPDATE development_state SET state=?3,preview_path=COALESCE(?4,preview_path),export_path=COALESCE(?5,export_path),error_json=NULL,warnings_json=?6,updated_at=?7 WHERE job_id=?1 AND asset_id=?2 AND revision=?8 AND request_id=?9",params![request.job_id,request.asset_id,stage,if request.preview{path.to_str()}else{None},output.as_ref().and_then(|p|p.to_str()),serde_json::to_string(&warnings).map_err(internal)?,chrono::Utc::now().to_rfc3339(),saved.revision,request.request_id]).map_err(internal)?;
        if changed > 0 {
            if saved
                .adjustments
                .local_layers
                .iter()
                .any(|l| l.enabled && l.strength > 0.)
            {
                tx.execute("INSERT INTO mask_state(job_id,asset_id,diagnostic_json,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,asset_id) DO UPDATE SET diagnostic_json=excluded.diagnostic_json,updated_at=excluded.updated_at",params![request.job_id,request.asset_id,serde_json::to_string(&diagnostics.mask).map_err(internal)?,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
            }
            tx.execute(
                "UPDATE development_state SET toolkit_json=?3 WHERE job_id=?1 AND asset_id=?2",
                params![
                    request.job_id,
                    request.asset_id,
                    serde_json::to_string(&diagnostics).map_err(internal)?
                ],
            )
            .map_err(internal)?;
            tx.execute("UPDATE processing_state SET stage=?3,engine_version=?4,updated_at=?5 WHERE job_id=?1 AND asset_id=?2",params![request.job_id,request.asset_id,if request.preview{"rendered"}else{"exported"},rendering::RENDERER_VERSION,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
        }
        tx.commit().map_err(internal)?;
        let state = self
            .repository
            .development(&request.job_id, &request.asset_id)?;
        Ok(DevelopmentResult {
            state,
            preview_data: data,
            width,
            height,
        })
    }
}
pub fn optics_metadata(m: &crate::models::ImageMetadata) -> OpticsMetadata {
    fn number(s: &Option<String>) -> Option<f32> {
        s.as_ref()?
            .split_whitespace()
            .next()?
            .trim_start_matches("f/")
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.)
    }
    OpticsMetadata {
        camera_make: m.camera_make.clone(),
        camera_model: m.camera_model.clone(),
        lens_make: m.lens_make.clone(),
        lens_model: m.lens.clone(),
        focal_length: number(&m.focal_length),
        aperture: number(&m.aperture),
        focus_distance: number(&m.focus_distance),
    }
}
fn valid_preview(path: &std::path::Path) -> bool {
    if !path.metadata().is_ok_and(|m| m.len() <= 16 * 1024 * 1024) {
        return false;
    }
    let Ok(mut reader) = image::ImageReader::open(path) else {
        return false;
    };
    reader.set_format(image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1600);
    limits.max_image_height = Some(1600);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().is_ok()
}
fn preview_data(path: &std::path::Path) -> ProcessingResult<String> {
    if !valid_preview(path) {
        return Err(internal("Rendered preview cache is invalid; render again"));
    }
    Ok(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(std::fs::read(path).map_err(io_error)?)
    ))
}
