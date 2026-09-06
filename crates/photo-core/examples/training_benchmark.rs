//! Synthetic throughput benchmark only. It does not measure photographic style quality.
use photo_contracts::{
    analysis::PhotoType,
    trained_style::{StyleFeatureVector, STYLE_FEATURE_SCHEMA_V1},
    training::{
        PairValidation, TargetFitConfidence, TrainingConfig, TrainingDataset, TrainingPair,
        TrainingSplit, TRAINING_DATASET_SCHEMA_VERSION, TRAINING_PAIR_SCHEMA_VERSION,
    },
    CancellationToken, OutputFormat, ProcessingEngine, RenderAdjustments, RenderRequest,
    RECIPE_SCHEMA_VERSION,
};
use photo_core::{
    rendering::{
        decode::{Decoded, RawDecoder},
        CpuProcessingEngine, RenderLimits, RENDERER_VERSION,
    },
    trained_styles::features::STYLE_FEATURE_NAMES,
    training::{
        package::{export_style_package, next_style_identity},
        target::{StagedTargetOptimizer, TargetRecipeOptimizer},
        trainer::{
            assign_splits, predict_controls, RegularizedLinearTrainer, StyleModelTrainer,
            TrainingExample,
        },
    },
};
use std::{path::Path, sync::Arc, time::Instant};

struct NoRaw;

impl RawDecoder for NoRaw {
    fn id(&self) -> &str {
        "phase-8-benchmark-raster-only"
    }

    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &CancellationToken,
    ) -> photo_contracts::ProcessingResult<Decoded> {
        unreachable!("benchmark fixtures are PNG/TIFF")
    }
}

