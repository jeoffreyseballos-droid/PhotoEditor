//! Local supervised Training Studio: before/after pairs become Phase 7 recipe-control packages.
pub mod matcher;
pub mod matching_task;
pub mod package;
mod storage;
pub mod target;
pub mod trainer;

use crate::{
    analysis::{AnalysisRequest, AnalysisService, AnalysisStatus},
    batch_context::{build_from_inputs, BatchAssetInput},
    discovery::inspect_file,
    models::{Asset, NewJob},
    rendering::{internal, CpuProcessingEngine, RENDERER_VERSION},
    repository::JobRepository,
    trained_styles::features::build_features,
};
use base64::Engine;
use matcher::AutoMatchResult;
use photo_contracts::{
    analysis::PhotoType,
    batch_context::{AssetBatchContext, ConsistencyNote, ConsistencyNoteCode, ContextAvailability},
    culling::CullingAssessment,
    formats::{photo_format, FileType, FormatFamily},
    trained_style::{PredictedCreativeAdjustments, STYLE_FEATURE_SCHEMA_V1},
    training::*,
    CancellationToken, EditRecipe, OutputFormat, ProcessingError, ProcessingErrorCode,
    ProcessingResult, RecipeOrigin, RenderAdjustments, RenderRequest, RECIPE_SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
use target::{StagedTargetOptimizer, TargetRecipeOptimizer};
use trainer::{
    assign_splits, predict_controls, RegularizedLinearTrainer, StyleModelTrainer, TrainingExample,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTrainingDatasetRequest {
    #[serde(default)]
    pub job_id: String,
    pub style_name: String,
    pub photo_type: PhotoType,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddTrainingPairRequest {
    pub dataset_id: String,
    pub source_asset_id: String,
    pub reference_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddTrainingFilesRequest {
    pub dataset_id: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddTrainingFolderRequest {
    pub dataset_id: String,
    pub folder: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddTrainingPathPairRequest {
    pub dataset_id: String,
    pub before_path: PathBuf,
    pub after_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingRequest {
    pub dataset_id: String,
    pub request_id: String,
    #[serde(default)]
    pub config: TrainingConfig,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoMatchApplyResult {
    pub dataset: TrainingDataset,
    pub matching: AutoMatchResult,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingPreviewSet {
    pub source_data: String,
    /// The currently applied Phase 7 trained-style render, when this pair's
    /// source asset has been used for validation after training.
    pub ai_data: Option<String>,
    pub target_data: Option<String>,
    pub reference_data: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationEditingSelection {
    pub photo_type: PhotoType,
    pub asset_ids: Vec<String>,
}

struct ActiveTraining {
    run_id: String,
    token: CancellationToken,
}

type ActiveSlot = Arc<Mutex<Option<ActiveTraining>>>;

pub struct TrainingPermit {
    request: TrainingRequest,
    token: CancellationToken,
    active: ActiveSlot,
    repository: JobRepository,
}

impl Drop for TrainingPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .is_some_and(|value| value.run_id == self.request.request_id)
            {
                if let Ok(Some(mut run)) = self.repository.training_run(&self.request.request_id) {
                    if matches!(
                        run.status,
                        TrainingRunStatus::Queued | TrainingRunStatus::Running
                    ) {
                        run.status = TrainingRunStatus::Cancelled;
                        run.stage = TrainingStage::Stopped;
                        run.updated_at = chrono::Utc::now().to_rfc3339();
                        run.error = Some(
                            "Training reservation ended; cached pair work remains available".into(),
                        );
                        let _ = self.repository.save_training_run(&run);
                    }
                }
                *active = None;
            }
        }
    }
}

pub struct TrainingService {
    repository: JobRepository,
    analysis: Arc<AnalysisService>,
    engine: Arc<CpuProcessingEngine>,
    optimizer: Arc<dyn TargetRecipeOptimizer>,
    trainer: Arc<dyn StyleModelTrainer>,
    style_root: PathBuf,
    cache_root: PathBuf,
    active: ActiveSlot,
    matching: Mutex<matching_task::MatchingSlot>,
}

impl TrainingService {
    pub fn new(
        repository: JobRepository,
        analysis: Arc<AnalysisService>,
        engine: Arc<CpuProcessingEngine>,
        style_root: PathBuf,
        cache_root: PathBuf,
    ) -> ProcessingResult<Self> {
        fs::create_dir_all(&style_root).map_err(crate::rendering::io_error)?;
        fs::create_dir_all(&cache_root).map_err(crate::rendering::io_error)?;
        Ok(Self {
            repository,
            analysis,
            optimizer: Arc::new(StagedTargetOptimizer::new(engine.clone())),
            trainer: Arc::new(RegularizedLinearTrainer),
            engine,
            style_root,
            cache_root,
            active: Arc::new(Mutex::new(None)),
            matching: Mutex::new(Default::default()),
        })
    }

    pub fn with_components(
        repository: JobRepository,
        analysis: Arc<AnalysisService>,
        engine: Arc<CpuProcessingEngine>,
        optimizer: Arc<dyn TargetRecipeOptimizer>,
        trainer: Arc<dyn StyleModelTrainer>,
        style_root: PathBuf,
        cache_root: PathBuf,
    ) -> ProcessingResult<Self> {
        fs::create_dir_all(&style_root).map_err(crate::rendering::io_error)?;
        fs::create_dir_all(&cache_root).map_err(crate::rendering::io_error)?;
        Ok(Self {
            repository,
            analysis,
            engine,
            optimizer,
            trainer,
            style_root,
            cache_root,
            active: Arc::new(Mutex::new(None)),
            matching: Mutex::new(Default::default()),
        })
    }

    pub fn create_dataset(
        &self,
        request: CreateTrainingDatasetRequest,
    ) -> ProcessingResult<TrainingDataset> {
        let name = request.style_name.trim();
        if name.is_empty() || name.len() > 256 {
            return Err(internal("Style name must contain 1 through 256 characters"));
        }
        let dataset_id = uuid::Uuid::new_v4().to_string();
        let job_id = if request.job_id.trim().is_empty() {
            self.create_standalone_job(&dataset_id)?
        } else {
            self.repository.get_job(&request.job_id).map_err(internal)?;
            request.job_id
        };
        let now = chrono::Utc::now().to_rfc3339();
        let dataset = TrainingDataset {
            schema_version: TRAINING_DATASET_SCHEMA_VERSION,
            dataset_id,
            job_id,
            style_name: name.into(),
            photo_type: request.photo_type,
            pairs: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            dataset_fingerprint: None,
            feature_schema: STYLE_FEATURE_SCHEMA_V1.into(),
            renderer_version: RENDERER_VERSION.into(),
            target_recipe_schema: RECIPE_SCHEMA_VERSION,
            batch_context_id: None,
            warnings: vec![
                "Fewer than 10 pairs is an experimental dataset; more varied examples are recommended"
                    .into(),
            ],
            before_files: Vec::new(),
            after_files: Vec::new(),
            alignment: None,
        };
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn datasets(&self, job_id: &str) -> ProcessingResult<Vec<TrainingDataset>> {
        self.repository.get_job(job_id).map_err(internal)?;
        self.repository.training_datasets(job_id)
    }

    pub fn all_datasets(&self) -> ProcessingResult<Vec<TrainingDataset>> {
        self.repository.training_datasets_all()
    }

    pub fn dataset(&self, dataset_id: &str) -> ProcessingResult<TrainingDataset> {
        self.repository.training_dataset(dataset_id)
    }

    pub fn add_pair(&self, request: AddTrainingPairRequest) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(&request.dataset_id)?;
        let source = self
            .repository
            .asset(&dataset.job_id, &request.source_asset_id)
            .map_err(internal)?;
        if !source_supported(&source.original_path) {
            return Err(internal(
                "This source format is not developable for training",
            ));
        }
        let reference = request
            .reference_path
            .canonicalize()
            .map_err(crate::rendering::io_error)?;
        if !reference_supported(&reference) {
            return Err(internal("Training references must be JPEG, TIFF, or PNG"));
        }
        if dataset
            .pairs
            .iter()
            .any(|pair| pair.source_asset_id == source.id)
        {
            return Err(internal(
                "This source already has a reference in the current dataset",
            ));
        }
        let source_path = source
            .original_path
            .canonicalize()
            .map_err(crate::rendering::io_error)?;
        if source_path == reference {
            return Err(internal("Source and reference must be different files"));
        }
        let source_fingerprint = file_fingerprint(&source_path, &CancellationToken::default())?;
        let reference_fingerprint = file_fingerprint(&reference, &CancellationToken::default())?;
        let pair_id = digest(&[
            "training-pair-v1",
            &dataset.dataset_id,
            &source.id,
            &reference_fingerprint,
        ]);
        dataset.pairs.push(TrainingPair {
            schema_version: TRAINING_PAIR_SCHEMA_VERSION,
            pair_id,
            dataset_id: dataset.dataset_id.clone(),
            source_job_id: dataset.job_id.clone(),
            source_asset_id: source.id,
            source_path,
            reference_path: reference,
            photo_type: dataset.photo_type,
            source_fingerprint,
            reference_fingerprint,
            validation: PairValidation::default(),
            source_analysis_id: None,
            batch_context: None,
            scene_group_id: None,
            target: None,
            split: TrainingSplit::Unassigned,
            excluded: false,
            feedback: None,
            diagnostics: Vec::new(),
        });
        invalidate_dataset(&mut dataset);
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn add_before_files(
        &self,
        request: AddTrainingFilesRequest,
    ) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(&request.dataset_id)?;
        for path in request.paths {
            let path = path.canonicalize().map_err(crate::rendering::io_error)?;
            if !source_supported(&path) {
                return Err(internal(format!(
                    "{} is not a supported before/source image",
                    path.display()
                )));
            }
            self.ensure_training_asset(&dataset, &path)?;
            if !dataset
                .before_files
                .iter()
                .any(|existing| existing == &path)
            {
                dataset.before_files.push(path);
            }
        }
        dataset
            .before_files
            .sort_by(|a, b| matcher::natural_cmp(a, b));
        dataset.alignment = None;
        invalidate_dataset(&mut dataset);
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn add_after_files(
        &self,
        request: AddTrainingFilesRequest,
    ) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(&request.dataset_id)?;
        for path in request.paths {
            let path = path.canonicalize().map_err(crate::rendering::io_error)?;
            if !reference_supported(&path) {
                return Err(internal(format!(
                    "{} is not a supported after/reference image",
                    path.display()
                )));
            }
            if !dataset.after_files.iter().any(|existing| existing == &path) {
                dataset.after_files.push(path);
            }
        }
        dataset
            .after_files
            .sort_by(|a, b| matcher::natural_cmp(a, b));
        dataset.alignment = None;
        invalidate_dataset(&mut dataset);
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn add_before_folder(
        &self,
        request: AddTrainingFolderRequest,
    ) -> ProcessingResult<TrainingDataset> {
        let paths =
            discover_training_paths(&request.folder, true, &[&self.cache_root, &self.style_root])?;
        self.add_before_files(AddTrainingFilesRequest {
            dataset_id: request.dataset_id,
            paths,
        })
    }

    pub fn add_after_folder(
        &self,
        request: AddTrainingFolderRequest,
    ) -> ProcessingResult<TrainingDataset> {
        let paths = discover_training_paths(
            &request.folder,
            false,
            &[&self.cache_root, &self.style_root],
        )?;
        self.add_after_files(AddTrainingFilesRequest {
            dataset_id: request.dataset_id,
            paths,
        })
    }

    pub fn add_path_pair(
        &self,
        request: AddTrainingPathPairRequest,
    ) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(&request.dataset_id)?;
        let before = request
            .before_path
            .canonicalize()
            .map_err(crate::rendering::io_error)?;
        let after = request
            .after_path
            .canonicalize()
            .map_err(crate::rendering::io_error)?;
        if !source_supported(&before) || !reference_supported(&after) {
            return Err(internal(
                "The selected files are not supported training inputs",
            ));
        }
        if before == after {
            return Err(internal("Before and after must be different files"));
        }
        if dataset
            .pairs
            .iter()
            .any(|pair| pair.source_path == before || pair.reference_path == after)
        {
            return Err(internal(
                "One of these files is already paired in this dataset",
            ));
        }
        let source = self.ensure_training_asset(&dataset, &before)?;
        dataset.pairs.push(self.pair_from_paths(
            &dataset,
            source.id,
            before.clone(),
            after.clone(),
        )?);
        dataset
            .pairs
            .last_mut()
            .expect("Just inserted pair")
            .diagnostics
            .push("Manual pairing".into());
        if !dataset.before_files.iter().any(|path| path == &before) {
            dataset.before_files.push(before);
        }
        if !dataset.after_files.iter().any(|path| path == &after) {
            dataset.after_files.push(after);
        }
        dataset.before_files.sort();
        dataset.after_files.sort();
        dataset.alignment = None;
        dataset.warnings.push(
            "A manual pair was added; run Match / Validate Dataset again to refresh alignment."
                .into(),
        );
        invalidate_dataset(&mut dataset);
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn match_dataset(&self, dataset_id: &str) -> ProcessingResult<AutoMatchApplyResult> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(dataset_id)?;
        if dataset.before_files.is_empty() || dataset.after_files.is_empty() {
            return Err(internal(
                "Add at least one before and one after image first",
            ));
        }
        let matching = matcher::match_paths(&dataset.before_files, &dataset.after_files);
        dataset.pairs.clear();
        for candidate in &matching.matched {
            let Some(source_path) = candidate.source_path.as_ref() else {
                continue;
            };
            let source = self.ensure_training_asset(&dataset, source_path)?;
            dataset.pairs.push(self.pair_from_paths(
                &dataset,
                source.id,
                source_path.clone(),
                candidate.reference_path.clone(),
            )?);
        }
        dataset.alignment = Some(TrainingAlignment {
            before_count: matching.before_count as u32,
            after_count: matching.after_count as u32,
            matched_count: matching.matched.len() as u32,
            ambiguous_count: matching.ambiguous_sources.len() as u32,
            unmatched_before: matching
                .unmatched_sources
                .iter()
                .map(PathBuf::from)
                .collect(),
            unmatched_after: matching.unmatched_references.clone(),
            first_before: dataset.before_files.first().cloned(),
            first_after: dataset.after_files.first().cloned(),
            last_before: dataset.before_files.last().cloned(),
            last_after: dataset.after_files.last().cloned(),
            start_aligned: matching.start_aligned,
            end_aligned: matching.end_aligned,
            order_fallback_used: matching.order_fallback_used,
            diagnostics: matching.diagnostics.clone(),
        });
        dataset
            .warnings
            .extend(matching.diagnostics.iter().cloned());
        invalidate_dataset(&mut dataset);
        self.repository.save_training_dataset(&dataset)?;
        Ok(AutoMatchApplyResult { dataset, matching })
    }

    pub fn auto_match_folder(
        &self,
        dataset_id: &str,
        folder: &Path,
    ) -> ProcessingResult<AutoMatchApplyResult> {
        let canonical = folder.canonicalize().map_err(crate::rendering::io_error)?;
        if !canonical.is_dir() {
            return Err(internal("Reference folder was not found"));
        }
        let dataset = self.dataset(dataset_id)?;
        let assets = self.all_assets(&dataset.job_id)?;
        let matching = matcher::auto_match(&assets, &canonical);
        let mut current = dataset;
        for candidate in &matching.matched {
            if current
                .pairs
                .iter()
                .any(|pair| pair.source_asset_id == candidate.source_asset_id)
            {
                continue;
            }
            current = self.add_pair(AddTrainingPairRequest {
                dataset_id: current.dataset_id.clone(),
                source_asset_id: candidate.source_asset_id.clone(),
                reference_path: candidate.reference_path.clone(),
            })?;
        }
        if !matching.ambiguous_sources.is_empty() {
            current.warnings.push(format!(
                "{} filename matches are ambiguous and require manual pairing",
                matching.ambiguous_sources.len()
            ));
            current.updated_at = chrono::Utc::now().to_rfc3339();
            self.repository.save_training_dataset(&current)?;
        }
        Ok(AutoMatchApplyResult {
            dataset: current,
            matching,
        })
    }

    pub fn set_pair_excluded(
        &self,
        dataset_id: &str,
        pair_id: &str,
        excluded: bool,
    ) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(dataset_id)?;
        let pair = dataset
            .pairs
            .iter_mut()
            .find(|pair| pair.pair_id == pair_id)
            .ok_or_else(|| internal("Training pair was not found"))?;
        pair.excluded = excluded;
        pair.split = if excluded {
            TrainingSplit::Excluded
        } else {
            TrainingSplit::Unassigned
        };
        invalidate_dataset(&mut dataset);
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn validate_dataset(&self, dataset_id: &str) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        self.validate_dataset_with_token(dataset_id, &CancellationToken::default(), None)
    }

    fn validate_dataset_with_token(
        &self,
        dataset_id: &str,
        token: &CancellationToken,
        mut run: Option<&mut TrainingRun>,
    ) -> ProcessingResult<TrainingDataset> {
        let mut dataset = self.dataset(dataset_id)?;
        if dataset.pairs.is_empty() {
            return Err(internal("Add at least one source/reference pair first"));
        }
        if let Some(run) = run.as_deref_mut() {
            update_run(
                &self.repository,
                run,
                TrainingStage::ValidatingPairs,
                0,
                dataset.pairs.len() as u32,
            )?;
        }
        let mut analyses = HashMap::new();
        let pair_total = dataset.pairs.len() as u32;
        for (index, pair) in dataset.pairs.iter_mut().enumerate() {
            token.check()?;
            pair.diagnostics.retain(|item| item == "Manual pairing");
            if !source_supported(&pair.source_path) || !reference_supported(&pair.reference_path) {
                pair.validation = PairValidation {
                    status: PairValidationStatus::Unusable,
                    geometry: GeometryRelationship::Unusable,
                    diagnostics: vec!["Unsupported source or reference format".into()],
                    ..Default::default()
                };
                continue;
            }
            let source_fingerprint = file_fingerprint(&pair.source_path, token);
            let reference_fingerprint = file_fingerprint(&pair.reference_path, token);
            let (source_fingerprint, reference_fingerprint) =
                match (source_fingerprint, reference_fingerprint) {
                    (Ok(source), Ok(reference)) => (source, reference),
                    (source, reference) => {
                        pair.validation = PairValidation {
                            status: PairValidationStatus::Unusable,
                            geometry: GeometryRelationship::Unusable,
                            diagnostics: vec![source
                                .err()
                                .or_else(|| reference.err())
                                .map(|error| error.message)
                                .unwrap_or_else(|| "Pair is unreadable".into())],
                            ..Default::default()
                        };
                        continue;
                    }
                };
            if source_fingerprint != pair.source_fingerprint
                || reference_fingerprint != pair.reference_fingerprint
            {
                pair.source_fingerprint = source_fingerprint;
                pair.reference_fingerprint = reference_fingerprint;
                pair.target = None;
                pair.split = TrainingSplit::Unassigned;
            }
            if let Some(run) = run.as_deref_mut() {
                update_run(
                    &self.repository,
                    run,
                    TrainingStage::Analyzing,
                    index as u32,
                    pair_total,
                )?;
            }
            let state = self.analysis.get_analysis(
                &dataset.job_id,
                &pair.source_asset_id,
                dataset.photo_type,
            )?;
            let state = if matches!(
                state.status,
                AnalysisStatus::Complete | AnalysisStatus::Warning
            ) && state.analysis.is_some()
            {
                state
            } else {
                let request = AnalysisRequest {
                    job_id: dataset.job_id.clone(),
                    asset_id: pair.source_asset_id.clone(),
                    photo_type: dataset.photo_type,
                    request_id: format!("training-analysis-{}", uuid::Uuid::new_v4()),
                };
                self.analysis
                    .analyze_asset(self.analysis.reserve(request)?)?
            };
            let Some(analysis) = state.analysis else {
                pair.validation = PairValidation {
                    status: PairValidationStatus::Unusable,
                    geometry: GeometryRelationship::Unusable,
                    diagnostics: vec!["Current PhotoAnalysis is unavailable".into()],
                    ..Default::default()
                };
                continue;
            };
            pair.source_analysis_id = Some(analysis.analysis_id.clone());
            analyses.insert(pair.source_asset_id.clone(), analysis);
            pair.validation = self.optimizer.validate_pair(pair, token)?;
            if let Some(run) = run.as_deref_mut() {
                run.completed = (index + 1) as u32;
                run.updated_at = chrono::Utc::now().to_rfc3339();
                self.repository.save_training_run(run)?;
            }
        }
        let mut inputs = Vec::new();
        for pair in &dataset.pairs {
            if matches!(
                pair.validation.status,
                PairValidationStatus::Rejected | PairValidationStatus::Unusable
            ) {
                continue;
            }
            let asset = self
                .repository
                .asset(&dataset.job_id, &pair.source_asset_id)
                .map_err(internal)?;
            let analysis = analyses.get(&pair.source_asset_id).cloned();
            let culling = current_culling(
                &self.repository,
                &dataset.job_id,
                &pair.source_asset_id,
                dataset.photo_type,
                analysis.as_ref(),
            );
            inputs.push(BatchAssetInput {
                asset_id: pair.source_asset_id.clone(),
                source_fingerprint: asset.fingerprint,
                analysis,
                culling,
                unavailable_reason: None,
            });
        }
        if !inputs.is_empty() {
            match build_from_inputs(&dataset.job_id, dataset.photo_type, &inputs, token) {
                Ok(context) => {
                    dataset.batch_context_id = Some(context.batch_id.clone());
                    let contexts = context
                        .asset_contexts
                        .into_iter()
                        .map(|context| (context.asset_id.clone(), context))
                        .collect::<HashMap<_, _>>();
                    for pair in &mut dataset.pairs {
                        pair.batch_context = contexts.get(&pair.source_asset_id).cloned();
                        pair.scene_group_id = pair
                            .batch_context
                            .as_ref()
                            .and_then(|context| context.scene_group_id.clone());
                    }
                }
                Err(error) => {
                    dataset.batch_context_id = None;
                    dataset.warnings.push(format!(
                        "Batch context unavailable; explicit missing context will be used: {}",
                        error.message
                    ));
                }
            }
        }
        dataset.dataset_fingerprint = Some(dataset_identity(&dataset));
        dataset.updated_at = chrono::Utc::now().to_rfc3339();
        dataset.warnings.retain(|warning| {
            !warning.contains("training pairs") && !warning.contains("experimental dataset")
        });
        dataset.warnings.push(size_guidance(dataset.pairs.len()));
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn reserve(&self, request: TrainingRequest) -> ProcessingResult<TrainingPermit> {
        let _guard = self.idle_matching()?;
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err(internal("Invalid training request ID"));
        }
        request.config.validate().map_err(internal)?;
        let dataset = self.dataset(&request.dataset_id)?;
        let mut active = self.active.lock().map_err(internal)?;
        if active.is_some() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "One Training Studio run is already active",
            ));
        }
        let token = CancellationToken::default();
        let now = chrono::Utc::now().to_rfc3339();
        self.repository.save_training_run(&TrainingRun {
            schema_version: TRAINING_RUN_SCHEMA_VERSION,
            run_id: request.request_id.clone(),
            dataset_id: dataset.dataset_id.clone(),
            style_id: None,
            style_name: dataset.style_name,
            style_version: None,
            status: TrainingRunStatus::Queued,
            stage: TrainingStage::Queued,
            completed: 0,
            total: dataset.pairs.len() as u32,
            config: request.config.clone(),
            metrics: None,
            artifact_path: None,
            started_at: now.clone(),
            updated_at: now,
            duration_ms: 0,
            error: None,
        })?;
        *active = Some(ActiveTraining {
            run_id: request.request_id.clone(),
            token: token.clone(),
        });
        Ok(TrainingPermit {
            request,
            token,
            active: self.active.clone(),
            repository: self.repository.clone(),
        })
    }

    pub fn cancel(&self, request_id: &str) -> ProcessingResult<()> {
        if let Some(active) = self.active.lock().map_err(internal)?.as_ref() {
            if active.run_id == request_id {
                active.token.cancel();
            }
        }
        Ok(())
    }

    pub fn progress(&self, dataset_id: &str) -> ProcessingResult<Option<TrainingRun>> {
        self.repository.latest_training_run(dataset_id)
    }

    pub fn train(&self, permit: TrainingPermit) -> ProcessingResult<TrainingRun> {
        if !Arc::ptr_eq(&self.active, &permit.active) {
            return Err(internal("Training permit belongs to another service"));
        }
        let started = Instant::now();
        let mut run = self
            .repository
            .training_run(&permit.request.request_id)?
            .ok_or_else(|| internal("Training run was not found"))?;
        run.status = TrainingRunStatus::Running;
        run.updated_at = chrono::Utc::now().to_rfc3339();
        self.repository.save_training_run(&run)?;
        let outcome = self.train_inner(&permit, &mut run, started);
        match outcome {
            Ok(()) => Ok(run),
            Err(error) => {
                run.status = if error.code == ProcessingErrorCode::Cancelled
                    || permit.token.is_cancelled()
                {
                    TrainingRunStatus::Cancelled
                } else {
                    TrainingRunStatus::Failed
                };
                run.stage = TrainingStage::Stopped;
                run.error = Some(error.message.clone());
                run.duration_ms = started.elapsed().as_millis() as u64;
                run.updated_at = chrono::Utc::now().to_rfc3339();
                let _ = self.repository.save_training_run(&run);
                Err(error)
            }
        }
    }

    fn train_inner(
        &self,
        permit: &TrainingPermit,
        run: &mut TrainingRun,
        started: Instant,
    ) -> ProcessingResult<()> {
        permit.token.check()?;
        let mut dataset =
            self.validate_dataset_with_token(&permit.request.dataset_id, &permit.token, Some(run))?;
        update_run(
            &self.repository,
            run,
            TrainingStage::EstimatingTargetRecipes,
            0,
            dataset.pairs.len() as u32,
        )?;
        for (index, pair) in dataset.pairs.iter_mut().enumerate() {
            permit.token.check()?;
            if matches!(
                pair.validation.status,
                PairValidationStatus::Rejected | PairValidationStatus::Unusable
            ) || pair.excluded
            {
                pair.split = TrainingSplit::Excluded;
                continue;
            }
            let identity = target::target_cache_identity(pair, self.engine.analysis_mask_version());
            let target = if let Some(cached) = self.repository.cached_target(&identity)? {
                if cached.cache_identity == identity
                    && cached.optimizer_version == self.optimizer.version()
                    && cached.schema_version == TARGET_RECIPE_SCHEMA_VERSION
                    && cached.loss.total.is_finite()
                    && cached.recipe.clone().validated().is_ok()
                {
                    cached
                } else {
                    let estimated = self.optimizer.estimate(pair, &permit.token)?;
                    self.repository.save_target(&pair.pair_id, &estimated)?;
                    estimated
                }
            } else {
                let estimated = self.optimizer.estimate(pair, &permit.token)?;
                self.repository.save_target(&pair.pair_id, &estimated)?;
                estimated
            };
            pair.target = Some(target);
            run.completed = (index + 1) as u32;
            run.duration_ms = started.elapsed().as_millis() as u64;
            run.updated_at = chrono::Utc::now().to_rfc3339();
            self.repository.save_training_run(run)?;
        }
        assign_splits(&mut dataset, &permit.request.config).map_err(internal)?;
        dataset.updated_at = chrono::Utc::now().to_rfc3339();
        self.repository.save_training_dataset(&dataset)?;
        update_run(
            &self.repository,
            run,
            TrainingStage::BuildingExamples,
            0,
            dataset.pairs.len() as u32,
        )?;
        let examples = self.examples(&dataset, &permit.token, run)?;
        let dataset_identity = dataset
            .dataset_fingerprint
            .as_deref()
            .ok_or_else(|| internal("Validated dataset has no stable identity"))?;
        let version =
            package::next_style_identity(&self.style_root, &dataset.style_name, dataset_identity);
        update_run(
            &self.repository,
            run,
            TrainingStage::Training,
            0,
            permit.request.config.epochs,
        )?;
        let mut artifact = self
            .trainer
            .train(&examples, &permit.request.config, &version.model_version)
            .map_err(internal)?;
        permit.token.check()?;
        update_run(
            &self.repository,
            run,
            TrainingStage::Validating,
            0,
            examples.len() as u32,
        )?;
        self.rendered_metrics(&dataset, &examples, &mut artifact, &permit.token, run)?;
        update_run(&self.repository, run, TrainingStage::ExportingStyle, 0, 1)?;
        let (artifact_path, package) = package::export_style_package(
            &self.style_root,
            &version,
            &dataset,
            &artifact,
            &permit.token,
        )
        .map_err(internal)?;
        run.style_id = Some(package.manifest.style_id);
        run.style_version = Some(package.manifest.version);
        run.style_name = package.manifest.name;
        run.metrics = Some(artifact.metrics);
        run.artifact_path = Some(artifact_path);
        run.status = TrainingRunStatus::Complete;
        run.stage = TrainingStage::Complete;
        run.completed = 1;
        run.total = 1;
        run.duration_ms = started.elapsed().as_millis() as u64;
        run.updated_at = chrono::Utc::now().to_rfc3339();
        self.repository.save_training_run(run)?;
        Ok(())
    }

    fn examples(
        &self,
        dataset: &TrainingDataset,
        token: &CancellationToken,
        run: &mut TrainingRun,
    ) -> ProcessingResult<Vec<TrainingExample>> {
        let batch_id = dataset
            .batch_context_id
            .clone()
            .unwrap_or_else(|| digest(&["training-missing-context", &dataset.dataset_id]));
        let mut examples = Vec::new();
        for (index, pair) in dataset.pairs.iter().enumerate() {
            token.check()?;
            let Some(target) = pair.target.as_ref() else {
                continue;
            };
            if pair.split == TrainingSplit::Excluded {
                continue;
            }
            let analysis = self
                .analysis
                .get_analysis(&dataset.job_id, &pair.source_asset_id, dataset.photo_type)?
                .analysis
                .ok_or_else(|| internal("Current source analysis is unavailable"))?;
            let fallback = missing_context(&pair.source_asset_id);
            let context = pair.batch_context.as_ref().unwrap_or(&fallback);
            let features = build_features(&analysis, context, &batch_id).map_err(internal)?;
            examples.push(TrainingExample {
                pair_id: pair.pair_id.clone(),
                features,
                target: target.controls,
                confidence: target.confidence,
                split: pair.split,
            });
            run.completed = (index + 1) as u32;
            run.updated_at = chrono::Utc::now().to_rfc3339();
            self.repository.save_training_run(run)?;
        }
        Ok(examples)
    }

    fn rendered_metrics(
        &self,
        dataset: &TrainingDataset,
        examples: &[TrainingExample],
        artifact: &mut trainer::TrainedModelArtifact,
        token: &CancellationToken,
        run: &mut TrainingRun,
    ) -> ProcessingResult<()> {
        let pairs = dataset
            .pairs
            .iter()
            .map(|pair| (pair.pair_id.as_str(), pair))
            .collect::<HashMap<_, _>>();
        let mut train_losses = Vec::new();
        let mut validation_losses = Vec::new();
        let mut neutral_losses = Vec::new();
        let mut mean_losses = Vec::new();
        for (index, example) in examples.iter().enumerate() {
            token.check()?;
            let pair = pairs
                .get(example.pair_id.as_str())
                .copied()
                .ok_or_else(|| internal("Training example lost its pair"))?;
            let prediction = predict_controls(&artifact.model, &example.features);
            let loss = self.optimizer.rendered_loss(pair, prediction, token)?;
            match example.split {
                TrainingSplit::Train => train_losses.push(loss),
                TrainingSplit::Validation => {
                    validation_losses.push(loss);
                    neutral_losses.push(self.optimizer.rendered_loss(
                        pair,
                        PredictedCreativeAdjustments::default(),
                        token,
                    )?);
                    mean_losses.push(self.optimizer.rendered_loss(
                        pair,
                        artifact.mean_controls,
                        token,
                    )?);
                }
                _ => {}
            }
            run.completed = (index + 1) as u32;
            run.updated_at = chrono::Utc::now().to_rfc3339();
            self.repository.save_training_run(run)?;
        }
        artifact.metrics.train.rendered_loss = average(&train_losses);
        artifact.metrics.validation.rendered_loss = average(&validation_losses);
        artifact.metrics.neutral_baseline.rendered_loss = average(&neutral_losses);
        artifact.metrics.mean_baseline.rendered_loss = average(&mean_losses);
        artifact.metrics.beats_mean_baseline = match (
            artifact.metrics.validation.rendered_loss,
            artifact.metrics.mean_baseline.rendered_loss,
        ) {
            (Some(model), Some(mean)) => model < mean,
            _ => false,
        };
        if !artifact.metrics.beats_mean_baseline {
            artifact.metrics.warnings.push(
                "The trained model did not beat the mean-recipe baseline on held-out rendered loss; do not claim learning success from this run"
                    .into(),
            );
        }
        Ok(())
    }

    pub fn previews(
        &self,
        dataset_id: &str,
        pair_id: &str,
    ) -> ProcessingResult<TrainingPreviewSet> {
        let dataset = self.dataset(dataset_id)?;
        let pair = dataset
            .pairs
            .iter()
            .find(|pair| pair.pair_id == pair_id)
            .ok_or_else(|| internal("Training pair was not found"))?;
        let target = pair.target.as_ref();
        let identity = digest(&[
            &pair.source_fingerprint,
            &pair.reference_fingerprint,
            RENDERER_VERSION,
            target
                .map(|t| t.cache_identity.as_str())
                .unwrap_or("before-training"),
        ]);
        let directory = self.cache_root.join("pair-previews");
        fs::create_dir_all(&directory).map_err(crate::rendering::io_error)?;
        let source_path = directory.join(format!("{identity}-source.jpg"));
        let target_path = directory.join(format!("{identity}-target.jpg"));
        let reference_path = directory.join(format!("{identity}-reference.jpg"));
        render_cached(
            &self.engine,
            &pair.source_path,
            &EditRecipe::neutral(
                format!("training-source-{}", pair.pair_id),
                pair.source_asset_id.clone(),
                "1970-01-01T00:00:00Z".into(),
            ),
            &pair.source_asset_id,
            &source_path,
        )?;
        if let Some(target) = target {
            render_cached(
                &self.engine,
                &pair.source_path,
                &target.recipe,
                &pair.source_asset_id,
                &target_path,
            )?;
        }
        let reference_asset = format!("training-reference-{}", pair.pair_id);
        render_cached(
            &self.engine,
            &pair.reference_path,
            &EditRecipe::neutral(
                format!("training-reference-recipe-{}", pair.pair_id),
                reference_asset.clone(),
                "1970-01-01T00:00:00Z".into(),
            ),
            &reference_asset,
            &reference_path,
        )?;
        let state = self
            .repository
            .get_recipe(&dataset.job_id, &pair.source_asset_id)?;
        let ai_data = if state.error.is_none()
            && state.recipe.provenance.origin == RecipeOrigin::TrainedStyle
        {
            let ai_path = directory.join(format!("{}-ai-{}.jpg", identity, state.recipe_hash));
            render_cached(
                &self.engine,
                &pair.source_path,
                &state.recipe,
                &pair.source_asset_id,
                &ai_path,
            )?;
            Some(data_url(&ai_path)?)
        } else {
            None
        };
        Ok(TrainingPreviewSet {
            source_data: data_url(&source_path)?,
            ai_data,
            target_data: target.map(|_| data_url(&target_path)).transpose()?,
            reference_data: data_url(&reference_path)?,
        })
    }

    pub fn feedback(
        &self,
        dataset_id: &str,
        pair_id: &str,
        feedback: ValidationFeedback,
    ) -> ProcessingResult<TrainingDataset> {
        let _guard = self.idle_matching()?;
        let mut dataset = self.dataset(dataset_id)?;
        let pair = dataset
            .pairs
            .iter_mut()
            .find(|pair| pair.pair_id == pair_id)
            .ok_or_else(|| internal("Training pair was not found"))?;
        pair.feedback = Some(feedback);
        dataset.updated_at = chrono::Utc::now().to_rfc3339();
        self.repository.save_training_dataset(&dataset)?;
        Ok(dataset)
    }

    pub fn prepare_validation_editing(
        &self,
        dataset_id: &str,
    ) -> ProcessingResult<ValidationEditingSelection> {
        let dataset = self.dataset(dataset_id)?;
        let selected = dataset
            .pairs
            .iter()
            .filter(|pair| pair.split == TrainingSplit::Validation)
            .map(|pair| pair.source_asset_id.clone())
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err(internal("This run has no held-out validation photos"));
        }
        let selected_set = selected.iter().collect::<std::collections::HashSet<_>>();
        let snapshot = self
            .all_assets(&dataset.job_id)?
            .into_iter()
            .map(|asset| {
                let value = selected_set.contains(&asset.id);
                (asset.id, value)
            })
            .collect::<Vec<_>>();
        self.repository.culling_select(&dataset.job_id, &snapshot)?;
        Ok(ValidationEditingSelection {
            photo_type: dataset.photo_type,
            asset_ids: selected,
        })
    }

    fn all_assets(&self, job: &str) -> ProcessingResult<Vec<Asset>> {
        let mut offset = 0;
        let mut assets = Vec::new();
        loop {
            let page = self.repository.assets(job, offset, 100).map_err(internal)?;
            let count = page.items.len() as u32;
            assets.extend(page.items);
            offset += count;
            if u64::from(offset) >= page.total || count == 0 {
                break;
            }
        }
        Ok(assets)
    }

    fn create_standalone_job(&self, dataset_id: &str) -> ProcessingResult<String> {
        let root = self.cache_root.join("datasets").join(dataset_id);
        let input = root.join("before");
        let output = root.join("output");
        fs::create_dir_all(&input).map_err(crate::rendering::io_error)?;
        fs::create_dir_all(&output).map_err(crate::rendering::io_error)?;
        let job = self
            .repository
            .create_job(&NewJob {
                name: format!("__training__{dataset_id}"),
                input_path: input,
                output_path: output,
            })
            .map_err(internal)?;
        self.repository
            .set_status(&job.id, "ready", 0, None)
            .map_err(internal)?;
        Ok(job.id)
    }

    fn ensure_training_asset(
        &self,
        dataset: &TrainingDataset,
        path: &Path,
    ) -> ProcessingResult<Asset> {
        let discovered = inspect_file(path).map_err(internal)?;
        let asset = Asset {
            id: discovered.id,
            job_id: dataset.job_id.clone(),
            original_path: discovered.original_path,
            filename: discovered.filename,
            file_type: discovered.file_type,
            file_size: discovered.file_size,
            modified_at: discovered.modified_at,
            fingerprint: discovered.fingerprint,
            metadata: Default::default(),
            thumbnail_path: None,
            // The repository schema uses the same lifecycle values as normal
            // ingestion; standalone inputs are immediately usable by the
            // training renderer, so mark their preview as ready.
            preview_status: "ready".into(),
            metadata_warning: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            warnings: Vec::new(),
        };
        self.repository
            .save_assets(std::slice::from_ref(&asset))
            .map_err(internal)?;
        Ok(asset)
    }

    fn pair_from_paths(
        &self,
        dataset: &TrainingDataset,
        source_asset_id: String,
        source_path: PathBuf,
        reference_path: PathBuf,
    ) -> ProcessingResult<TrainingPair> {
        self.pair_from_paths_with_token(
            dataset,
            source_asset_id,
            source_path,
            reference_path,
            &CancellationToken::default(),
        )
    }

    fn pair_from_paths_with_token(
        &self,
        dataset: &TrainingDataset,
        source_asset_id: String,
        source_path: PathBuf,
        reference_path: PathBuf,
        token: &CancellationToken,
    ) -> ProcessingResult<TrainingPair> {
        let source_fingerprint = file_fingerprint(&source_path, token)?;
        let reference_fingerprint = file_fingerprint(&reference_path, token)?;
        Ok(TrainingPair {
            schema_version: TRAINING_PAIR_SCHEMA_VERSION,
            pair_id: digest(&[
                "training-pair-v1",
                &dataset.dataset_id,
                &source_asset_id,
                &reference_fingerprint,
            ]),
            dataset_id: dataset.dataset_id.clone(),
            source_job_id: dataset.job_id.clone(),
            source_asset_id,
            source_path,
            reference_path,
            photo_type: dataset.photo_type,
            source_fingerprint,
            reference_fingerprint,
            validation: PairValidation::default(),
            source_analysis_id: None,
            batch_context: None,
            scene_group_id: None,
            target: None,
            split: TrainingSplit::Unassigned,
            excluded: false,
            feedback: None,
            diagnostics: Vec::new(),
        })
    }
}

fn current_culling(
    repository: &JobRepository,
    job: &str,
    asset: &str,
    photo_type: PhotoType,
    analysis: Option<&photo_contracts::analysis::PhotoAnalysis>,
) -> Option<CullingAssessment> {
    repository
        .culling_state(job, asset, photo_type)
        .ok()
        .and_then(|state| state.assessment)
        .filter(|assessment| {
            analysis.is_some_and(|analysis| {
                assessment.source_analysis_id.as_deref() == Some(&analysis.analysis_id)
                    && assessment.source_fingerprint == analysis.source_fingerprint
            })
        })
}

fn missing_context(asset: &str) -> AssetBatchContext {
    AssetBatchContext {
        asset_id: asset.into(),
        availability: ContextAvailability::Unavailable,
        scene_group_id: None,
        lighting_group_id: None,
        sequence_group_id: None,
        reference_asset_id: None,
        exposure_delta_from_group: None,
        wb_delta_from_group: None,
        group_confidence: 0.0,
        consistency_notes: vec![ConsistencyNote {
            code: ConsistencyNoteCode::AnalysisUnavailable,
            message: "Training batch context unavailable".into(),
        }],
    }
}

fn update_run(
    repository: &JobRepository,
    run: &mut TrainingRun,
    stage: TrainingStage,
    completed: u32,
    total: u32,
) -> ProcessingResult<()> {
    run.status = TrainingRunStatus::Running;
    run.stage = stage;
    run.completed = completed;
    run.total = total;
    run.updated_at = chrono::Utc::now().to_rfc3339();
    repository.save_training_run(run)
}

fn source_supported(path: &Path) -> bool {
    photo_format(path).is_some_and(|format| {
        matches!(
            format.file_type,
            FileType::Cr3
                | FileType::Cr2
                | FileType::Arw
                | FileType::Dng
                | FileType::Jpg
                | FileType::Jpeg
                | FileType::Tif
                | FileType::Tiff
                | FileType::Png
        )
    })
}

fn reference_supported(path: &Path) -> bool {
    photo_format(path).is_some_and(|format| {
        matches!(
            format.family,
            FormatFamily::Jpeg | FormatFamily::Tiff | FormatFamily::Png
        )
    })
}

fn discover_training_paths(
    folder: &Path,
    before: bool,
    excluded: &[&Path],
) -> ProcessingResult<Vec<PathBuf>> {
    let excluded = excluded
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect::<Vec<_>>();
    let folder = folder.canonicalize().map_err(crate::rendering::io_error)?;
    if !folder.is_dir() {
        return Err(internal("The selected training folder was not found"));
    }
    let mut paths = walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            if excluded.iter().any(|path| entry.path().starts_with(path)) {
                return false;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with('.') {
                return false;
            }
            if entry.file_type().is_dir()
                && matches!(
                    name.as_str(),
                    "photoeditor-cache"
                        | "photoeditor-output"
                        | "training-cache"
                        | "trained-styles"
                        | "node_modules"
                        | "target"
                )
            {
                return false;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if entry.metadata().is_ok_and(|m| m.file_attributes() & 6 != 0) {
                    return false;
                }
            }
            true
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| {
            if before {
                source_supported(path)
            } else {
                reference_supported(path)
            }
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| matcher::natural_cmp(a, b));
    Ok(paths)
}

fn file_fingerprint(path: &Path, cancel: &CancellationToken) -> ProcessingResult<String> {
    let mut file = fs::File::open(path).map_err(crate::rendering::io_error)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        cancel.check()?;
        let count = file.read(&mut buffer).map_err(crate::rendering::io_error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn dataset_identity(dataset: &TrainingDataset) -> String {
    let mut entries = dataset
        .pairs
        .iter()
        .map(|pair| {
            format!(
                "{}:{}:{}:{}:{:?}",
                pair.pair_id,
                pair.source_fingerprint,
                pair.reference_fingerprint,
                pair.source_analysis_id
                    .as_deref()
                    .unwrap_or("analysis-unavailable"),
                pair.validation.status
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    digest(&[
        "training-dataset-v1",
        &dataset.dataset_id,
        &dataset.feature_schema,
        &dataset.renderer_version,
        &entries.join("|"),
    ])
}

fn invalidate_dataset(dataset: &mut TrainingDataset) {
    dataset.dataset_fingerprint = None;
    dataset.batch_context_id = None;
    dataset.updated_at = chrono::Utc::now().to_rfc3339();
}

fn size_guidance(count: usize) -> String {
    match count {
        0..=9 => format!(
            "{count} training pairs — experimental dataset; at least 10 varied examples are recommended"
        ),
        10..=19 => format!(
            "{count} training pairs — experimental dataset; more varied examples are recommended"
        ),
        20..=50 => format!("{count} training pairs — reasonable first-style dataset"),
        _ => format!("{count} training pairs — broader coverage, subject to holdout review"),
    }
}

fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

fn average(values: &[f32]) -> Option<f32> {
    (!values.is_empty()).then(|| values.iter().sum::<f32>() / values.len() as f32)
}

fn render_cached(
    engine: &CpuProcessingEngine,
    source: &Path,
    recipe: &EditRecipe,
    asset_id: &str,
    destination: &Path,
) -> ProcessingResult<()> {
    if destination.exists() {
        return Ok(());
    }
    engine.render_recipe(
        recipe,
        &RenderRequest {
            asset_id: asset_id.into(),
            original: source.to_path_buf(),
            adjustments: RenderAdjustments::default(),
            source_metadata: Default::default(),
            destination: destination.to_path_buf(),
            output_format: OutputFormat::Jpeg,
            preview: true,
            jpeg_quality: 90,
        },
        &CancellationToken::default(),
    )?;
    Ok(())
}

fn data_url(path: &Path) -> ProcessingResult<String> {
    let bytes = fs::read(path).map_err(crate::rendering::io_error)?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
