use super::{target::TARGET_CONTROLS, trainer::TrainedModelArtifact};
use crate::{rendering::RENDERER_VERSION, trained_styles::package::load_style_package};
use photo_contracts::{
    trained_style::{
        LoadedStylePackage, PackageFileReference, RendererCompatibility, StyleMetadata,
        StyleOutputBound, StylePackageIntegrity, StyleRules, StyleSignatureMetadata, TrainedStyle,
        TrainingPackageProvenance, STYLE_INTEGRITY_SCHEMA_VERSION, STYLE_METADATA_SCHEMA_VERSION,
        STYLE_PACKAGE_SCHEMA_VERSION, STYLE_RULES_SCHEMA_VERSION, TRAINED_STYLE_SCHEMA_VERSION,
    },
    training::{TrainingDataset, TrainingMetrics, TRAINER_VERSION},
    CancellationToken, RECIPE_SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct StyleVersionIdentity {
    pub style_id: String,
    pub display_name: String,
    pub version: String,
    pub version_number: u32,
    pub model_version: String,
}

pub fn next_style_identity(
    root: &Path,
    name: &str,
    dataset_identity: &str,
) -> StyleVersionIdentity {
    let slug = slug(name);
    let prefix = format!("{slug}-v");
    let maximum = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter_map(|value| value.strip_prefix(&prefix)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    let version_number = maximum + 1;
    let style_id = format!("{slug}-v{version_number}");
    StyleVersionIdentity {
        display_name: format!("{} v{version_number}", name.trim()),
        version: format!("{version_number}.0.0"),
        model_version: format!(
            "{style_id}-{}-{}",
            TRAINER_VERSION,
            dataset_identity.chars().take(12).collect::<String>()
        ),
        style_id,
        version_number,
    }
}

pub fn export_style_package(
    root: &Path,
    identity: &StyleVersionIdentity,
    dataset: &TrainingDataset,
    artifact: &TrainedModelArtifact,
    cancel: &CancellationToken,
) -> Result<(PathBuf, LoadedStylePackage), String> {
    cancel.check().map_err(|error| error.message)?;
    fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let destination = root.join(&identity.style_id);
    if destination.exists() {
        return Err("A trained style with this version already exists".into());
    }
    let mut model = artifact.model.clone();
    model.model_version = identity.model_version.clone();
    let manifest = TrainedStyle {
        schema_version: TRAINED_STYLE_SCHEMA_VERSION,
        package_schema_version: STYLE_PACKAGE_SCHEMA_VERSION,
        style_id: identity.style_id.clone(),
        name: identity.display_name.clone(),
        version: identity.version.clone(),
        photo_type: dataset.photo_type,
        model_version: identity.model_version.clone(),
        feature_schema: dataset.feature_schema.clone(),
        renderer_compatibility: RendererCompatibility {
            recipe_schema_versions: vec![RECIPE_SCHEMA_VERSION],
            minimum_renderer_version: RENDERER_VERSION.into(),
        },
        supported_controls: TARGET_CONTROLS.to_vec(),
        model: PackageFileReference {
            path: "model.json".into(),
            format: "linear_json_v1".into(),
        },
        rules: PackageFileReference {
            path: "rules.json".into(),
            format: "style_rules_v1".into(),
        },
        metadata: PackageFileReference {
            path: "metadata.json".into(),
            format: "style_metadata_v1".into(),
        },
        integrity: PackageFileReference {
            path: "checksums.json".into(),
            format: "sha256_canonical_json_v1".into(),
        },
    };
    let rules = StyleRules {
        schema_version: STYLE_RULES_SCHEMA_VERSION,
        output_bounds: TARGET_CONTROLS
            .iter()
            .map(|control| (*control, training_bound(*control)))
            .collect(),
    };
    let trained_at = chrono::Utc::now().to_rfc3339();
    let metadata = StyleMetadata {
        schema_version: STYLE_METADATA_SCHEMA_VERSION,
        description: format!(
            "Locally trained recipe-control style from {} before/after pairs.",
            dataset.pairs.len()
        ),
        author: "PhotoEditor Training Studio".into(),
        created_at: trained_at.clone(),
        development_only: false,
        trained_from_user_photos: true,
        training_provenance: "Local supervised recipe-control training; source/reference paths are intentionally omitted from this package.".into(),
        training: Some(TrainingPackageProvenance {
            dataset_identity: dataset.dataset_fingerprint.clone().ok_or_else(|| {
                "Validated dataset is missing its stable fingerprint".to_string()
            })?,
            training_pairs: artifact.train_pair_ids.len() as u32,
            validation_pairs: artifact.validation_pair_ids.len() as u32,
            feature_schema: dataset.feature_schema.clone(),
            target_recipe_schema: dataset.target_recipe_schema,
            trainer_version: TRAINER_VERSION.into(),
            renderer_version: dataset.renderer_version.clone(),
            trained_at,
            metrics_summary: metrics_summary(&artifact.metrics),
        }),
    };
    let temp = tempfile::Builder::new()
        .prefix(".training-style-")
        .tempdir_in(root)
        .map_err(|error| error.to_string())?;
    let style_digest = write_json(&temp.path().join("style.json"), &manifest)?;
    let model_digest = write_json(&temp.path().join("model.json"), &model)?;
    let rules_digest = write_json(&temp.path().join("rules.json"), &rules)?;
    let metadata_digest = write_json(&temp.path().join("metadata.json"), &metadata)?;
    let files = BTreeMap::from([
        ("metadata.json".into(), metadata_digest),
        ("model.json".into(), model_digest),
        ("rules.json".into(), rules_digest),
        ("style.json".into(), style_digest),
    ]);
    let integrity = StylePackageIntegrity {
        schema_version: STYLE_INTEGRITY_SCHEMA_VERSION,
        algorithm: "sha256".into(),
        package_identity: package_identity(&files),
        files,
        signature: StyleSignatureMetadata {
            scheme: "unsigned_sha256_v1".into(),
            signer: None,
            value: None,
        },
    };
    write_json(&temp.path().join("checksums.json"), &integrity)?;
    let package = load_style_package(temp.path()).map_err(|error| error.to_string())?;
    cancel.check().map_err(|error| error.message)?;
    let pending = temp.keep();
    fs::rename(&pending, &destination).map_err(|error| error.to_string())?;
    Ok((destination, package))
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut separator = false;
    for character in value.trim().to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character);
            separator = false;
        } else if !separator && !result.is_empty() {
            result.push('-');
            separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "trained-style".into()
    } else {
        result.chars().take(80).collect()
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<String, String> {
    let canonical_value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    let canonical = serde_json::to_vec(&canonical_value).map_err(|error| error.to_string())?;
    let pretty = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, pretty).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn package_identity(files: &BTreeMap<String, String>) -> String {
    let mut hash = Sha256::new();
    for (name, identity) in files {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name);
        hash.update((identity.len() as u64).to_le_bytes());
        hash.update(identity);
    }
    format!("{:x}", hash.finalize())
}

fn training_bound(control: photo_contracts::trained_style::StyleControl) -> StyleOutputBound {
    use photo_contracts::trained_style::StyleControl;
    match control {
        StyleControl::ExposureEv => StyleOutputBound {
            minimum: -3.0,
            maximum: 3.0,
        },
        StyleControl::TemperatureDelta => StyleOutputBound {
            minimum: -3000.0,
            maximum: 3000.0,
        },
        StyleControl::Tint => StyleOutputBound {
            minimum: -50.0,
            maximum: 50.0,
        },
        StyleControl::Saturation | StyleControl::Vibrance => StyleOutputBound {
            minimum: -50.0,
            maximum: 50.0,
        },
        StyleControl::Clarity => StyleOutputBound {
            minimum: -40.0,
            maximum: 40.0,
        },
        _ => StyleOutputBound {
            minimum: -80.0,
            maximum: 80.0,
        },
    }
}

fn metrics_summary(metrics: &TrainingMetrics) -> BTreeMap<String, f32> {
    let mut result = BTreeMap::from([
        (
            "train_recipe_mae_normalized".into(),
            metrics.train.mean_recipe_mae,
        ),
        (
            "validation_recipe_mae_normalized".into(),
            metrics.validation.mean_recipe_mae,
        ),
        (
            "mean_baseline_recipe_mae_normalized".into(),
            metrics.mean_baseline.mean_recipe_mae,
        ),
    ]);
    for (name, value) in [
        ("train_rendered_loss", metrics.train.rendered_loss),
        ("validation_rendered_loss", metrics.validation.rendered_loss),
        (
            "neutral_baseline_rendered_loss",
            metrics.neutral_baseline.rendered_loss,
        ),
        (
            "mean_baseline_rendered_loss",
            metrics.mean_baseline.rendered_loss,
        ),
    ] {
        if let Some(value) = value {
            result.insert(name.into(), value);
        }
    }
    result
}