fn feature(index: usize, luminance: f32) -> StyleFeatureVector {
    let mut values = vec![0.0; STYLE_FEATURE_NAMES.len()];
    values[0] = luminance;
    values[8] = (index as f32 * 0.17).sin() * 0.2;
    values[23] = 1.0;
    StyleFeatureVector {
        schema_version: STYLE_FEATURE_SCHEMA_V1.into(),
        asset_id: format!("asset-{index}"),
        analysis_id: format!("analysis-{index}"),
        batch_context_id: "synthetic-batch".into(),
        feature_names: STYLE_FEATURE_NAMES
            .iter()
            .map(|name| (*name).into())
            .collect(),
        values,
        available: vec![true; STYLE_FEATURE_NAMES.len()],
        missing_features: vec![],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ));
    let optimizer = StagedTargetOptimizer::with_proxy_edge(engine.clone(), 64);
    let cancel = CancellationToken::default();
    let mut all_pairs = Vec::new();
    for index in 0..250 {
        let source = root.path().join(format!("source-{index}.png"));
        let reference = root.path().join(format!("reference-{index}.tif"));
        let base = 30 + (index % 90) as u8;
        image::RgbImage::from_fn(96, 72, |x, y| {
            let detail = ((x / 8 + y / 8 + index as u32) % 2) as u8 * 28;
            image::Rgb([
                base.saturating_add(detail),
                base.saturating_add((x % 45) as u8),
                base.saturating_add((y % 50) as u8),
            ])
        })
        .save(&source)?;
        let exposure = 0.9 - base as f32 / 180.0;
        engine.render(
            &RenderRequest {
                asset_id: format!("asset-{index}"),
                original: source.clone(),
                adjustments: RenderAdjustments {
                    exposure_ev: exposure,
                    shadows: 8.0,
                    ..Default::default()
                },
                source_metadata: Default::default(),
                destination: reference.clone(),
                output_format: OutputFormat::Tiff,
                preview: false,
                jpeg_quality: 95,
            },
            &cancel,
        )?;
        all_pairs.push(TrainingPair {
            schema_version: TRAINING_PAIR_SCHEMA_VERSION,
            pair_id: format!("pair-{index}"),
            dataset_id: "benchmark".into(),
            source_job_id: "benchmark-job".into(),
            source_asset_id: format!("asset-{index}"),
            source_path: source,
            reference_path: reference,
            photo_type: PhotoType::Portrait,
            source_fingerprint: format!("{index:064x}"),
            reference_fingerprint: format!("{:064x}", index + 1000),
            validation: PairValidation::default(),
            source_analysis_id: Some(format!("analysis-{index}")),
            batch_context: None,
            scene_group_id: Some(format!("scene-{}", index / 2)),
            target: None,
            split: TrainingSplit::Unassigned,
            excluded: false,
            feedback: None,
            diagnostics: vec![],
        });
    }

    println!("Phase 8 synthetic benchmark; 64 px target proxies; no quality extrapolation");
    println!("pairs,analysis_cached_ms,target_estimation_ms,model_fit_ms,validation_ms,package_export_ms");
    for count in [10usize, 25, 50, 100, 250] {
        let mut dataset = TrainingDataset {
            schema_version: TRAINING_DATASET_SCHEMA_VERSION,
            dataset_id: "benchmark".into(),
            job_id: "benchmark-job".into(),
            style_name: format!("Benchmark {count}"),
            photo_type: PhotoType::Portrait,
            pairs: all_pairs[..count].to_vec(),
            created_at: "2026-09-05T00:00:00Z".into(),
            updated_at: "2026-09-05T00:00:00Z".into(),
            dataset_fingerprint: Some(format!("{count:064x}")),
            feature_schema: STYLE_FEATURE_SCHEMA_V1.into(),
            renderer_version: RENDERER_VERSION.into(),
            target_recipe_schema: RECIPE_SCHEMA_VERSION,
            batch_context_id: Some("synthetic-batch".into()),
            warnings: vec![],
            before_files: vec![],
            after_files: vec![],
            alignment: None,
        };
        let analysis_started = Instant::now();
        let features = (0..count)
            .map(|index| feature(index, 0.1 + (index % 80) as f32 / 100.0))
            .collect::<Vec<_>>();
        let analysis_ms = analysis_started.elapsed().as_millis();

        let target_started = Instant::now();
        for pair in &mut dataset.pairs {
            pair.target = Some(optimizer.estimate(pair, &cancel)?);
        }
        let target_ms = target_started.elapsed().as_millis();
        assign_splits(&mut dataset, &TrainingConfig::default())?;
        let examples = dataset
            .pairs
            .iter()
            .enumerate()
            .map(|(index, pair)| TrainingExample {
                pair_id: pair.pair_id.clone(),
                features: features[index].clone(),
                target: pair.target.as_ref().unwrap().controls,
                confidence: pair
                    .target
                    .as_ref()
                    .map(|target| target.confidence)
                    .unwrap_or(TargetFitConfidence::Low),
                split: pair.split,
            })
            .collect::<Vec<_>>();

        let fit_started = Instant::now();
        let artifact = RegularizedLinearTrainer.train(
            &examples,
            &TrainingConfig::default(),
            &format!("benchmark-{count}"),
        )?;
        let fit_ms = fit_started.elapsed().as_millis();

        let validation_started = Instant::now();
        let _validation_checksum = examples
            .iter()
            .filter(|example| example.split == TrainingSplit::Validation)
            .map(|example| predict_controls(&artifact.model, &example.features).exposure_ev)
            .sum::<f32>();
        let validation_ms = validation_started.elapsed().as_millis();

        let style_root = root.path().join(format!("styles-{count}"));
        let export_started = Instant::now();
        let identity = next_style_identity(
            &style_root,
            &dataset.style_name,
            dataset.dataset_fingerprint.as_deref().unwrap(),
        );
        export_style_package(&style_root, &identity, &dataset, &artifact, &cancel)?;
        let export_ms = export_started.elapsed().as_millis();
        println!("{count},{analysis_ms},{target_ms},{fit_ms},{validation_ms},{export_ms}");
    }
    Ok(())
}
