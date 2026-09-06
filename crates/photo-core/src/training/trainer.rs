use super::target::{control_value, TARGET_CONTROLS};
use photo_contracts::{
    trained_style::{
        LinearOutput, LinearStyleModel, PredictedCreativeAdjustments, StyleControl,
        StyleFeatureVector, StyleModel, StyleModelKind, STYLE_FEATURE_SCHEMA_V1,
        STYLE_MODEL_SCHEMA_VERSION,
    },
    training::{
        MetricSet, TargetFitConfidence, TrainingConfig, TrainingDataset, TrainingMetrics,
        TrainingSplit, TRAINER_VERSION,
    },
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct TrainingExample {
    pub pair_id: String,
    pub features: StyleFeatureVector,
    pub target: PredictedCreativeAdjustments,
    pub confidence: TargetFitConfidence,
    pub split: TrainingSplit,
}

#[derive(Clone)]
pub struct TrainedModelArtifact {
    pub model: StyleModel,
    pub metrics: TrainingMetrics,
    pub mean_controls: PredictedCreativeAdjustments,
    pub train_pair_ids: Vec<String>,
    pub validation_pair_ids: Vec<String>,
}

pub trait StyleModelTrainer: Send + Sync {
    fn version(&self) -> &str;
    fn train(
        &self,
        examples: &[TrainingExample],
        config: &TrainingConfig,
        model_version: &str,
    ) -> Result<TrainedModelArtifact, String>;
}

#[derive(Default)]
pub struct RegularizedLinearTrainer;

pub fn assign_splits(dataset: &mut TrainingDataset, config: &TrainingConfig) -> Result<(), String> {
    config.validate()?;
    let eligible = dataset
        .pairs
        .iter()
        .filter(|pair| {
            !pair.excluded
                && pair.target.as_ref().is_some_and(|target| {
                    !config.exclude_low_confidence || target.confidence != TargetFitConfidence::Low
                })
        })
        .map(|pair| pair.pair_id.clone())
        .collect::<BTreeSet<_>>();
    if eligible.is_empty() {
        return Err("No usable target recipes are available for training".into());
    }
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for pair in dataset
        .pairs
        .iter()
        .filter(|pair| eligible.contains(&pair.pair_id))
    {
        groups
            .entry(
                pair.scene_group_id
                    .clone()
                    .unwrap_or_else(|| format!("pair:{}", pair.pair_id)),
            )
            .or_default()
            .push(pair.pair_id.clone());
    }
    let mut ordered = groups.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(group, _)| stable_score(&dataset.dataset_id, group));
    let target_validation = if eligible.len() >= 2 {
        ((eligible.len() * config.validation_percent as usize).div_ceil(100)).max(1)
    } else {
        0
    };
    let mut validation = BTreeSet::new();
    if ordered.len() >= 2 {
        for (_, pair_ids) in ordered.iter().take(ordered.len() - 1) {
            if validation.len() >= target_validation {
                break;
            }
            validation.extend(pair_ids.iter().cloned());
        }
    }
    for pair in &mut dataset.pairs {
        pair.split = if !eligible.contains(&pair.pair_id) {
            TrainingSplit::Excluded
        } else if validation.contains(&pair.pair_id) {
            TrainingSplit::Validation
        } else {
            TrainingSplit::Train
        };
    }
    if eligible.len() >= 2 && validation.is_empty() {
        dataset.warnings.push(
            "All usable examples belong to one scene group; a leakage-safe validation split was not possible"
                .into(),
        );
    }
    Ok(())
}

impl StyleModelTrainer for RegularizedLinearTrainer {
    fn version(&self) -> &str {
        TRAINER_VERSION
    }

