//! Portable trained-style packages, stable inference features and creative predictions.
//! Models predict controls; they never render pixels or replace objective corrections.
use crate::{analysis::PhotoType, RECIPE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const TRAINED_STYLE_SCHEMA_VERSION: u32 = 1;
pub const STYLE_PACKAGE_SCHEMA_VERSION: u32 = 1;
pub const STYLE_MODEL_SCHEMA_VERSION: u32 = 1;
pub const STYLE_RULES_SCHEMA_VERSION: u32 = 1;
pub const STYLE_METADATA_SCHEMA_VERSION: u32 = 1;
pub const STYLE_INTEGRITY_SCHEMA_VERSION: u32 = 1;
pub const STYLE_FEATURE_SCHEMA_V1: &str = "style_features_v1";
pub const MAX_STYLE_PACKAGE_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_STYLE_FEATURES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleControl {
    ExposureEv,
    TemperatureDelta,
    Tint,
    Contrast,
    Highlights,
    Shadows,
    Whites,
    Blacks,
    Saturation,
    Vibrance,
    Texture,
    Clarity,
    Dehaze,
    SharpeningAmount,
    NoiseReduction,
    VignetteAmount,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendererCompatibility {
    pub recipe_schema_versions: Vec<u32>,
    pub minimum_renderer_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFileReference {
    pub path: String,
    pub format: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainedStyle {
    pub schema_version: u32,
    pub package_schema_version: u32,
    pub style_id: String,
    pub name: String,
    pub version: String,
    pub photo_type: PhotoType,
    pub model_version: String,
    pub feature_schema: String,
    pub renderer_compatibility: RendererCompatibility,
    pub supported_controls: Vec<StyleControl>,
    pub model: PackageFileReference,
    pub rules: PackageFileReference,
    pub metadata: PackageFileReference,
    pub integrity: PackageFileReference,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearOutput {
    pub control: StyleControl,
    pub intercept: f32,
    pub weights: Vec<f32>,
    /// Contribution when a corresponding feature is unavailable. The neutral feature
    /// value remains zero, so absence is explicit rather than an invented measurement.
    pub missing_weights: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearStyleModel {
    pub feature_names: Vec<String>,
    /// Training-time feature normalization. Empty means identity normalization for
    /// legacy/development packages; trained packages persist one entry per feature.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_means: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feature_scales: Vec<f32>,
    pub outputs: Vec<LinearOutput>,
    pub base_confidence: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model_type", content = "parameters", rename_all = "snake_case")]
pub enum StyleModelKind {
    LinearV1(LinearStyleModel),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleModel {
    pub schema_version: u32,
    pub feature_schema: String,
    pub model_version: String,
    pub model: StyleModelKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleOutputBound {
    pub minimum: f32,
    pub maximum: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleRules {
    pub schema_version: u32,
    pub output_bounds: BTreeMap<StyleControl, StyleOutputBound>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleMetadata {
    pub schema_version: u32,
    pub description: String,
    pub author: String,
    pub created_at: String,
    pub development_only: bool,
    pub trained_from_user_photos: bool,
    pub training_provenance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training: Option<TrainingPackageProvenance>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingPackageProvenance {
    pub dataset_identity: String,
    pub training_pairs: u32,
    pub validation_pairs: u32,
    pub feature_schema: String,
    pub target_recipe_schema: u32,
    pub trainer_version: String,
    pub renderer_version: String,
    pub trained_at: String,
    pub metrics_summary: BTreeMap<String, f32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleSignatureMetadata {
    pub scheme: String,
    pub signer: Option<String>,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StylePackageIntegrity {
    pub schema_version: u32,
    pub algorithm: String,
    /// SHA-256 of canonical JSON for every payload file except this integrity document.
    pub files: BTreeMap<String, String>,
    pub package_identity: String,
    pub signature: StyleSignatureMetadata,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadedStylePackage {
    pub manifest: TrainedStyle,
    pub model: StyleModel,
    pub rules: StyleRules,
    pub metadata: StyleMetadata,
    pub integrity: StylePackageIntegrity,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleFeatureVector {
    pub schema_version: String,
    pub asset_id: String,
    pub analysis_id: String,
    pub batch_context_id: String,
    pub feature_names: Vec<String>,
    pub values: Vec<f32>,
    pub available: Vec<bool>,
    pub missing_features: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PredictedCreativeAdjustments {
    pub exposure_ev: f32,
    /// Delta from the recipe/renderer's neutral 6500 source-relative control.
    pub temperature_delta: f32,
    pub tint: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub texture: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub sharpening_amount: f32,
    pub noise_reduction: f32,
    pub vignette_amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleConfidence {
    High,
    Medium,
    Low,
    InsufficientEvidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StylePredictionDiagnostics {
    pub resolver: String,
    pub missing_feature_count: u32,
    pub bounded_controls: Vec<StyleControl>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StylePrediction {
    pub style_id: String,
    pub style_version: String,
    pub model_version: String,
    pub package_identity: String,
    pub feature_schema: String,
    pub adjustments: PredictedCreativeAdjustments,
    pub confidence: StyleConfidence,
    pub confidence_score: f32,
    pub diagnostics: StylePredictionDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum StyleError {
    #[error("Unsupported trained-style version: {0}")]
    UnsupportedVersion(u32),
    #[error("Incompatible style feature schema: {0}")]
    IncompatibleFeatureSchema(String),
    #[error("Corrupt style package: {0}")]
    CorruptPackage(String),
    #[error("Invalid style model: {0}")]
    InvalidModel(String),
    #[error("Invalid style prediction: {0}")]
    InvalidPrediction(String),
}

fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn package_path(value: &str, expected: &str) -> bool {
    value == expected && !value.contains(['/', '\\'])
}

impl TrainedStyle {
    pub fn validate(&self) -> Result<(), StyleError> {
        if self.schema_version != TRAINED_STYLE_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedVersion(self.schema_version));
        }
        if self.package_schema_version != STYLE_PACKAGE_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedVersion(self.package_schema_version));
        }
        if !text(&self.style_id, 128)
            || !text(&self.name, 256)
            || !text(&self.version, 64)
            || !text(&self.model_version, 128)
            || self.feature_schema != STYLE_FEATURE_SCHEMA_V1
            || self.supported_controls.is_empty()
            || self.supported_controls.len() > 32
            || self
                .renderer_compatibility
                .minimum_renderer_version
                .is_empty()
            || !self
                .renderer_compatibility
                .recipe_schema_versions
                .contains(&RECIPE_SCHEMA_VERSION)
            || !package_path(&self.model.path, "model.json")
            || self.model.format != "linear_json_v1"
            || !package_path(&self.rules.path, "rules.json")
            || self.rules.format != "style_rules_v1"
            || !package_path(&self.metadata.path, "metadata.json")
            || self.metadata.format != "style_metadata_v1"
            || !package_path(&self.integrity.path, "checksums.json")
            || self.integrity.format != "sha256_canonical_json_v1"
        {
            return Err(StyleError::CorruptPackage(
                "Invalid or incompatible style manifest".into(),
            ));
        }
        let unique = self.supported_controls.iter().collect::<HashSet<_>>();
        if unique.len() != self.supported_controls.len() {
            return Err(StyleError::CorruptPackage(
                "Supported controls must be unique".into(),
            ));
        }
        Ok(())
    }
}

impl StyleModel {
    pub fn validate(&self, expected_features: &[&str]) -> Result<(), StyleError> {
        if self.schema_version != STYLE_MODEL_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedVersion(self.schema_version));
        }
        if self.feature_schema != STYLE_FEATURE_SCHEMA_V1 {
            return Err(StyleError::IncompatibleFeatureSchema(
                self.feature_schema.clone(),
            ));
        }
        let StyleModelKind::LinearV1(model) = &self.model;
        let normalization_absent =
            model.feature_means.is_empty() && model.feature_scales.is_empty();
        let normalization_valid = model.feature_means.len() == expected_features.len()
            && model.feature_scales.len() == expected_features.len()
            && model.feature_means.iter().all(|value| value.is_finite())
            && model
                .feature_scales
                .iter()
                .all(|value| value.is_finite() && *value > 1e-6);
        if self.model_version.is_empty()
            || model.feature_names.len() != expected_features.len()
            || model.feature_names.len() > MAX_STYLE_FEATURES
            || model
                .feature_names
                .iter()
                .map(String::as_str)
                .ne(expected_features.iter().copied())
            || !model.base_confidence.is_finite()
            || !(0.0..=1.0).contains(&model.base_confidence)
            || model.outputs.is_empty()
            || (!normalization_absent && !normalization_valid)
        {
            return Err(StyleError::InvalidModel(
                "Linear model feature layout or confidence is invalid".into(),
            ));
        }
        let mut controls = HashSet::new();
        if model.outputs.iter().any(|output| {
            !controls.insert(output.control)
                || !output.intercept.is_finite()
                || output.weights.len() != expected_features.len()
                || output.missing_weights.len() != expected_features.len()
                || output
                    .weights
                    .iter()
                    .chain(&output.missing_weights)
                    .any(|value| !value.is_finite())
        }) {
            return Err(StyleError::InvalidModel(
                "Linear model output is non-finite, duplicated, or dimensionally invalid".into(),
            ));
        }
        Ok(())
    }
}

impl StyleRules {
    pub fn validate(&self, supported: &[StyleControl]) -> Result<(), StyleError> {
        if self.schema_version != STYLE_RULES_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedVersion(self.schema_version));
        }
        if self.output_bounds.len() != supported.len()
            || supported
                .iter()
                .any(|control| !self.output_bounds.contains_key(control))
            || self.output_bounds.values().any(|bound| {
                !bound.minimum.is_finite()
                    || !bound.maximum.is_finite()
                    || bound.minimum > bound.maximum
            })
        {
            return Err(StyleError::CorruptPackage(
                "Output bounds do not match supported controls".into(),
            ));
        }
        Ok(())
    }
}

impl StyleMetadata {
    pub fn validate(&self) -> Result<(), StyleError> {
        if self.schema_version != STYLE_METADATA_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedVersion(self.schema_version));
        }
        if !text(&self.description, 2048)
            || !text(&self.author, 256)
            || !text(&self.training_provenance, 2048)
            || chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err()
        {
            return Err(StyleError::CorruptPackage("Invalid style metadata".into()));
        }
        if let Some(training) = &self.training {
            if !digest(&training.dataset_identity)
                || training.training_pairs == 0
                || training.feature_schema != STYLE_FEATURE_SCHEMA_V1
                || training.target_recipe_schema != RECIPE_SCHEMA_VERSION
                || !training
                    .metrics_summary
                    .values()
                    .all(|value| value.is_finite())
                || !text(&training.trainer_version, 128)
                || !text(&training.renderer_version, 128)
                || chrono::DateTime::parse_from_rfc3339(&training.trained_at).is_err()
            {
                return Err(StyleError::CorruptPackage(
                    "Invalid training provenance metadata".into(),
                ));
            }
        }
        Ok(())
    }
}

impl StylePackageIntegrity {
    pub fn validate(&self) -> Result<(), StyleError> {
        if self.schema_version != STYLE_INTEGRITY_SCHEMA_VERSION {
            return Err(StyleError::UnsupportedVersion(self.schema_version));
        }
        let required = ["metadata.json", "model.json", "rules.json", "style.json"];
        if self.algorithm != "sha256"
            || self.files.len() != required.len()
            || required.iter().any(|name| {
                self.files
                    .get(*name)
                    .is_none_or(|identity| !digest(identity))
            })
            || !digest(&self.package_identity)
            || self.signature.scheme != "unsigned_sha256_v1"
            || self.signature.signer.is_some()
            || self.signature.value.is_some()
        {
            return Err(StyleError::CorruptPackage(
                "Invalid package integrity metadata".into(),
            ));
        }
        Ok(())
    }
}

impl StyleFeatureVector {
    pub fn validate(&self, expected_features: &[&str]) -> Result<(), StyleError> {
        if self.schema_version != STYLE_FEATURE_SCHEMA_V1 {
            return Err(StyleError::IncompatibleFeatureSchema(
                self.schema_version.clone(),
            ));
        }
        if self.asset_id.is_empty()
            || self.analysis_id.is_empty()
            || !digest(&self.batch_context_id)
            || self.feature_names.len() != expected_features.len()
            || self.values.len() != expected_features.len()
            || self.available.len() != expected_features.len()
            || self
                .feature_names
                .iter()
                .map(String::as_str)
                .ne(expected_features.iter().copied())
            || self.values.iter().any(|value| !value.is_finite())
            || self.missing_features.len() != self.available.iter().filter(|value| !**value).count()
        {
            return Err(StyleError::InvalidModel(
                "Invalid style feature vector".into(),
            ));
        }
        Ok(())
    }
}

impl StylePrediction {
    pub fn validate(&self) -> Result<(), StyleError> {
        let values = [
            self.adjustments.exposure_ev,
            self.adjustments.temperature_delta,
            self.adjustments.tint,
            self.adjustments.contrast,
            self.adjustments.highlights,
            self.adjustments.shadows,
            self.adjustments.whites,
            self.adjustments.blacks,
            self.adjustments.saturation,
            self.adjustments.vibrance,
            self.adjustments.texture,
            self.adjustments.clarity,
            self.adjustments.dehaze,
            self.adjustments.sharpening_amount,
            self.adjustments.noise_reduction,
            self.adjustments.vignette_amount,
            self.confidence_score,
        ];
        if self.style_id.is_empty()
            || self.style_version.is_empty()
            || self.model_version.is_empty()
            || !digest(&self.package_identity)
            || self.feature_schema != STYLE_FEATURE_SCHEMA_V1
            || values.iter().any(|value| !value.is_finite())
            || !(0.0..=1.0).contains(&self.confidence_score)
            || self.diagnostics.resolver.is_empty()
            || self.diagnostics.warnings.len() > 64
        {
            return Err(StyleError::InvalidPrediction(
                "Prediction contains invalid identity, confidence, or numeric output".into(),
            ));
        }
        Ok(())
    }
}

/// Replaceable execution boundary. Linear v1 is local today; future ONNX or
/// accelerator implementations can satisfy the same interface.
pub trait StyleResolver: Send + Sync {
    fn backend_id(&self) -> &str;
    fn resolve(
        &self,
        package: &LoadedStylePackage,
        features: &StyleFeatureVector,
    ) -> Result<StylePrediction, StyleError>;
}
