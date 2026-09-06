//! Local Training Studio contracts. Training learns creative recipe controls, never pixels.
use crate::{
    analysis::PhotoType,
    batch_context::AssetBatchContext,
    trained_style::{PredictedCreativeAdjustments, StyleControl, STYLE_FEATURE_SCHEMA_V1},
    EditRecipe, RECIPE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub const TRAINING_PAIR_SCHEMA_VERSION: u32 = 1;
pub const TRAINING_DATASET_SCHEMA_VERSION: u32 = 1;
pub const TARGET_RECIPE_SCHEMA_VERSION: u32 = 1;
pub const TRAINING_RUN_SCHEMA_VERSION: u32 = 1;
pub const TRAINER_VERSION: &str = "regularized-linear-recipe-v1";
pub const TARGET_OPTIMIZER_VERSION: &str = "staged-photographic-proxy-v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairValidationStatus {
    #[default]
    Pending,
    Ready,
    NeedsReview,
    Rejected,
    Unusable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeometryRelationship {
    #[default]
    Unknown,
    ExactOrNear,
    CropDifference,
    MajorMismatch,
    Unusable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingSplit {
    #[default]
    Unassigned,
    Train,
    Validation,
    Excluded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetFitConfidence {
    High,
    Medium,
    Low,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationFeedback {
    Accept,
    NeedsAdjustment,
    Reject,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PairValidation {
    pub status: PairValidationStatus,
    pub geometry: GeometryRelationship,
    pub structural_similarity: Option<f32>,
    pub source_width: Option<u32>,
    pub source_height: Option<u32>,
    pub reference_width: Option<u32>,
    pub reference_height: Option<u32>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrainingAlignment {
    pub before_count: u32,
    pub after_count: u32,
    pub matched_count: u32,
    pub ambiguous_count: u32,
    pub unmatched_before: Vec<PathBuf>,
    pub unmatched_after: Vec<PathBuf>,
    pub first_before: Option<PathBuf>,
    pub first_after: Option<PathBuf>,
    pub last_before: Option<PathBuf>,
    pub last_after: Option<PathBuf>,
    pub start_aligned: bool,
    pub end_aligned: bool,
    pub order_fallback_used: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetLossBreakdown {
    pub total: f32,
    pub luminance: f32,
    pub color_balance: f32,
    pub saturation: f32,
    pub structure: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRecipeResult {
    pub schema_version: u32,
    pub optimizer_version: String,
    pub cache_identity: String,
    pub recipe: EditRecipe,
    pub controls: PredictedCreativeAdjustments,
    pub confidence: TargetFitConfidence,
    pub loss: TargetLossBreakdown,
    pub iterations: u32,
    pub unsupported_differences: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingPair {
    pub schema_version: u32,
    pub pair_id: String,
    pub dataset_id: String,
    pub source_job_id: String,
    pub source_asset_id: String,
    pub source_path: PathBuf,
    pub reference_path: PathBuf,
    pub photo_type: PhotoType,
    pub source_fingerprint: String,
    pub reference_fingerprint: String,
    pub validation: PairValidation,
    pub source_analysis_id: Option<String>,
    pub batch_context: Option<AssetBatchContext>,
    pub scene_group_id: Option<String>,
    pub target: Option<TargetRecipeResult>,
    pub split: TrainingSplit,
    pub excluded: bool,
    pub feedback: Option<ValidationFeedback>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingDataset {
    pub schema_version: u32,
    pub dataset_id: String,
    pub job_id: String,
    pub style_name: String,
    pub photo_type: PhotoType,
    pub pairs: Vec<TrainingPair>,
    pub created_at: String,
    pub updated_at: String,
    pub dataset_fingerprint: Option<String>,
    pub feature_schema: String,
    pub renderer_version: String,
    pub target_recipe_schema: u32,
    pub batch_context_id: Option<String>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub before_files: Vec<PathBuf>,
    #[serde(default)]
    pub after_files: Vec<PathBuf>,
    #[serde(default)]
    pub alignment: Option<TrainingAlignment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrainingConfig {
    pub validation_percent: u8,
    pub regularization: f32,
    pub learning_rate: f32,
    pub epochs: u32,
    pub exclude_low_confidence: bool,
    pub deterministic: bool,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            validation_percent: 20,
            regularization: 0.08,
            learning_rate: 0.025,
            epochs: 1_200,
            exclude_low_confidence: true,
            deterministic: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MetricSet {
    pub recipe_mae: BTreeMap<StyleControl, f32>,
    pub mean_recipe_mae: f32,
    pub rendered_loss: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingMetrics {
    pub train: MetricSet,
    pub validation: MetricSet,
    pub neutral_baseline: MetricSet,
    pub mean_baseline: MetricSet,
    pub beats_mean_baseline: bool,
    pub overfitting_warning: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingRunStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingStage {
    Queued,
    ValidatingPairs,
    Analyzing,
    EstimatingTargetRecipes,
    BuildingExamples,
    Training,
    Validating,
    ExportingStyle,
    Complete,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingRun {
    pub schema_version: u32,
    pub run_id: String,
    pub dataset_id: String,
    pub style_id: Option<String>,
    pub style_name: String,
    pub style_version: Option<String>,
    pub status: TrainingRunStatus,
    pub stage: TrainingStage,
    pub completed: u32,
    pub total: u32,
    pub config: TrainingConfig,
    pub metrics: Option<TrainingMetrics>,
    pub artifact_path: Option<PathBuf>,
    pub started_at: String,
    pub updated_at: String,
    pub duration_ms: u64,
    pub error: Option<String>,
}

fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl TrainingPair {
    pub fn validate_shape(&self) -> Result<(), String> {
        if self.schema_version != TRAINING_PAIR_SCHEMA_VERSION {
            return Err("Unsupported training-pair schema".into());
        }
        if self.pair_id.is_empty()
            || self.dataset_id.is_empty()
            || self.source_job_id.is_empty()
            || self.source_asset_id.is_empty()
            || self.source_path.as_os_str().is_empty()
            || self.reference_path.as_os_str().is_empty()
            || (!self.source_fingerprint.is_empty() && !digest(&self.source_fingerprint))
            || (!self.reference_fingerprint.is_empty() && !digest(&self.reference_fingerprint))
        {
            return Err("Training-pair identity is invalid".into());
        }
        if let Some(target) = &self.target {
            if target.schema_version != TARGET_RECIPE_SCHEMA_VERSION
                || target.optimizer_version != TARGET_OPTIMIZER_VERSION
                || !digest(&target.cache_identity)
                || !target.loss.total.is_finite()
            {
                return Err("Target recipe result is invalid".into());
            }
            target
                .recipe
                .clone()
                .validated()
                .map_err(|error| error.message)?;
        }
        Ok(())
    }
}

impl TrainingDataset {
    pub fn validate_shape(&self) -> Result<(), String> {
        if self.schema_version != TRAINING_DATASET_SCHEMA_VERSION
            || self.dataset_id.is_empty()
            || self.job_id.is_empty()
            || self.style_name.trim().is_empty()
            || self.style_name.len() > 256
            || self.feature_schema != STYLE_FEATURE_SCHEMA_V1
            || self.renderer_version.is_empty()
            || self.target_recipe_schema != RECIPE_SCHEMA_VERSION
            || self
                .dataset_fingerprint
                .as_ref()
                .is_some_and(|value| !digest(value))
        {
            return Err("Training dataset identity or compatibility is invalid".into());
        }
        for pair in &self.pairs {
            pair.validate_shape()?;
            if pair.dataset_id != self.dataset_id
                || pair.source_job_id != self.job_id
                || pair.photo_type != self.photo_type
            {
                return Err("Training pair does not belong to this dataset".into());
            }
        }
        Ok(())
    }
}

impl TrainingConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=50).contains(&self.validation_percent)
            || !self.regularization.is_finite()
            || !(0.0..=10.0).contains(&self.regularization)
            || !self.learning_rate.is_finite()
            || !(0.0001..=0.25).contains(&self.learning_rate)
            || !(10..=20_000).contains(&self.epochs)
        {
            return Err("Training configuration is outside safe bounds".into());
        }
        Ok(())
    }
}
