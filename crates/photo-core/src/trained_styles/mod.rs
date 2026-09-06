//! Adaptive creative-control inference over Phase 4 analysis and Phase 6 source context.
pub mod features;
pub mod package;
pub mod resolver;
mod storage;

use crate::{
    analysis::{AnalysisService, AnalysisStatus},
    batch_context::{BatchContextRequest, BatchContextService},
    rendering::internal,
    repository::JobRepository,
};
use photo_contracts::{
    analysis::{PhotoAnalysis, PhotoType, PHOTO_ANALYSIS_SCHEMA_VERSION},
    batch_context::{AssetBatchContext, BatchContext, ContextAvailability},
    trained_style::{
        LoadedStylePackage, StyleConfidence, StyleError, StyleFeatureVector, StylePrediction,
        StyleResolver,
    },
    CancellationToken, EditRecipe, ProcessingError, ProcessingErrorCode, ProcessingResult,
    RecipeGlobal, RecipeOrigin, RecipeProvenance,
};
use resolver::LINEAR_RESOLVER_VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

pub const TRAINED_STYLE_SOURCE: &str = "photo-editor/trained-style";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleSummary {
    pub style_id: String,
    pub name: String,
    pub version: String,
    pub model_version: String,
    pub package_identity: String,
    pub photo_type: PhotoType,
    pub description: String,
    pub development_only: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleFeatureSummary {
    pub median_luminance: f32,
    pub batch_exposure_delta_ev: Option<f32>,
    pub warm_cool_balance: f32,
    pub batch_warm_cool_delta: Option<f32>,
    pub group_confidence: f32,
    pub missing_feature_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleAssetInference {
    pub job_id: String,
    pub asset_id: String,
    pub style_id: String,
    pub style_version: String,
    pub model_version: String,
    pub package_identity: String,
    pub feature_schema: String,
    pub input_identity: Option<String>,
    pub analysis_id: Option<String>,
    pub batch_context_id: Option<String>,
    pub status: String,
    pub prediction: Option<StylePrediction>,
    pub feature_summary: Option<StyleFeatureSummary>,
    pub recipe_hash: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleApplyRequest {
    pub job_id: String,
    pub photo_type: PhotoType,
    pub style_id: String,
    pub selected_asset_ids: Vec<String>,
    pub request_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleApplyProgress {
    pub job_id: String,
    pub request_id: String,
    pub photo_type: PhotoType,
    pub style_id: String,
    pub status: String,
    pub stage: String,
    pub completed: u32,
    pub total: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleApplyResult {
    pub style: StyleSummary,
    pub selected_asset_ids: Vec<String>,
    pub predictions_attempted: u32,
    pub predictions_succeeded: u32,
    pub predictions_failed: u32,
    pub recipes_updated: u32,
    pub recipes_unchanged: u32,
    pub needs_review: Vec<String>,
    pub inferences: Vec<StyleAssetInference>,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleEditingState {
    pub styles: Vec<StyleSummary>,
    pub selected_asset_ids: Vec<String>,
    pub applied_style: Option<StyleSummary>,
    pub applied_count: u32,
    pub stale_asset_ids: Vec<String>,
    pub needs_review: Vec<String>,
    pub inferences: Vec<StyleAssetInference>,
    pub progress: Option<StyleApplyProgress>,
}

struct Active {
    request: StyleApplyRequest,
    token: CancellationToken,
    context_request_id: Option<String>,
}
type ActiveSlot = Arc<Mutex<Option<Active>>>;

pub struct StyleApplyPermit {
    request: StyleApplyRequest,
    token: CancellationToken,
    active: ActiveSlot,
    repository: JobRepository,
}

impl Drop for StyleApplyPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .is_some_and(|entry| entry.request.request_id == self.request.request_id)
            {
                if let Ok(Some(mut progress)) = self
                    .repository
                    .style_progress(&self.request.job_id, self.request.photo_type)
                {
                    if matches!(progress.status.as_str(), "queued" | "running") {
                        progress.status = "cancelled".into();
                        progress.stage = "Stopped; completed recipes remain available".into();
                        let _ = self.repository.save_style_progress(&progress);
                    }
                }
                *active = None;
            }
        }
    }
}

pub struct TrainedStyleService {
    repository: JobRepository,
    analysis: Arc<AnalysisService>,
    batch_context: Arc<BatchContextService>,
    catalog: Mutex<package::LocalStyleCatalog>,
    resolver: Arc<dyn StyleResolver>,
    active: ActiveSlot,
}

fn processing(error: StyleError) -> ProcessingError {
    ProcessingError::new(ProcessingErrorCode::InvalidAdjustments, error.to_string())
}

fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

fn summary(package: &LoadedStylePackage) -> StyleSummary {
    StyleSummary {
        style_id: package.manifest.style_id.clone(),
        name: package.manifest.name.clone(),
        version: package.manifest.version.clone(),
        model_version: package.manifest.model_version.clone(),
        package_identity: package.integrity.package_identity.clone(),
        photo_type: package.manifest.photo_type,
        description: package.metadata.description.clone(),
        development_only: package.metadata.development_only,
    }
}

fn input_identity(
    analysis: &PhotoAnalysis,
    context: &BatchContext,
    package: &LoadedStylePackage,
    resolver: &str,
) -> String {
    digest(&[
        "trained-style-input-v1",
        &analysis.source_fingerprint,
        &analysis.analysis_id,
        &context.batch_id,
        &package.integrity.package_identity,
        &package.manifest.feature_schema,
        resolver,
    ])
}

fn feature_summary(
    analysis: &PhotoAnalysis,
    context: &AssetBatchContext,
    features: &StyleFeatureVector,
) -> StyleFeatureSummary {
    StyleFeatureSummary {
        median_luminance: analysis.common.exposure.median_luminance as f32,
        batch_exposure_delta_ev: context
            .exposure_delta_from_group
            .as_ref()
            .map(|value| value.delta_ev as f32),
        warm_cool_balance: analysis.common.color.warm_cool_balance as f32,
        batch_warm_cool_delta: context
            .wb_delta_from_group
            .as_ref()
            .map(|value| value.warm_cool_delta as f32),
        group_confidence: context.group_confidence as f32,
        missing_feature_count: features.missing_features.len() as u32,
    }
}

pub fn resolve_prediction_to_recipe(
    current: &EditRecipe,
    package: &LoadedStylePackage,
    analysis: &PhotoAnalysis,
    context: &BatchContext,
    asset_context: &AssetBatchContext,
    prediction: &StylePrediction,
) -> ProcessingResult<EditRecipe> {
    prediction.validate().map_err(processing)?;
    if prediction.style_id != package.manifest.style_id
        || prediction.style_version != package.manifest.version
        || prediction.model_version != package.manifest.model_version
        || prediction.package_identity != package.integrity.package_identity
        || analysis.asset_id != current.asset_id
        || asset_context.asset_id != current.asset_id
    {
        return Err(internal("Style prediction identity mismatch"));
    }
    let current = current.validated()?;
    let optics = current.global.optics;
    let geometry = current.global.geometry.clone();
    let mut recipe = current;
    recipe.global = RecipeGlobal {
        optics,
        geometry,
        ..Default::default()
    };
    recipe.local_layers.clear();
    let predicted = prediction.adjustments;
    recipe.global.basic.exposure_ev = predicted.exposure_ev;
    recipe.global.basic.temperature = 6500.0 + predicted.temperature_delta;
    recipe.global.basic.tint = predicted.tint;
    recipe.global.basic.contrast = predicted.contrast;
    recipe.global.basic.highlights = predicted.highlights;
    recipe.global.basic.shadows = predicted.shadows;
    recipe.global.basic.whites = predicted.whites;
    recipe.global.basic.blacks = predicted.blacks;
    recipe.global.basic.saturation = predicted.saturation;
    recipe.global.basic.vibrance = predicted.vibrance;
    recipe.global.presence.texture = predicted.texture;
    recipe.global.presence.clarity = predicted.clarity;
    recipe.global.presence.dehaze = predicted.dehaze;
    recipe.global.detail.sharpening.amount = predicted.sharpening_amount;
    recipe.global.detail.noise.luminance = predicted.noise_reduction;
    recipe.global.effects.vignette.amount = predicted.vignette_amount;
    recipe.metadata.scene_cluster_id = asset_context.scene_group_id.clone();
    recipe.metadata.sequence_id = asset_context.sequence_group_id.clone();
    recipe.metadata.reference_asset_id = asset_context.reference_asset_id.clone();
    recipe.metadata.consistency_group_id = asset_context.lighting_group_id.clone();
    recipe.metadata.consistency_note = asset_context
        .consistency_notes
        .first()
        .map(|note| note.message.clone());
    recipe.metadata.confidence = Some(prediction.confidence_score);
    recipe.metadata.needs_review =
        Some(prediction.confidence == StyleConfidence::InsufficientEvidence);
    recipe.provenance = RecipeProvenance {
        origin: RecipeOrigin::TrainedStyle,
        created_by: Some(TRAINED_STYLE_SOURCE.into()),
        source_recipe_id: None,
        style_id: Some(package.manifest.style_id.clone()),
        model_id: Some(format!(
            "{}:{}",
            package.manifest.model.format, package.integrity.package_identity
        )),
        model_version: Some(package.manifest.model_version.clone()),
        analysis_id: Some(analysis.analysis_id.clone()),
        style_version: Some(package.manifest.version.clone()),
        style_package_id: Some(package.integrity.package_identity.clone()),
        feature_schema_version: Some(package.manifest.feature_schema.clone()),
        batch_context_id: Some(context.batch_id.clone()),
        batch_context_version: Some(context.grouping_version.clone()),
        photo_analysis_version: Some(format!(
            "schema:{};engine:{}",
            PHOTO_ANALYSIS_SCHEMA_VERSION, analysis.diagnostics.engine_version
        )),
        manually_modified: false,
        acceptance: None,
    };
    recipe.validated().map_err(Into::into)
}

impl TrainedStyleService {
    pub fn new(
        repository: JobRepository,
        analysis: Arc<AnalysisService>,
        batch_context: Arc<BatchContextService>,
        style_root: &Path,
    ) -> ProcessingResult<Self> {
        let catalog = package::LocalStyleCatalog::load(style_root).map_err(processing)?;
        Ok(Self::with_resolver(
            repository,
            analysis,
            batch_context,
            catalog,
            Arc::new(resolver::LinearStyleResolver),
        ))
    }

    pub fn with_resolver(
        repository: JobRepository,
        analysis: Arc<AnalysisService>,
        batch_context: Arc<BatchContextService>,
        catalog: package::LocalStyleCatalog,
        resolver: Arc<dyn StyleResolver>,
    ) -> Self {
        Self {
            repository,
            analysis,
            batch_context,
            catalog: Mutex::new(catalog),
            resolver,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub fn styles(&self, photo_type: PhotoType) -> Vec<StyleSummary> {
        self.catalog
            .lock()
            .ok()
            .map(|catalog| {
                catalog
                    .packages()
                    .filter(|package| package.manifest.photo_type == photo_type)
                    .map(summary)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn load_additional_style_root(&self, root: &Path) -> ProcessingResult<()> {
        self.catalog
            .lock()
            .map_err(internal)?
            .load_additional_root(root)
            .map_err(processing)
    }

    pub fn install_style_package(&self, directory: &Path) -> ProcessingResult<StyleSummary> {
        let package = package::load_style_package(directory).map_err(processing)?;
        let result = summary(&package);
        self.catalog
            .lock()
            .map_err(internal)?
            .insert_package(package)
            .map_err(processing)?;
        Ok(result)
    }

    fn package(&self, style_id: &str) -> ProcessingResult<Option<LoadedStylePackage>> {
        Ok(self
            .catalog
            .lock()
            .map_err(internal)?
            .get(style_id)
            .cloned())
    }

    pub fn reserve(&self, request: StyleApplyRequest) -> ProcessingResult<StyleApplyPermit> {
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err(internal("Invalid trained-style request ID"));
        }
        let package = self
            .package(&request.style_id)?
            .ok_or_else(|| internal("The selected AI style package is unavailable"))?;
        if package.manifest.photo_type != request.photo_type {
            return Err(internal(
                "The selected AI style does not support this photo type",
            ));
        }
        let persisted = self
            .repository
            .selected_editing_asset_ids(&request.job_id)?;
        let requested = request
            .selected_asset_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let persisted_set = persisted.iter().map(String::as_str).collect::<HashSet<_>>();
        if request.selected_asset_ids.is_empty() {
            return Err(internal(
                "Select at least one photograph before applying an AI style",
            ));
        }
        if requested.len() != request.selected_asset_ids.len() || requested != persisted_set {
            return Err(internal(
                "Editing selection changed. Return to culling or reload editing before applying a style",
            ));
        }
        let mut active = self.active.lock().map_err(internal)?;
        if active.is_some() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "One trained-style task is already active",
            ));
        }
        let token = CancellationToken::default();
        let progress = StyleApplyProgress {
            job_id: request.job_id.clone(),
            request_id: request.request_id.clone(),
            photo_type: request.photo_type,
            style_id: request.style_id.clone(),
            status: "queued".into(),
            stage: "Queued".into(),
            completed: 0,
            total: request.selected_asset_ids.len() as u32,
            succeeded: 0,
            failed: 0,
            duration_ms: 0,
            error: None,
        };
        self.repository.save_style_progress(&progress)?;
        *active = Some(Active {
            request: request.clone(),
            token: token.clone(),
            context_request_id: None,
        });
        Ok(StyleApplyPermit {
            request,
            token,
            active: self.active.clone(),
            repository: self.repository.clone(),
        })
    }

    pub fn cancel(&self, request_id: &str) -> ProcessingResult<()> {
        let context_request = {
            let active = self.active.lock().map_err(internal)?;
            active.as_ref().and_then(|entry| {
                if entry.request.request_id == request_id {
                    entry.token.cancel();
                    entry.context_request_id.clone()
                } else {
                    None
                }
            })
        };
        if let Some(context_request) = context_request {
            self.batch_context.cancel(&context_request)?;
        }
        Ok(())
    }

    pub fn progress(
        &self,
        job: &str,
        photo_type: PhotoType,
    ) -> ProcessingResult<Option<StyleApplyProgress>> {
        self.repository.style_progress(job, photo_type)
    }

    fn current_context(
        &self,
        request: &StyleApplyRequest,
        token: &CancellationToken,
        progress: &mut StyleApplyProgress,
    ) -> ProcessingResult<BatchContext> {
        token.check()?;
        if let Some(context) = self
            .batch_context
            .state(&request.job_id, request.photo_type)?
            .context
        {
            return Ok(context);
        }
        progress.stage = "Building current batch context".into();
        self.repository.save_style_progress(progress)?;
        let context_request_id = format!("style-context-{}", uuid::Uuid::new_v4());
        {
            let mut active = self.active.lock().map_err(internal)?;
            if let Some(entry) = active.as_mut() {
                entry.context_request_id = Some(context_request_id.clone());
            }
        }
        let result =
            self.batch_context
                .run(self.batch_context.reserve(BatchContextRequest {
                    job_id: request.job_id.clone(),
                    photo_type: request.photo_type,
                    request_id: context_request_id,
                    force: false,
                })?)?;
        token.check()?;
        result
            .context
            .ok_or_else(|| internal("The current editing selection has no usable batch context"))
    }

    pub fn apply(&self, permit: StyleApplyPermit) -> ProcessingResult<StyleApplyResult> {
        if !Arc::ptr_eq(&self.active, &permit.active) {
            return Err(internal("Trained-style permit belongs to another service"));
        }
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.apply_inner(&permit)))
                .unwrap_or_else(|_| Err(internal("Trained-style worker stopped unexpectedly")));
        if let Err(error) = &result {
            if let Ok(Some(mut progress)) =
                self.progress(&permit.request.job_id, permit.request.photo_type)
            {
                progress.status = if error.code == ProcessingErrorCode::Cancelled {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                progress.stage = if error.code == ProcessingErrorCode::Cancelled {
                    "Stopped; completed recipes remain available"
                } else {
                    "AI style failed; prior recipes remain available"
                }
                .into();
                progress.error = Some(error.message.clone());
                let _ = self.repository.save_style_progress(&progress);
            }
        }
        result
    }

    fn apply_inner(&self, permit: &StyleApplyPermit) -> ProcessingResult<StyleApplyResult> {
        let started = Instant::now();
        let request = &permit.request;
        let package = self
            .package(&request.style_id)?
            .ok_or_else(|| internal("The selected AI style package is unavailable"))?;
        let mut progress = StyleApplyProgress {
            job_id: request.job_id.clone(),
            request_id: request.request_id.clone(),
            photo_type: request.photo_type,
            style_id: request.style_id.clone(),
            status: "running".into(),
            stage: "Loading current source context".into(),
            completed: 0,
            total: request.selected_asset_ids.len() as u32,
            succeeded: 0,
            failed: 0,
            duration_ms: 0,
            error: None,
        };
        self.repository.save_style_progress(&progress)?;
        let context = self.current_context(request, &permit.token, &mut progress)?;
        if context.selected_asset_ids.iter().collect::<HashSet<_>>()
            != request.selected_asset_ids.iter().collect::<HashSet<_>>()
        {
            return Err(internal(
                "Batch context no longer matches the editing selection",
            ));
        }
        progress.stage = format!("Applying {}", package.manifest.name);
        self.repository.save_style_progress(&progress)?;
        let contexts = context
            .asset_contexts
            .iter()
            .map(|asset| (asset.asset_id.as_str(), asset))
            .collect::<HashMap<_, _>>();
        let mut inferences = Vec::with_capacity(request.selected_asset_ids.len());
        let mut updated = 0u32;
        let mut unchanged = 0u32;
        let mut needs_review = Vec::new();
        for asset_id in &request.selected_asset_ids {
            permit.token.check()?;
            let attempt = (|| -> ProcessingResult<StyleAssetInference> {
                let state =
                    self.analysis
                        .get_analysis(&request.job_id, asset_id, request.photo_type)?;
                let analysis = state.analysis.ok_or_else(|| {
                    internal(format!(
                        "Current PhotoAnalysis is unavailable ({:?})",
                        state.status
                    ))
                })?;
                if !matches!(
                    state.status,
                    AnalysisStatus::Complete | AnalysisStatus::Warning
                ) {
                    return Err(internal("Current PhotoAnalysis is incomplete"));
                }
                let asset_context = contexts
                    .get(asset_id.as_str())
                    .copied()
                    .ok_or_else(|| internal("Asset is missing from BatchContext"))?;
                if asset_context.availability == ContextAvailability::Unavailable {
                    return Err(internal("AssetBatchContext is unavailable"));
                }
                let features =
                    features::build_features(&analysis, asset_context, &context.batch_id)
                        .map_err(processing)?;
                let prediction = self
                    .resolver
                    .resolve(&package, &features)
                    .map_err(processing)?;
                let current = self.repository.get_recipe(&request.job_id, asset_id)?;
                if let Some(error) = current.error.clone() {
                    return Err(error.into());
                }
                let recipe = resolve_prediction_to_recipe(
                    &current.recipe,
                    &package,
                    &analysis,
                    &context,
                    asset_context,
                    &prediction,
                )?;
                let recipe = if recipe == current.recipe {
                    unchanged += 1;
                    current
                } else {
                    updated += 1;
                    self.repository.save_recipe(
                        &request.job_id,
                        asset_id,
                        &recipe,
                        current.generation,
                        Some(crate::recipes::RevisionReason::TrainedStyle),
                    )?
                };
                Ok(StyleAssetInference {
                    job_id: request.job_id.clone(),
                    asset_id: asset_id.clone(),
                    style_id: package.manifest.style_id.clone(),
                    style_version: package.manifest.version.clone(),
                    model_version: package.manifest.model_version.clone(),
                    package_identity: package.integrity.package_identity.clone(),
                    feature_schema: package.manifest.feature_schema.clone(),
                    input_identity: Some(input_identity(
                        &analysis,
                        &context,
                        &package,
                        self.resolver.backend_id(),
                    )),
                    analysis_id: Some(analysis.analysis_id.clone()),
                    batch_context_id: Some(context.batch_id.clone()),
                    status: "applied".into(),
                    feature_summary: Some(feature_summary(&analysis, asset_context, &features)),
                    prediction: Some(prediction),
                    recipe_hash: Some(recipe.recipe_hash),
                    error: None,
                    stale: false,
                })
            })();
            let inference = match attempt {
                Ok(inference) => {
                    progress.succeeded += 1;
                    inference
                }
                Err(error) if error.code == ProcessingErrorCode::Cancelled => return Err(error),
                Err(error) => {
                    progress.failed += 1;
                    needs_review.push(asset_id.clone());
                    StyleAssetInference {
                        job_id: request.job_id.clone(),
                        asset_id: asset_id.clone(),
                        style_id: package.manifest.style_id.clone(),
                        style_version: package.manifest.version.clone(),
                        model_version: package.manifest.model_version.clone(),
                        package_identity: package.integrity.package_identity.clone(),
                        feature_schema: package.manifest.feature_schema.clone(),
                        input_identity: None,
                        analysis_id: None,
                        batch_context_id: Some(context.batch_id.clone()),
                        status: "failed".into(),
                        prediction: None,
                        feature_summary: None,
                        recipe_hash: None,
                        error: Some(error.message),
                        stale: false,
                    }
                }
            };
            self.repository.save_style_inference(&inference)?;
            inferences.push(inference);
            progress.completed += 1;
            progress.duration_ms = started.elapsed().as_millis() as u64;
            self.repository.save_style_progress(&progress)?;
        }
        progress.status = "complete".into();
        progress.stage = "Style recipes ready".into();
        progress.duration_ms = started.elapsed().as_millis() as u64;
        self.repository.save_style_progress(&progress)?;
        Ok(StyleApplyResult {
            style: summary(&package),
            selected_asset_ids: request.selected_asset_ids.clone(),
            predictions_attempted: progress.completed,
            predictions_succeeded: progress.succeeded,
            predictions_failed: progress.failed,
            recipes_updated: updated,
            recipes_unchanged: unchanged,
            needs_review,
            inferences,
            duration_ms: progress.duration_ms,
        })
    }

    pub fn state(&self, job: &str, photo_type: PhotoType) -> ProcessingResult<StyleEditingState> {
        let selected = self.repository.selected_editing_asset_ids(job)?;
        let context = self.batch_context.state(job, photo_type)?.context;
        let mut inferences = self.repository.style_inferences(job, &selected)?;
        let mut stale = Vec::new();
        let mut needs_review = Vec::new();
        let mut applied = Vec::new();
        for inference in &mut inferences {
            if inference.status == "failed" {
                needs_review.push(inference.asset_id.clone());
                continue;
            }
            let current = self.repository.get_recipe(job, &inference.asset_id)?;
            let package = self.package(&inference.style_id)?;
            let analysis = self
                .analysis
                .get_analysis(job, &inference.asset_id, photo_type)?
                .analysis;
            let current_identity = match (&context, package.as_ref(), analysis) {
                (Some(context), Some(package), Some(analysis)) => Some(input_identity(
                    &analysis,
                    context,
                    package,
                    self.resolver.backend_id(),
                )),
                _ => None,
            };
            let provenance = &current.recipe.provenance;
            inference.stale = current_identity.as_deref() != inference.input_identity.as_deref()
                || provenance.origin != RecipeOrigin::TrainedStyle
                || provenance.style_id.as_deref() != Some(inference.style_id.as_str())
                || provenance.style_package_id.as_deref()
                    != Some(inference.package_identity.as_str())
                || inference.recipe_hash.as_deref() != Some(current.recipe_hash.as_str());
            if inference.stale {
                stale.push(inference.asset_id.clone());
            } else {
                applied.push(inference.style_id.clone());
            }
        }
        let applied_style = applied
            .first()
            .filter(|first| {
                applied.len() == selected.len() && applied.iter().all(|style| style == *first)
            })
            .and_then(|id| self.package(id).ok().flatten())
            .map(|package| summary(&package));
        Ok(StyleEditingState {
            styles: self.styles(photo_type),
            selected_asset_ids: selected,
            applied_style,
            applied_count: applied.len() as u32,
            stale_asset_ids: stale,
            needs_review,
            inferences,
            progress: self.progress(job, photo_type)?,
        })
    }
}

pub fn benchmark_predictions(
    resolver: &dyn StyleResolver,
    package: &LoadedStylePackage,
    features: &StyleFeatureVector,
    count: usize,
) -> Result<u128, StyleError> {
    let started = Instant::now();
    for _ in 0..count {
        std::hint::black_box(resolver.resolve(package, features)?);
    }
    Ok(started.elapsed().as_micros())
}

pub fn resolver_version() -> &'static str {
    LINEAR_RESOLVER_VERSION
}
