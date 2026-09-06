use crate::rendering::{
    internal,
    pixels::{self, luma, FloatImage},
    tools, CpuProcessingEngine, RENDERER_VERSION,
};
use photo_contracts::{
    trained_style::{PredictedCreativeAdjustments, StyleControl},
    training::{
        GeometryRelationship, PairValidation, PairValidationStatus, TargetFitConfidence,
        TargetLossBreakdown, TargetRecipeResult, TrainingPair, TARGET_OPTIMIZER_VERSION,
        TARGET_RECIPE_SCHEMA_VERSION,
    },
    CancellationToken, EditRecipe, Presence, ProcessingResult, RenderAdjustments,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const TARGET_PROXY_EDGE: u32 = 1600;
pub const TARGET_CONTROLS: [StyleControl; 10] = [
    StyleControl::ExposureEv,
    StyleControl::TemperatureDelta,
    StyleControl::Tint,
    StyleControl::Highlights,
    StyleControl::Shadows,
    StyleControl::Whites,
    StyleControl::Blacks,
    StyleControl::Saturation,
    StyleControl::Vibrance,
    StyleControl::Clarity,
];

pub trait TargetRecipeOptimizer: Send + Sync {
    fn version(&self) -> &str;
    fn validate_pair(
        &self,
        pair: &TrainingPair,
        cancel: &CancellationToken,
    ) -> ProcessingResult<PairValidation>;
    fn estimate(
        &self,
        pair: &TrainingPair,
        cancel: &CancellationToken,
    ) -> ProcessingResult<TargetRecipeResult>;
    fn rendered_loss(
        &self,
        pair: &TrainingPair,
        controls: PredictedCreativeAdjustments,
        cancel: &CancellationToken,
    ) -> ProcessingResult<f32>;
}

pub struct StagedTargetOptimizer {
    engine: Arc<CpuProcessingEngine>,
    proxy_edge: u32,
}

#[derive(Clone)]
struct Comparison {
    source: FloatImage,
    reference: FloatImage,
}

#[derive(Clone)]
struct Stats {
    q10: f32,
    q50: f32,
    q90: f32,
    mean: [f32; 3],
    saturation: f32,
    structure: Vec<f32>,
}

impl StagedTargetOptimizer {
    pub fn new(engine: Arc<CpuProcessingEngine>) -> Self {
        Self {
            engine,
            proxy_edge: TARGET_PROXY_EDGE,
        }
    }

    pub fn with_proxy_edge(engine: Arc<CpuProcessingEngine>, proxy_edge: u32) -> Self {
        Self {
            engine,
            proxy_edge: proxy_edge.clamp(64, TARGET_PROXY_EDGE),
        }
    }

    fn comparison(
        &self,
        pair: &TrainingPair,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Comparison> {
        cancel.check()?;
        let source = self
            .engine
            .analysis_input(&pair.source_path, cancel)?
            .image
            .reduced(self.proxy_edge, cancel)?;
        let reference = self
            .engine
            .analysis_input(&pair.reference_path, cancel)?
            .image
            .reduced(self.proxy_edge, cancel)?;
        if source.width < 16 || source.height < 16 || reference.width < 16 || reference.height < 16
        {
            return Err(internal("Training pair is too small for target estimation"));
        }
        Ok(Comparison { source, reference })
    }

    fn loss(
        comparison: &Comparison,
        controls: PredictedCreativeAdjustments,
        cancel: &CancellationToken,
    ) -> ProcessingResult<TargetLossBreakdown> {
        let mut candidate = comparison.source.clone();
        pixels::apply(&mut candidate, &render_adjustments(controls), cancel)?;
        tools::presence(
            &mut candidate,
            Presence {
                clarity: controls.clarity,
                ..Default::default()
            },
            cancel,
        )?;
        let candidate = stats(&candidate);
        let reference = stats(&comparison.reference);
        let luminance = ((candidate.q10 - reference.q10).abs()
            + (candidate.q50 - reference.q50).abs() * 1.5
            + (candidate.q90 - reference.q90).abs())
            / 3.5;
        let candidate_sum = candidate.mean.iter().sum::<f32>().max(1e-6);
        let reference_sum = reference.mean.iter().sum::<f32>().max(1e-6);
        let color_balance = (0..3)
            .map(|index| {
                (candidate.mean[index] / candidate_sum - reference.mean[index] / reference_sum)
                    .abs()
            })
            .sum::<f32>()
            / 3.0;
        let saturation = (candidate.saturation - reference.saturation).abs();
        let structure = 1.0 - correlation(&candidate.structure, &reference.structure);
        let total = 0.48 * luminance + 0.22 * color_balance + 0.12 * saturation + 0.18 * structure;
        Ok(TargetLossBreakdown {
            total,
            luminance,
            color_balance,
            saturation,
            structure,
        })
    }
}

impl TargetRecipeOptimizer for StagedTargetOptimizer {
    fn version(&self) -> &str {
        TARGET_OPTIMIZER_VERSION
    }

    fn validate_pair(
        &self,
        pair: &TrainingPair,
        cancel: &CancellationToken,
    ) -> ProcessingResult<PairValidation> {
        let comparison = match self.comparison(pair, cancel) {
            Ok(value) => value,
            Err(error) => {
                return Ok(PairValidation {
                    status: PairValidationStatus::Unusable,
                    geometry: GeometryRelationship::Unusable,
                    diagnostics: vec![error.message],
                    ..Default::default()
                })
            }
        };
        let source_aspect = comparison.source.width as f32 / comparison.source.height as f32;
        let reference_aspect =
            comparison.reference.width as f32 / comparison.reference.height as f32;
        let aspect_delta = ((source_aspect / reference_aspect).ln()).abs();
        let geometry = if aspect_delta <= 0.025 {
            GeometryRelationship::ExactOrNear
        } else if aspect_delta <= 0.35 {
            GeometryRelationship::CropDifference
        } else {
            GeometryRelationship::MajorMismatch
        };
        let similarity = correlation(
            &stats(&comparison.source).structure,
            &stats(&comparison.reference).structure,
        );
        let status = if geometry == GeometryRelationship::MajorMismatch || similarity < 0.22 {
            PairValidationStatus::Rejected
        } else if geometry == GeometryRelationship::CropDifference || similarity < 0.52 {
            PairValidationStatus::NeedsReview
        } else {
            PairValidationStatus::Ready
        };
        let mut diagnostics = Vec::new();
        if geometry == GeometryRelationship::CropDifference {
            diagnostics.push(
                "Reference crop differs; target fitting uses the common centered region".into(),
            );
        }
        if similarity < 0.52 {
            diagnostics.push("Structural match is uncertain; confirm that source and reference show the same photograph".into());
        }
        Ok(PairValidation {
            status,
            geometry,
            structural_similarity: Some(similarity),
            source_width: Some(comparison.source.width),
            source_height: Some(comparison.source.height),
            reference_width: Some(comparison.reference.width),
            reference_height: Some(comparison.reference.height),
            diagnostics,
        })
    }

    fn estimate(
        &self,
        pair: &TrainingPair,
        cancel: &CancellationToken,
    ) -> ProcessingResult<TargetRecipeResult> {
        let comparison = self.comparison(pair, cancel)?;
        let source = stats(&comparison.source);
        let reference = stats(&comparison.reference);
        let mut controls = PredictedCreativeAdjustments {
            exposure_ev: (reference.q50.max(1e-5) / source.q50.max(1e-5))
                .log2()
                .clamp(-3.0, 3.0),
            ..Default::default()
        };
        let mut best = Self::loss(&comparison, controls, cancel)?;
        let stages: [(&[StyleControl], &[f32]); 3] = [
            (
                &[
                    StyleControl::ExposureEv,
                    StyleControl::TemperatureDelta,
                    StyleControl::Tint,
                ],
                &[0.5, 500.0, 8.0],
            ),
            (
                &[
                    StyleControl::Highlights,
                    StyleControl::Shadows,
                    StyleControl::Whites,
                    StyleControl::Blacks,
                ],
                &[20.0, 20.0, 20.0, 20.0],
            ),
            (
                &[
                    StyleControl::Saturation,
                    StyleControl::Vibrance,
                    StyleControl::Clarity,
                ],
                &[12.0, 12.0, 12.0],
            ),
        ];
        let mut iterations = 1u32;
        for (stage_controls, starting_steps) in stages {
            for refinement in 0..3 {
                for (control, initial_step) in stage_controls.iter().zip(starting_steps) {
                    cancel.check()?;
                    let step = *initial_step / 2f32.powi(refinement);
                    for direction in [-1.0, 1.0] {
                        let mut candidate = controls;
                        let value = control_value(candidate, *control) + direction * step;
                        set_control(&mut candidate, *control, bounded(*control, value));
                        let loss = Self::loss(&comparison, candidate, cancel)?;
                        iterations += 1;
                        if loss.total + 1e-7 < best.total {
                            best = loss;
                            controls = candidate;
                        }
                    }
                }
            }
        }
        let confidence = if best.total <= 0.055 {
            TargetFitConfidence::High
        } else if best.total <= 0.13 {
            TargetFitConfidence::Medium
        } else {
            TargetFitConfidence::Low
        };
        let mut unsupported_differences = Vec::new();
        if pair.validation.geometry == GeometryRelationship::CropDifference {
            unsupported_differences
                .push("crop/geometry preference is diagnosed but not learned in v1".into());
        }
        if best.structure > 0.35 {
            unsupported_differences
                .push("possible local retouching or structural difference".into());
        }
        let warnings = if confidence == TargetFitConfidence::Low {
            vec!["Low target-fit confidence; excluded from training by default".into()]
        } else {
            Vec::new()
        };
        let cache_identity = target_cache_identity(pair, self.engine.analysis_mask_version());
        let mut recipe = EditRecipe::neutral(
            format!("target-{}", pair.pair_id),
            pair.source_asset_id.clone(),
            "1970-01-01T00:00:00Z".into(),
        );
        apply_controls(&mut recipe, controls);
        recipe = recipe.validated()?;
        Ok(TargetRecipeResult {
            schema_version: TARGET_RECIPE_SCHEMA_VERSION,
            optimizer_version: self.version().into(),
            cache_identity,
            recipe,
            controls,
            confidence,
            loss: best,
            iterations,
            unsupported_differences,
            warnings,
        })
    }

    fn rendered_loss(
        &self,
        pair: &TrainingPair,
        controls: PredictedCreativeAdjustments,
        cancel: &CancellationToken,
    ) -> ProcessingResult<f32> {
        Ok(Self::loss(&self.comparison(pair, cancel)?, controls, cancel)?.total)
    }
}

pub fn target_cache_identity(pair: &TrainingPair, mask_version: &str) -> String {
    let controls = TARGET_CONTROLS
        .iter()
        .map(|control| format!("{control:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let mut hash = Sha256::new();
    for part in [
        pair.source_fingerprint.as_str(),
        pair.reference_fingerprint.as_str(),
        RENDERER_VERSION,
        TARGET_OPTIMIZER_VERSION,
        controls.as_str(),
        mask_version,
    ] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

pub fn apply_controls(recipe: &mut EditRecipe, controls: PredictedCreativeAdjustments) {
    recipe.global.basic.exposure_ev = controls.exposure_ev;
    recipe.global.basic.temperature = 6500.0 + controls.temperature_delta;
    recipe.global.basic.tint = controls.tint;
    recipe.global.basic.highlights = controls.highlights;
    recipe.global.basic.shadows = controls.shadows;
    recipe.global.basic.whites = controls.whites;
    recipe.global.basic.blacks = controls.blacks;
    recipe.global.basic.saturation = controls.saturation;
    recipe.global.basic.vibrance = controls.vibrance;
    recipe.global.presence.clarity = controls.clarity;
}

pub fn control_value(value: PredictedCreativeAdjustments, control: StyleControl) -> f32 {
    match control {
        StyleControl::ExposureEv => value.exposure_ev,
        StyleControl::TemperatureDelta => value.temperature_delta,
        StyleControl::Tint => value.tint,
        StyleControl::Highlights => value.highlights,
        StyleControl::Shadows => value.shadows,
        StyleControl::Whites => value.whites,
        StyleControl::Blacks => value.blacks,
        StyleControl::Saturation => value.saturation,
        StyleControl::Vibrance => value.vibrance,
        StyleControl::Clarity => value.clarity,
        _ => 0.0,
    }
}

fn set_control(value: &mut PredictedCreativeAdjustments, control: StyleControl, next: f32) {
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

pub fn bounded(control: StyleControl, value: f32) -> f32 {
    match control {
        StyleControl::ExposureEv => value.clamp(-3.0, 3.0),
        StyleControl::TemperatureDelta => value.clamp(-3000.0, 3000.0),
        StyleControl::Tint => value.clamp(-50.0, 50.0),
        StyleControl::Saturation | StyleControl::Vibrance => value.clamp(-50.0, 50.0),
        StyleControl::Clarity => value.clamp(-40.0, 40.0),
        _ => value.clamp(-80.0, 80.0),
    }
}

fn render_adjustments(value: PredictedCreativeAdjustments) -> RenderAdjustments {
    RenderAdjustments {
        exposure_ev: value.exposure_ev,
        temperature: 6500.0 + value.temperature_delta,
        tint: value.tint,
        highlights: value.highlights,
        shadows: value.shadows,
        whites: value.whites,
        blacks: value.blacks,
        saturation: value.saturation,
        vibrance: value.vibrance,
        ..Default::default()
    }
}

fn stats(image: &FloatImage) -> Stats {
    let structure = sample_structure(image, 20, 20);
    let mut luminance = image
        .pixels
        .iter()
        .map(|pixel| luma(*pixel).clamp(0.0, 2.0))
        .collect::<Vec<_>>();
    luminance.sort_by(f32::total_cmp);
    let quantile =
        |fraction: f32| luminance[((luminance.len() - 1) as f32 * fraction).round() as usize];
    let mut mean = [0.0f32; 3];
    let mut saturation = 0.0;
    for pixel in &image.pixels {
        for index in 0..3 {
            mean[index] += pixel[index].clamp(0.0, 2.0);
        }
        let high = pixel.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let low = pixel.iter().copied().fold(f32::INFINITY, f32::min);
        saturation += if high > 1e-6 {
            ((high - low) / high).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    for value in &mut mean {
        *value /= image.pixels.len() as f32;
    }
    Stats {
        q10: quantile(0.10),
        q50: quantile(0.50),
        q90: quantile(0.90),
        mean,
        saturation: saturation / image.pixels.len() as f32,
        structure,
    }
}

fn sample_structure(image: &FloatImage, width: usize, height: usize) -> Vec<f32> {
    let aspect = image.width as f32 / image.height as f32;
    let target_aspect = 1.0f32;
    let (crop_x, crop_y, crop_w, crop_h) = if aspect > target_aspect {
        let width_fraction = target_aspect / aspect;
        ((1.0 - width_fraction) / 2.0, 0.0, width_fraction, 1.0)
    } else {
        let height_fraction = aspect / target_aspect;
        (0.0, (1.0 - height_fraction) / 2.0, 1.0, height_fraction)
    };
    let mut samples = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let u = crop_x + (x as f32 + 0.5) / width as f32 * crop_w;
            let v = crop_y + (y as f32 + 0.5) / height as f32 * crop_h;
            samples.push(luma(
                image.sample(u * image.width as f32 - 0.5, v * image.height as f32 - 0.5),
            ));
        }
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let variance = samples
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f32>()
        / samples.len() as f32;
    let scale = variance.sqrt().max(1e-4);
    samples
        .into_iter()
        .map(|value| (value - mean) / scale)
        .collect()
}

fn correlation(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let score = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>()
        / left.len() as f32;
    ((score + 1.0) / 2.0).clamp(0.0, 1.0)
}
