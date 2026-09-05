//! Source analysis orchestration. Never reads or writes editing intent.
pub mod measure;
mod storage;
use crate::{
    models::Asset,
    rendering::{self, internal, io_error, masks::MaskCache, CpuProcessingEngine},
    repository::JobRepository,
};
use photo_contracts::{
    analysis::*,
    formats::{photo_format, FormatFamily},
    CancellationToken, ProcessingError, ProcessingErrorCode, ProcessingResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
pub const ANALYSIS_ENGINE_VERSION: &str = "photo-analysis-cpu-v1";
pub const ANALYSIS_INPUT_VERSION: &str =
    "normalized-unedited-linear-srgb-oriented-half-raw-edge1600-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    NotAnalyzed,
    Queued,
    Analyzing,
    Complete,
    Warning,
    Failed,
    Cancelled,
    Interrupted,
}
impl AnalysisStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotAnalyzed => "not_analyzed",
            Self::Queued => "queued",
            Self::Analyzing => "analyzing",
            Self::Complete => "complete",
            Self::Warning => "warning",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "queued" => Self::Queued,
            "analyzing" => Self::Analyzing,
            "complete" => Self::Complete,
            "warning" => Self::Warning,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "interrupted" => Self::Interrupted,
            _ => Self::NotAnalyzed,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisState {
    pub status: AnalysisStatus,
    pub analysis: Option<PhotoAnalysis>,
    pub cached: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisRequest {
    pub job_id: String,
    pub asset_id: String,
    pub photo_type: PhotoType,
    pub request_id: String,
}
struct Active {
    request: AnalysisRequest,
    token: CancellationToken,
}
type ActiveRequests = Arc<Mutex<HashMap<String, Active>>>;
pub struct AnalysisPermit {
    request: AnalysisRequest,
    token: CancellationToken,
    active: ActiveRequests,
    repository: JobRepository,
}
impl Drop for AnalysisPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            if let Ok((AnalysisStatus::Queued | AnalysisStatus::Analyzing, _)) =
                self.repository.last_analysis_status(
                    &self.request.job_id,
                    &self.request.asset_id,
                    self.request.photo_type,
                )
            {
                let _ = self.repository.analysis_status(
                    &self.request,
                    AnalysisStatus::Cancelled,
                    Some("Analysis reservation released before completion"),
                );
            }
            active.remove(&self.request.request_id);
        }
    }
}
pub struct AnalysisService {
    repository: JobRepository,
    engine: Arc<CpuProcessingEngine>,
    masks: Option<MaskCache>,
    active: ActiveRequests,
    worker: Mutex<()>,
}
fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    format!("{:x}", h.finalize())
}
pub fn cache_identity(
    source: &str,
    engine: &str,
    decoder: &str,
    kind: PhotoType,
    models: &str,
) -> String {
    digest(&[
        source,
        engine,
        decoder,
        ANALYSIS_INPUT_VERSION,
        &PHOTO_ANALYSIS_SCHEMA_VERSION.to_string(),
        kind.as_str(),
        models,
    ])
}
impl AnalysisService {
    pub fn new(
        repository: JobRepository,
        engine: Arc<CpuProcessingEngine>,
        masks: Option<MaskCache>,
    ) -> Self {
        Self {
            repository,
            engine,
            masks,
            active: Arc::new(Mutex::new(HashMap::new())),
            worker: Mutex::new(()),
        }
    }
    fn models(&self, kind: PhotoType) -> String {
        if kind == PhotoType::Portrait {
            format!(
                "renderer:{};analysis:{};faces:none",
                self.engine.analysis_mask_version(),
                self.masks
                    .as_ref()
                    .map(|m| m.provider_version())
                    .unwrap_or("cached-subject-only")
            )
        } else {
            "subject:skipped;faces:skipped;sky:none".into()
        }
    }
    fn identity(&self, a: &Asset) -> ProcessingResult<(String, String)> {
        let identity = rendering::source_identity(&a.original_path)?;
        // Include cached ingestion metadata so repaired metadata updates source observations.
        let metadata = serde_json::to_string(&a.metadata).map_err(internal)?;
        Ok((identity.clone(), digest(&[&identity, &metadata])))
    }
    fn key(&self, source: &str, kind: PhotoType) -> String {
        cache_identity(
            source,
            ANALYSIS_ENGINE_VERSION,
            self.engine.backend_id(),
            kind,
            &self.models(kind),
        )
    }
    pub fn reserve(&self, request: AnalysisRequest) -> ProcessingResult<AnalysisPermit> {
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err(internal("Invalid analysis request ID"));
        }
        self.repository
            .asset(&request.job_id, &request.asset_id)
            .map_err(internal)?;
        let mut active = self.active.lock().map_err(internal)?;
        if active.len() >= 2
            || active.contains_key(&request.request_id)
            || active.values().any(|a| {
                a.request.job_id == request.job_id && a.request.asset_id == request.asset_id
            })
        {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "Analysis queue is full or this asset is already queued",
            ));
        }
        let token = CancellationToken::default();
        self.repository
            .analysis_status(&request, AnalysisStatus::Queued, None)?;
        active.insert(
            request.request_id.clone(),
            Active {
                request: request.clone(),
                token: token.clone(),
            },
        );
        Ok(AnalysisPermit {
            request,
            token,
            active: self.active.clone(),
            repository: self.repository.clone(),
        })
    }
    pub fn cancel(&self, id: &str) -> ProcessingResult<()> {
        if let Some(a) = self.active.lock().map_err(internal)?.get(id) {
            a.token.cancel();
        }
        Ok(())
    }
    pub fn get_analysis(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
    ) -> ProcessingResult<AnalysisState> {
        let a = self.repository.asset(job, asset).map_err(internal)?;
        let (_, source) = self.identity(&a)?;
        let analysis =
            self.repository
                .analysis_record(job, asset, kind, &self.key(&source, kind))?;
        let (mut status, error) = self.repository.last_analysis_status(job, asset, kind)?;
        if matches!(status, AnalysisStatus::Complete | AnalysisStatus::Warning)
            && analysis.is_none()
        {
            status = AnalysisStatus::NotAnalyzed;
        }
        Ok(AnalysisState {
            status,
            cached: analysis.is_some(),
            analysis,
            error,
        })
    }
    pub fn invalidate_analysis(&self, job: &str, asset: &str) -> ProcessingResult<()> {
        let active = self.active.lock().map_err(internal)?;
        if active
            .values()
            .any(|a| a.request.job_id == job && a.request.asset_id == asset)
        {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "Cancel and await analysis before invalidating it",
            ));
        }
        self.repository.clear_analysis(job, asset)
    }
    pub fn export_analysis(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
    ) -> ProcessingResult<PathBuf> {
        let state = self.get_analysis(job, asset, kind)?;
        let a = state
            .analysis
            .ok_or_else(|| internal("No current source analysis to export"))?;
        self.repository
            .export_analysis_json(job, asset, &a.canonical_json().map_err(internal)?)
    }
    pub fn analyze_asset(&self, permit: AnalysisPermit) -> ProcessingResult<AnalysisState> {
        if !Arc::ptr_eq(&self.active, &permit.active) {
            return Err(internal("Analysis permit belongs to a different service"));
        }
        let _guard = rendering::analysis_input::lock_cancellable(&self.worker, &permit.token)?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run(&permit)))
            .unwrap_or_else(|_| {
                Err(internal(
                    "Analysis worker stopped unexpectedly; source and recipes are unchanged",
                ))
            });
        if let Err(e) = &result {
            self.repository.analysis_status(
                &permit.request,
                if e.code == ProcessingErrorCode::Cancelled {
                    AnalysisStatus::Cancelled
                } else {
                    AnalysisStatus::Failed
                },
                Some(&e.message),
            )?;
        }
        result
    }
    fn run(&self, permit: &AnalysisPermit) -> ProcessingResult<AnalysisState> {
        let r = &permit.request;
        let cancel = &permit.token;
        cancel.check()?;
        let started = Instant::now();
        let a = self
            .repository
            .asset(&r.job_id, &r.asset_id)
            .map_err(internal)?;
        let (identity, source) = self.identity(&a)?;
        let key = self.key(&source, r.photo_type);
        if let Some(analysis) =
            self.repository
                .analysis_record(&r.job_id, &r.asset_id, r.photo_type, &key)?
        {
            cancel.check()?;
            let status = completed_status(&analysis);
            self.repository.analysis_status(r, status, None)?;
            return Ok(AnalysisState {
                status,
                analysis: Some(analysis),
                cached: true,
                error: None,
            });
        }
        self.repository
            .analysis_status(r, AnalysisStatus::Analyzing, None)?;
        let common_key = digest(&[
            &source,
            ANALYSIS_ENGINE_VERSION,
            self.engine.backend_id(),
            ANALYSIS_INPUT_VERSION,
            &PHOTO_ANALYSIS_SCHEMA_VERSION.to_string(),
        ]);
        let cached_common = self
            .repository
            .common_analysis(&r.job_id, &r.asset_id, &common_key)?;
        let reused = cached_common.is_some();
        let input = if !reused || r.photo_type == PhotoType::Portrait {
            Some(self.engine.analysis_input(&a.original_path, cancel)?)
        } else {
            None
        };
        let common = if let Some(c) = cached_common {
            c
        } else {
            let i = input
                .as_ref()
                .ok_or_else(|| internal("Missing analysis input"))?;
            let m = &a.metadata;
            let source = AnalysisSource {
                width: i.image.width,
                height: i.image.height,
                metadata_width: m.width,
                metadata_height: m.height,
                exif_orientation: m.orientation,
                camera_make: m.camera_make.clone(),
                camera_model: m.camera_model.clone(),
                lens: m.lens.clone(),
                focal_length: m.focal_length.clone(),
                aperture: m.aperture.clone(),
                shutter_speed: m.shutter_speed.clone(),
                iso: m.iso,
                capture_timestamp: m.capture_timestamp.clone(),
                raw: photo_format(&a.original_path)
                    .is_some_and(|f| f.family == FormatFamily::CameraRaw),
                color_representation: "linear-sRGB-D65; color metrics use display-sRGB".into(),
                decoder: self.engine.backend_id().into(),
            };
            measure::measure(&i.image, source, i.warnings.clone(), cancel)?
        };
        let mut warnings = common.warnings.clone();
        let mut providers = Vec::new();
        let subjects = if r.photo_type == PhotoType::Portrait {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> ProcessingResult<SubjectAnalysis> {
                    let i = input
                        .as_ref()
                        .ok_or_else(|| internal("Missing portrait input"))?;
                    let existing = self.engine.analysis_cached_mask(&a.original_path)?;
                    let mut failed_reason = None;
                    let mask = if existing.is_some() {
                        existing
                    } else if let Some(cache) = &self.masks {
                        providers.push(ProviderIdentity {
                            provider: "SegmentationProvider".into(),
                            model: "portrait-alpha".into(),
                            version: cache.provider_version().into(),
                        });
                        let diag = cache.generate(
                            &identity,
                            self.engine.backend_id(),
                            &i.image,
                            cancel,
                        )?;
                        if diag.status == photo_contracts::MaskStatus::Ready {
                            let (m, d) = cache.load(&identity, self.engine.backend_id());
                            m.map(|m| (m, d))
                        } else {
                            if diag.status == photo_contracts::MaskStatus::Failed {
                                failed_reason = Some(diag.warnings.join("; "));
                            }
                            warnings.extend(diag.warnings);
                            None
                        }
                    } else {
                        None
                    };
                    if let Some((mask, diag)) = mask {
                        let provider = ProviderIdentity {
                            provider: "SegmentationProvider".into(),
                            model: "portrait-alpha".into(),
                            version: diag.model_version.unwrap_or_else(|| "unknown".into()),
                        };
                        if !providers.contains(&provider) {
                            providers.push(provider);
                        }
                        warnings.extend(diag.warnings);
                        Ok(measure::subject(
                            &i.image,
                            &mask,
                            diag.reference.unwrap_or_else(|| "unavailable".into()),
                            cancel,
                        )?)
                    } else {
                        warnings.push(
                    "Subject segmentation unavailable or failed; common measurements remain valid"
                        .into(),
                );
                        let mut s = unavailable_subjects(false);
                        if let Some(reason) = failed_reason {
                            s.measurements = Observation::Failed { reason };
                        }
                        Ok(s)
                    }
                },
            ))
            .unwrap_or_else(|_| Err(internal("Subject analyzer stopped unexpectedly")));
            match result {
                Ok(s) => s,
                Err(e) => {
                    cancel.check()?;
                    warnings.push(e.message.clone());
                    let mut s = unavailable_subjects(false);
                    s.measurements = Observation::Failed { reason: e.message };
                    s
                }
            }
        } else {
            unavailable_subjects(true)
        };
        let lighting = lighting(&common, &subjects);
        let type_specific = match r.photo_type {
            PhotoType::Portrait => TypeAnalysis::Portrait(PortraitAnalysis {
                backlighting: lighting.backlighting_tendency.clone(),
                face_provider: "unavailable; no face model installed".into(),
            }),
            PhotoType::RealEstate => TypeAnalysis::RealEstate(RealEstateAnalysis {
                interior_exterior: Observation::unavailable(
                    "No semantic interior/exterior model configured",
                ),
                bright_region_fraction: common.exposure.near_highlight_clip_fraction,
                shadow_depth: common.exposure.percentiles.p05,
                mixed_lighting: lighting.mixed_lighting_tendency.clone(),
                estimated_roll: common.composition.horizontal_line.clone(),
            }),
            PhotoType::Landscape => TypeAnalysis::Landscape(LandscapeAnalysis {
                sky_fraction: Observation::unavailable("No sky segmentation model configured"),
                foreground_fraction: Observation::unavailable(
                    "No semantic foreground provider configured",
                ),
                low_contrast_tendency: common.dynamic_range.low_contrast_tendency.clone(),
                horizon: common.composition.horizontal_line.clone(),
            }),
        };
        let subject_status = match &subjects.measurements {
            Observation::Available { .. } => "complete",
            Observation::Failed { .. } => "failed",
            Observation::NotApplicable { .. } => "not_applicable",
            Observation::Unavailable { .. } => "unavailable",
        };
        let diagnostics = AnalysisDiagnostics {
            engine_version: ANALYSIS_ENGINE_VERSION.into(),
            providers,
            analyzers: vec![
                AnalyzerDiagnostic {
                    analyzer: "exposure/color/detail/straight-lines".into(),
                    status: if reused { "reused" } else { "complete" }.into(),
                    message: "Source proxy; straight lines are candidate level references, not semantic horizons".into(),
                },
                AnalyzerDiagnostic {
                    analyzer: "portrait-alpha".into(),
                    status: subject_status.into(),
                    message: "No instance count or calibrated confidence from alpha".into(),
                },
                AnalyzerDiagnostic {
                    analyzer: "faces/sky/semantic-scene".into(),
                    status: "unavailable".into(),
                    message: "No additional models introduced".into(),
                },
            ],
            duration_ms: started.elapsed().as_millis() as u64,
            common_cache_reused: reused,
            warnings,
        };
        let analysis = PhotoAnalysis {
            schema_version: PHOTO_ANALYSIS_SCHEMA_VERSION,
            analysis_id: uuid::Uuid::new_v4().to_string(),
            asset_id: r.asset_id.clone(),
            source_fingerprint: source.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            photo_type: r.photo_type,
            common,
            subjects,
            lighting,
            type_specific,
            confidence: Observation::unavailable(
                "No meaningful aggregate confidence; inspect individual observations",
            ),
            diagnostics,
        };
        cancel.check()?;
        let after = self
            .repository
            .asset(&r.job_id, &r.asset_id)
            .map_err(internal)?;
        if self.identity(&after)?.1 != source {
            return Err(ProcessingError::new(
                ProcessingErrorCode::SourceChanged,
                "Source or metadata changed during analysis",
            ));
        }
        let status = completed_status(&analysis);
        self.repository.persist_analysis(
            &r.job_id,
            &analysis,
            &key,
            &common_key,
            status,
            cancel,
        )?;
        Ok(AnalysisState {
            status,
            analysis: Some(analysis),
            cached: false,
            error: None,
        })
    }
}
fn completed_status(a: &PhotoAnalysis) -> AnalysisStatus {
    if a.diagnostics.warnings.is_empty() {
        AnalysisStatus::Complete
    } else {
        AnalysisStatus::Warning
    }
}
fn unavailable_subjects(skip: bool) -> SubjectAnalysis {
    fn absent<T>(skip: bool) -> Observation<T> {
        if skip {
            Observation::NotApplicable {
                reason: "Portrait alpha skipped for this photo type".into(),
            }
        } else {
            Observation::unavailable("No usable portrait alpha")
        }
    }
    SubjectAnalysis {
        subject_present: absent(skip),
        measurements: absent(skip),
        subject_count: absent(skip),
        faces: if skip {
            Observation::NotApplicable {
                reason: "Portrait face analysis skipped".into(),
            }
        } else {
            Observation::unavailable("No face detector installed")
        },
    }
}
fn lighting(c: &CommonAnalysis, s: &SubjectAnalysis) -> LightingAnalysis {
    let (subject, background, ev, backlit) = if let Some(m) = s.measurements.value() {
        (
            Observation::measured(m.subject.mean_luminance),
            Observation::measured(m.background.mean_luminance),
            Observation::measured(m.subject_background_ev_difference),
            Observation::inferred(
                (-m.subject_background_ev_difference / 3.).clamp(0., 1.),
                0.4,
            ),
        )
    } else {
        (
            Observation::unavailable("Subject unavailable"),
            Observation::unavailable("Subject unavailable"),
            Observation::unavailable("Subject unavailable"),
            Observation::unavailable("Subject unavailable"),
        )
    };
    LightingAnalysis {
        overall_light_level: c.exposure.mean_luminance,
        subject_light_level: subject,
        background_light_level: background,
        subject_background_ev_difference: ev,
        backlighting_tendency: backlit,
        mixed_lighting_tendency: Observation::inferred(
            (c.color.spatial_cast_variation / 0.25).clamp(0., 1.),
            0.25,
        ),
    }
}