    fn train(
        &self,
        examples: &[TrainingExample],
        config: &TrainingConfig,
        model_version: &str,
    ) -> Result<TrainedModelArtifact, String> {
        config.validate()?;
        let train = examples
            .iter()
            .filter(|example| example.split == TrainingSplit::Train)
            .collect::<Vec<_>>();
        let validation = examples
            .iter()
            .filter(|example| example.split == TrainingSplit::Validation)
            .collect::<Vec<_>>();
        if train.is_empty() {
            return Err("The deterministic split left no training examples".into());
        }
        let feature_count = train[0].features.values.len();
        if feature_count == 0
            || examples.iter().any(|example| {
                example.features.schema_version != STYLE_FEATURE_SCHEMA_V1
                    || example.features.values.len() != feature_count
                    || example.features.available.len() != feature_count
            })
        {
            return Err("Training examples use incompatible feature schemas".into());
        }
        let (means, scales) = normalization(&train, feature_count);
        let mean_controls = mean_target(&train);
        let mut outputs = Vec::new();
        for control in TARGET_CONTROLS {
            let mut intercept = control_value(mean_controls, control);
            let mut weights = vec![0.0f32; feature_count];
            for epoch in 0..config.epochs {
                let mut intercept_gradient = 0.0f32;
                let mut gradients = vec![0.0f32; feature_count];
                let mut total_weight = 0.0f32;
                for example in &train {
                    let sample_weight = confidence_weight(example.confidence);
                    let normalized = normalized_values(&example.features, &means, &scales);
                    let predicted = intercept
                        + weights
                            .iter()
                            .zip(&normalized)
                            .map(|(weight, value)| weight * value)
                            .sum::<f32>();
                    let error = predicted - control_value(example.target, control);
                    intercept_gradient += sample_weight * error;
                    for index in 0..feature_count {
                        gradients[index] += sample_weight * error * normalized[index];
                    }
                    total_weight += sample_weight;
                }
                let rate = config.learning_rate / (1.0 + epoch as f32 / 800.0);
                intercept -= rate * 2.0 * intercept_gradient / total_weight.max(1e-6);
                for index in 0..feature_count {
                    let gradient = 2.0 * gradients[index] / total_weight.max(1e-6)
                        + 2.0 * config.regularization * weights[index];
                    weights[index] = (weights[index] - rate * gradient).clamp(-5000.0, 5000.0);
                }
                if !intercept.is_finite() || weights.iter().any(|weight| !weight.is_finite()) {
                    return Err("Linear training became non-finite".into());
                }
            }
            outputs.push(LinearOutput {
                control,
                intercept,
                weights,
                missing_weights: vec![0.0; feature_count],
            });
        }
        let base_confidence = train
            .iter()
            .map(|example| confidence_weight(example.confidence))
            .sum::<f32>()
            / train.len() as f32;
        let model = StyleModel {
            schema_version: STYLE_MODEL_SCHEMA_VERSION,
            feature_schema: STYLE_FEATURE_SCHEMA_V1.into(),
            model_version: model_version.into(),
            model: StyleModelKind::LinearV1(LinearStyleModel {
                feature_names: train[0].features.feature_names.clone(),
                feature_means: means,
                feature_scales: scales,
                outputs,
                base_confidence: (0.55 + base_confidence * 0.35).clamp(0.55, 0.9),
            }),
        };
        let train_metrics = recipe_metrics(&model, &train);
        let validation_metrics = recipe_metrics(&model, &validation);
        let mean_metrics = baseline_metrics(&validation, mean_controls);
        let neutral_metrics =
            baseline_metrics(&validation, PredictedCreativeAdjustments::default());
        let overfitting_warning = if validation_metrics.mean_recipe_mae > 0.0
            && validation_metrics.mean_recipe_mae > train_metrics.mean_recipe_mae * 1.8 + 0.05
        {
            Some("Validation recipe error is substantially higher than training error".into())
        } else {
            None
        };
        Ok(TrainedModelArtifact {
            model,
            metrics: TrainingMetrics {
                train: train_metrics,
                validation: validation_metrics,
                neutral_baseline: neutral_metrics,
                mean_baseline: mean_metrics,
                beats_mean_baseline: false,
                overfitting_warning,
                warnings: Vec::new(),
            },
            mean_controls,
            train_pair_ids: train
                .iter()
                .map(|example| example.pair_id.clone())
                .collect(),
            validation_pair_ids: validation
                .iter()
                .map(|example| example.pair_id.clone())
                .collect(),
        })
    }
}

pub fn predict_controls(
    model: &StyleModel,
    features: &StyleFeatureVector,
) -> PredictedCreativeAdjustments {
    let StyleModelKind::LinearV1(linear) = &model.model;
    let normalized = normalized_values(features, &linear.feature_means, &linear.feature_scales);
    let mut controls = PredictedCreativeAdjustments::default();
    for output in &linear.outputs {
        let value = output.intercept
            + output
                .weights
                .iter()
                .zip(&normalized)
                .map(|(weight, value)| weight * value)
                .sum::<f32>();
        assign(
            &mut controls,
            output.control,
            super::target::bounded(output.control, value),
        );
    }
    controls
}

