use super::features::STYLE_FEATURE_NAMES;
use photo_contracts::trained_style::*;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn canonical<T: DeserializeOwned + serde::Serialize>(
    text: &str,
) -> Result<(T, String), StyleError> {
    if text.len() > MAX_STYLE_PACKAGE_FILE_BYTES {
        return Err(StyleError::CorruptPackage(
            "Style package file exceeds 1 MiB".into(),
        ));
    }
    let parsed = serde_json::from_str::<T>(text)
        .map_err(|error| StyleError::CorruptPackage(error.to_string()))?;
    let value = serde_json::to_value(&parsed)
        .map_err(|error| StyleError::CorruptPackage(error.to_string()))?;
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| StyleError::CorruptPackage(error.to_string()))?;
    Ok((parsed, format!("{:x}", Sha256::digest(bytes))))
}

fn read(path: &Path) -> Result<String, StyleError> {
    let metadata = fs::metadata(path)
        .map_err(|error| StyleError::CorruptPackage(format!("{}: {error}", path.display())))?;
    if metadata.len() > MAX_STYLE_PACKAGE_FILE_BYTES as u64 {
        return Err(StyleError::CorruptPackage(format!(
            "{} exceeds 1 MiB",
            path.display()
        )));
    }
    fs::read_to_string(path)
        .map_err(|error| StyleError::CorruptPackage(format!("{}: {error}", path.display())))
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

fn legal_bound(control: StyleControl) -> StyleOutputBound {
    match control {
        StyleControl::ExposureEv => StyleOutputBound {
            minimum: -5.0,
            maximum: 5.0,
        },
        StyleControl::TemperatureDelta => StyleOutputBound {
            minimum: -4500.0,
            maximum: 5500.0,
        },
        StyleControl::SharpeningAmount | StyleControl::NoiseReduction => StyleOutputBound {
            minimum: 0.0,
            maximum: 100.0,
        },
        _ => StyleOutputBound {
            minimum: -100.0,
            maximum: 100.0,
        },
    }
}

pub fn validate_loaded_package(package: &LoadedStylePackage) -> Result<(), StyleError> {
    package.manifest.validate()?;
    package.model.validate(&STYLE_FEATURE_NAMES)?;
    package
        .rules
        .validate(&package.manifest.supported_controls)?;
    package.metadata.validate()?;
    package.integrity.validate()?;
    if package.model.feature_schema != package.manifest.feature_schema
        || package.model.model_version != package.manifest.model_version
        || package
            .manifest
            .renderer_compatibility
            .minimum_renderer_version
            != crate::rendering::RENDERER_VERSION
    {
        return Err(StyleError::CorruptPackage(
            "Manifest model or renderer compatibility does not agree with this runtime".into(),
        ));
    }
    let StyleModelKind::LinearV1(model) = &package.model.model;
    let model_controls = model
        .outputs
        .iter()
        .map(|output| output.control)
        .collect::<std::collections::BTreeSet<_>>();
    let manifest_controls = package
        .manifest
        .supported_controls
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if model_controls != manifest_controls {
        return Err(StyleError::CorruptPackage(
            "Model outputs do not match the manifest controls".into(),
        ));
    }
    for (control, bound) in &package.rules.output_bounds {
        let legal = legal_bound(*control);
        if bound.minimum < legal.minimum || bound.maximum > legal.maximum {
            return Err(StyleError::CorruptPackage(format!(
                "Bounds for {control:?} exceed renderer limits"
            )));
        }
    }
    Ok(())
}

pub fn load_style_package(directory: &Path) -> Result<LoadedStylePackage, StyleError> {
    if !directory.is_dir() {
        return Err(StyleError::CorruptPackage(format!(
            "Style package is not a directory: {}",
            directory.display()
        )));
    }
    let style_text = read(&directory.join("style.json"))?;
    let model_text = read(&directory.join("model.json"))?;
    let rules_text = read(&directory.join("rules.json"))?;
    let metadata_text = read(&directory.join("metadata.json"))?;
    let integrity_text = read(&directory.join("checksums.json"))?;
    let (manifest, style_digest) = canonical::<TrainedStyle>(&style_text)?;
    let (model, model_digest) = canonical::<StyleModel>(&model_text)?;
    let (rules, rules_digest) = canonical::<StyleRules>(&rules_text)?;
    let (metadata, metadata_digest) = canonical::<StyleMetadata>(&metadata_text)?;
    let (integrity, _) = canonical::<StylePackageIntegrity>(&integrity_text)?;
    let actual = BTreeMap::from([
        ("metadata.json".into(), metadata_digest),
        ("model.json".into(), model_digest),
        ("rules.json".into(), rules_digest),
        ("style.json".into(), style_digest),
    ]);
    if integrity.files != actual || integrity.package_identity != package_identity(&actual) {
        return Err(StyleError::CorruptPackage(
            "Canonical package checksum mismatch".into(),
        ));
    }
    let package = LoadedStylePackage {
        manifest,
        model,
        rules,
        metadata,
        integrity,
    };
    validate_loaded_package(&package)?;
    Ok(package)
}

#[derive(Clone)]
pub struct LocalStyleCatalog {
    styles: BTreeMap<String, LoadedStylePackage>,
}

impl LocalStyleCatalog {
    pub fn load(root: &Path) -> Result<Self, StyleError> {
        let mut directories = fs::read_dir(root)
            .map_err(|error| StyleError::CorruptPackage(format!("{}: {error}", root.display())))?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        directories.sort();
        let mut styles = BTreeMap::new();
        for directory in directories {
            let package = load_style_package(&directory)?;
            let id = package.manifest.style_id.clone();
            if styles.insert(id.clone(), package).is_some() {
                return Err(StyleError::CorruptPackage(format!(
                    "Duplicate style ID: {id}"
                )));
            }
        }
        if styles.is_empty() {
            return Err(StyleError::CorruptPackage(
                "No style packages were found".into(),
            ));
        }
        Ok(Self { styles })
    }

    pub fn get(&self, id: &str) -> Option<&LoadedStylePackage> {
        self.styles.get(id)
    }

    pub fn packages(&self) -> impl Iterator<Item = &LoadedStylePackage> {
        self.styles.values()
    }
}
