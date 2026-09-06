use photo_contracts::trained_style::*;

pub const LINEAR_RESOLVER_VERSION: &str = "photo-editor-linear-style-v1";

#[derive(Default)]
pub struct LinearStyleResolver;

fn assign(adjustments: &mut PredictedCreativeAdjustments, control: StyleControl, value: f32) {
    match control {
        StyleControl::ExposureEv => adjustments.exposure_ev = value,
        StyleControl::TemperatureDelta => adjustments.temperature_delta = value,
        StyleControl::Tint => adjustments.tint = value,
        StyleControl::Contrast => adjustments.contrast = value,
        StyleControl::Highlights => adjustments.highlights = value,
        StyleControl::Shadows => adjustments.shadows = value,
        StyleControl::Whites => adjustments.whites = value,
        StyleControl::Blacks => adjustments.blacks = value,
        StyleControl::Saturation => adjustments.saturation = value,
        StyleControl::Vibrance => adjustments.vibrance = value,
        StyleControl::Texture => adjustments.texture = value,
        StyleControl::Clarity => adjustments.clarity = value,
        StyleControl::Dehaze => adjustments.dehaze = value,
        StyleControl::SharpeningAmount => adjustments.sharpening_amount = value,
        StyleControl::NoiseReduction => adjustments.noise_reduction = value,
        StyleControl::VignetteAmount => adjustments.vignette_amount = value,
    }
}

impl StyleResolver for LinearStyleResolver {
    fn backend_id(&self) -> &str {
        LINEAR_RESOLVER_VERSION
    }

    fn resolve(
        &self,
        package: &LoadedStylePackage,
        features: &StyleFeatureVector,
    ) -> Result<StylePrediction, StyleError> {
        super::package::validate_loaded_package(package)?;
        features.validate(&super::features::STYLE_FEATURE_NAMES)?;
        if features.schema_version != package.manifest.feature_schema {
            return Err(StyleError::IncompatibleFeatureSchema(
                features.schema_version.clone(),
            ));
        }
        let StyleModelKind::LinearV1(model) = &package.model.model;
        let mut adjustments = PredictedCreativeAdjustments::default();
        let mut bounded_controls = Vec::new();
        for output in &model.outputs {
            let mut value = output.intercept;
            for (index, feature) in features.values.iter().enumerate() {
                value += if features.available[index] {
                    feature * output.weights[index]
                } else {
                    output.missing_weights[index]
                };
            }
            if !value.is_finite() {
                return Err(StyleError::InvalidPrediction(format!(
                    "Non-finite {:?} model output",
                    output.control
                )));
            }
            let bound = package
                .rules
                .output_bounds
                .get(&output.control)
                .ok_or_else(|| {
                    StyleError::InvalidPrediction(format!(
                        "Missing {:?} output bound",
                        output.control
                    ))
                })?;
            let bounded = value.clamp(bound.minimum, bound.maximum);
            if bounded != value {
                bounded_controls.push(output.control);
            }
            assign(&mut adjustments, output.control, bounded);
        }
        let available_fraction = features.available.iter().filter(|value| **value).count() as f32
            / features.available.len() as f32;
        let confidence_score =
            (model.base_confidence * (0.55 + available_fraction * 0.45)).clamp(0.0, 1.0);
        let confidence = if available_fraction < 0.45 {
            StyleConfidence::InsufficientEvidence
        } else if confidence_score >= 0.78 {
            StyleConfidence::High
        } else if confidence_score >= 0.58 {
            StyleConfidence::Medium
        } else {
            StyleConfidence::Low
        };
        let prediction = StylePrediction {
            style_id: package.manifest.style_id.clone(),
            style_version: package.manifest.version.clone(),
            model_version: package.manifest.model_version.clone(),
            package_identity: package.integrity.package_identity.clone(),
            feature_schema: package.manifest.feature_schema.clone(),
            adjustments,
            confidence,
            confidence_score,
            diagnostics: StylePredictionDiagnostics {
                resolver: self.backend_id().into(),
                missing_feature_count: features.missing_features.len() as u32,
                bounded_controls,
                warnings: if features.missing_features.is_empty() {
                    Vec::new()
                } else {
                    vec![format!(
                        "{} unavailable source/context features used explicit missingness weights",
                        features.missing_features.len()
                    )]
                },
            },
        };
        prediction.validate()?;
        Ok(prediction)
    }
}