fn normalization(train: &[&TrainingExample], count: usize) -> (Vec<f32>, Vec<f32>) {
    let mut means = vec![0.0f32; count];
    let mut weights = vec![0.0f32; count];
    for example in train {
        let sample_weight = confidence_weight(example.confidence);
        for index in 0..count {
            if example.features.available[index] {
                means[index] += example.features.values[index] * sample_weight;
                weights[index] += sample_weight;
            }
        }
    }
    for index in 0..count {
        means[index] /= weights[index].max(1e-6);
    }
    let mut scales = vec![0.0f32; count];
    for example in train {
        let sample_weight = confidence_weight(example.confidence);
        for index in 0..count {
            if example.features.available[index] {
                scales[index] +=
                    (example.features.values[index] - means[index]).powi(2) * sample_weight;
            }
        }
    }
    for index in 0..count {
        scales[index] = (scales[index] / weights[index].max(1e-6)).sqrt().max(1e-3);
    }
    (means, scales)
}

fn normalized_values(features: &StyleFeatureVector, means: &[f32], scales: &[f32]) -> Vec<f32> {
    features
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            if features.available[index] {
                (*value - means[index]) / scales[index]
            } else {
                0.0
            }
        })
        .collect()
}

fn confidence_weight(confidence: TargetFitConfidence) -> f32 {
    match confidence {
        TargetFitConfidence::High => 1.0,
        TargetFitConfidence::Medium => 0.65,
        TargetFitConfidence::Low => 0.2,
    }
}

fn mean_target(examples: &[&TrainingExample]) -> PredictedCreativeAdjustments {
    let mut result = PredictedCreativeAdjustments::default();
    let total = examples
        .iter()
        .map(|example| confidence_weight(example.confidence))
        .sum::<f32>()
        .max(1e-6);
    for control in TARGET_CONTROLS {
        let mean = examples
            .iter()
            .map(|example| {
                control_value(example.target, control) * confidence_weight(example.confidence)
            })
            .sum::<f32>()
            / total;
        assign(&mut result, control, mean);
    }
    result
}

fn recipe_metrics(model: &StyleModel, examples: &[&TrainingExample]) -> MetricSet {
    if examples.is_empty() {
        return MetricSet::default();
    }
    let mut recipe_mae = BTreeMap::new();
    for control in TARGET_CONTROLS {
        let value = examples
            .iter()
            .map(|example| {
                (control_value(predict_controls(model, &example.features), control)
                    - control_value(example.target, control))
                .abs()
            })
            .sum::<f32>()
            / examples.len() as f32;
        recipe_mae.insert(control, value);
    }
    MetricSet {
        mean_recipe_mae: normalized_mae(&recipe_mae),
        recipe_mae,
        rendered_loss: None,
    }
}

fn baseline_metrics(
    examples: &[&TrainingExample],
    baseline: PredictedCreativeAdjustments,
) -> MetricSet {
    if examples.is_empty() {
        return MetricSet::default();
    }
    let recipe_mae = TARGET_CONTROLS
        .iter()
        .map(|control| {
            (
                *control,
                examples
                    .iter()
                    .map(|example| {
                        (control_value(baseline, *control)
                            - control_value(example.target, *control))
                        .abs()
                    })
                    .sum::<f32>()
                    / examples.len() as f32,
            )
        })
        .collect::<BTreeMap<_, _>>();
    MetricSet {
        mean_recipe_mae: normalized_mae(&recipe_mae),
        recipe_mae,
        rendered_loss: None,
    }
}

fn normalized_mae(values: &BTreeMap<StyleControl, f32>) -> f32 {
    values
        .iter()
        .map(|(control, value)| value / scale(*control))
        .sum::<f32>()
        / values.len().max(1) as f32
}

fn scale(control: StyleControl) -> f32 {
    match control {
        StyleControl::ExposureEv => 3.0,
        StyleControl::TemperatureDelta => 3000.0,
        StyleControl::Tint => 50.0,
        StyleControl::Saturation | StyleControl::Vibrance => 50.0,
        StyleControl::Clarity => 40.0,
        _ => 80.0,
    }
}

fn assign(value: &mut PredictedCreativeAdjustments, control: StyleControl, next: f32) {
    match control {
        StyleControl::ExposureEv => value.exposure_ev = next,
        StyleControl::TemperatureDelta => value.temperature_delta = next,
        StyleControl::Tint => value.tint = next,
        StyleControl::Highlights => value.highlights = next,
        StyleControl::Shadows => value.shadows = next,
        StyleControl::Whites => value.whites = next,
        StyleControl::Blacks => value.blacks = next,
        StyleControl::Saturation => value.saturation = next,
        StyleControl::Vibrance => value.vibrance = next,
        StyleControl::Clarity => value.clarity = next,
        _ => {}
    }
}

fn stable_score(dataset: &str, group: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(dataset);
    hash.update([0]);
    hash.update(group);
    hash.finalize().into()
}
