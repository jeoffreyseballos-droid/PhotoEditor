use photo_contracts::{
    analysis::PhotoType,
    formats::FileType,
    trained_style::{
        PredictedCreativeAdjustments, StyleFeatureVector, StyleModelKind, STYLE_FEATURE_SCHEMA_V1,
    },
    training::{
        GeometryRelationship, PairValidation, PairValidationStatus, TargetFitConfidence,
        TargetLossBreakdown, TargetRecipeResult, TrainingConfig, TrainingDataset, TrainingPair,
        TrainingSplit, TARGET_OPTIMIZER_VERSION, TARGET_RECIPE_SCHEMA_VERSION,
        TRAINING_DATASET_SCHEMA_VERSION, TRAINING_PAIR_SCHEMA_VERSION,
    },
    CancellationToken, EditRecipe, OutputFormat, ProcessingEngine, RenderAdjustments,
    RenderRequest, RECIPE_SCHEMA_VERSION,
};
use photo_core::{
    analysis::AnalysisService,
    jobs::JobService,
    models::NewJob,
    models::{Asset, ImageMetadata},
    rendering::{
        decode::{Decoded, RawDecoder},
        CpuProcessingEngine, RenderLimits, RENDERER_VERSION,
    },
    trained_styles::{features::STYLE_FEATURE_NAMES, package::load_style_package},
    training::{
        matcher::{auto_match, match_paths, normalized_stem},
        package::{export_style_package, next_style_identity},
        target::{target_cache_identity, StagedTargetOptimizer, TargetRecipeOptimizer},
        trainer::{
            assign_splits, predict_controls, RegularizedLinearTrainer, StyleModelTrainer,
            TrainingExample,
        },
        CreateTrainingDatasetRequest, TrainingRequest, TrainingService,
    },
};
use std::{path::Path, sync::Arc};
use tempfile::tempdir;

struct NoRaw;

impl RawDecoder for NoRaw {
    fn id(&self) -> &str {
        "phase-8-training-test"
    }

    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &CancellationToken,
    ) -> photo_contracts::ProcessingResult<Decoded> {
        panic!("Synthetic tests only use PNG inputs")
    }
}

fn engine() -> Arc<CpuProcessingEngine> {
    Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ))
}

fn synthetic_source(path: &Path) {
    image::RgbImage::from_fn(256, 192, |x, y| {
        let checker = if (x / 24 + y / 24) % 2 == 0 { 25 } else { 0 };
        image::Rgb([
            (25 + x * 150 / 255 + checker).min(255) as u8,
            (30 + y * 140 / 191 + checker).min(255) as u8,
            (45 + (x + y) * 90 / 446 + checker).min(255) as u8,
        ])
    })
    .save(path)
    .unwrap();
}

fn training_pair(source: &Path, reference: &Path, id: &str) -> TrainingPair {
    TrainingPair {
        schema_version: TRAINING_PAIR_SCHEMA_VERSION,
        pair_id: id.into(),
        dataset_id: "dataset".into(),
        source_job_id: "job".into(),
        source_asset_id: format!("asset-{id}"),
        source_path: source.into(),
        reference_path: reference.into(),
        photo_type: PhotoType::Portrait,
        source_fingerprint: "a".repeat(64),
        reference_fingerprint: "b".repeat(64),
        validation: PairValidation::default(),
        source_analysis_id: None,
        batch_context: None,
        scene_group_id: None,
        target: None,
        split: TrainingSplit::Unassigned,
        excluded: false,
        feedback: None,
        diagnostics: vec![],
    }
}

fn target(exposure: f32) -> TargetRecipeResult {
    TargetRecipeResult {
        schema_version: TARGET_RECIPE_SCHEMA_VERSION,
        optimizer_version: TARGET_OPTIMIZER_VERSION.into(),
        cache_identity: "c".repeat(64),
        recipe: EditRecipe::neutral(
            "target".into(),
            "asset".into(),
            "2026-09-05T00:00:00Z".into(),
        ),
        controls: PredictedCreativeAdjustments {
            exposure_ev: exposure,
            ..Default::default()
        },
        confidence: TargetFitConfidence::High,
        loss: TargetLossBreakdown {
            total: 0.01,
            luminance: 0.01,
            color_balance: 0.0,
            saturation: 0.0,
            structure: 0.0,
        },
        iterations: 61,
        unsupported_differences: vec![],
        warnings: vec![],
    }
}

fn feature(id: usize, source_luminance: f32) -> StyleFeatureVector {
    let mut values = vec![0.0; STYLE_FEATURE_NAMES.len()];
    values[0] = source_luminance;
    StyleFeatureVector {
        schema_version: STYLE_FEATURE_SCHEMA_V1.into(),
        asset_id: format!("asset-{id}"),
        analysis_id: format!("analysis-{id}"),
        batch_context_id: "batch".into(),
        feature_names: STYLE_FEATURE_NAMES
            .iter()
            .map(|value| (*value).into())
            .collect(),
        values,
        available: vec![true; STYLE_FEATURE_NAMES.len()],
        missing_features: vec![],
    }
}

fn examples() -> Vec<TrainingExample> {
    (0..10)
        .map(|index| {
            let luminance = 0.1 + index as f32 * 0.08;
            TrainingExample {
                pair_id: format!("pair-{index}"),
                features: feature(index, luminance),
                target: PredictedCreativeAdjustments {
                    exposure_ev: 1.2 - 2.0 * luminance,
                    temperature_delta: 150.0 + 200.0 * luminance,
                    shadows: 20.0 - 10.0 * luminance,
                    ..Default::default()
                },
                confidence: if index == 9 {
                    TargetFitConfidence::Medium
                } else {
                    TargetFitConfidence::High
                },
                split: if index == 2 || index == 7 {
                    TrainingSplit::Validation
                } else {
                    TrainingSplit::Train
                },
            }
        })
        .collect()
}

fn dataset_with_pairs(count: usize) -> TrainingDataset {
    let placeholder = Path::new("placeholder.png");
    let pairs = (0..count)
        .map(|index| {
            let mut pair = training_pair(placeholder, placeholder, &format!("pair-{index}"));
            pair.target = Some(target(index as f32 / 10.0));
            pair.scene_group_id = Some(format!("scene-{}", index / 2));
            pair
        })
        .collect();
    TrainingDataset {
        schema_version: TRAINING_DATASET_SCHEMA_VERSION,
        dataset_id: "dataset".into(),
        job_id: "job".into(),
        style_name: "Synthetic Portrait".into(),
        photo_type: PhotoType::Portrait,
        pairs,
        created_at: "2026-09-05T00:00:00Z".into(),
        updated_at: "2026-09-05T00:00:00Z".into(),
        dataset_fingerprint: Some("d".repeat(64)),
        feature_schema: STYLE_FEATURE_SCHEMA_V1.into(),
        renderer_version: RENDERER_VERSION.into(),
        target_recipe_schema: RECIPE_SCHEMA_VERSION,
        batch_context_id: None,
        warnings: vec![],
        before_files: vec![],
        after_files: vec![],
        alignment: None,
    }
}

#[test]
fn known_renderer_edit_is_recovered_by_target_optimizer() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.png");
    let reference = root.path().join("source_EDIT.tif");
    synthetic_source(&source);
    let engine = engine();
    let known = RenderAdjustments {
        exposure_ev: 0.8,
        temperature: 7000.0,
        ..Default::default()
    };
    engine
        .render(
            &RenderRequest {
                asset_id: "asset-pair".into(),
                original: source.clone(),
                adjustments: known,
                source_metadata: Default::default(),
                destination: reference.clone(),
                output_format: OutputFormat::Tiff,
                preview: false,
                jpeg_quality: 95,
            },
            &CancellationToken::default(),
        )
        .unwrap();
    let optimizer = StagedTargetOptimizer::with_proxy_edge(engine, 192);
    let mut pair = training_pair(&source, &reference, "pair");
    let validation = optimizer
        .validate_pair(&pair, &CancellationToken::default())
        .unwrap();
    assert_eq!(validation.geometry, GeometryRelationship::ExactOrNear);
    assert_eq!(validation.status, PairValidationStatus::Ready);
    pair.validation = validation;
    let neutral_loss = optimizer
        .rendered_loss(
            &pair,
            PredictedCreativeAdjustments::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let estimated = optimizer
        .estimate(&pair, &CancellationToken::default())
        .unwrap();
    println!(
        "synthetic target recovery: exposure={:.3} EV, temperature_delta={:.1} K, neutral_loss={neutral_loss:.5}, fitted_loss={:.5}",
        estimated.controls.exposure_ev,
        estimated.controls.temperature_delta,
        estimated.loss.total
    );
    assert!((estimated.controls.exposure_ev - 0.8).abs() <= 0.4);
    assert!((estimated.controls.temperature_delta - 500.0).abs() <= 750.0);
    assert!(estimated.controls.temperature_delta > 0.0);
    assert!(estimated.loss.total < neutral_loss * 0.45);
    assert_ne!(estimated.confidence, TargetFitConfidence::Low);
    let repeated = optimizer
        .estimate(&pair, &CancellationToken::default())
        .unwrap();
    assert_eq!(estimated.controls, repeated.controls);
    assert_eq!(estimated.loss, repeated.loss);
}

#[test]
fn conservative_auto_match_reports_reference_and_source_ambiguity() {
    let root = tempdir().unwrap();
    std::fs::write(root.path().join("IMG_1001_EDIT.jpg"), b"reference").unwrap();
    std::fs::write(root.path().join("IMG_1002.jpg"), b"reference").unwrap();
    std::fs::write(root.path().join("IMG_1002-final.tif"), b"reference").unwrap();
    let asset = |id: &str, filename: &str| Asset {
        id: id.into(),
        job_id: "job".into(),
        original_path: Path::new(filename).into(),
        filename: filename.into(),
        file_type: FileType::Cr3,
        file_size: 1,
        modified_at: None,
        fingerprint: "a".repeat(64),
        metadata: ImageMetadata::default(),
        thumbnail_path: None,
        preview_status: "ready".into(),
        metadata_warning: None,
        created_at: "2026-09-05T00:00:00Z".into(),
        warnings: vec![],
    };
    let assets = vec![
        asset("1", "IMG_1001.CR3"),
        asset("1-jpg", "IMG_1001.JPG"),
        asset("2", "IMG_1002.CR3"),
    ];
    let result = auto_match(&assets, root.path());
    assert!(result.matched.is_empty());
    assert_eq!(result.ambiguous_sources.len(), 3);
    assert_eq!(result.unmatched_references.len(), 3);
    assert_eq!(
        normalized_stem(Path::new("IMG_1001_EDIT.jpg")).as_deref(),
        Some("img1001")
    );
}

#[test]
fn standalone_matching_reports_alignment_and_never_shifts_after_a_missing_file() {
    let before = vec![
        Path::new("before/IMG_1001.CR3").to_path_buf(),
        Path::new("before/IMG_1002.CR3").to_path_buf(),
        Path::new("before/IMG_1003.CR3").to_path_buf(),
        Path::new("before/IMG_1004.CR3").to_path_buf(),
    ];
    let after = vec![
        Path::new("after/IMG_1001_EDIT.JPG").to_path_buf(),
        Path::new("after/IMG_1003.JPG").to_path_buf(),
        Path::new("after/IMG_1004.JPG").to_path_buf(),
    ];
    let result = match_paths(&before, &after);
    assert_eq!(result.before_count, 4);
    assert_eq!(result.after_count, 3);
    assert_eq!(result.matched.len(), 3);
    assert_eq!(result.unmatched_sources, vec!["before/IMG_1002.CR3"]);
    assert!(result
        .matched
        .iter()
        .all(|candidate| candidate.source_filename != "IMG_1002.CR3"));
    assert!(!result.order_fallback_used);
}

#[test]
fn standalone_matching_allows_order_candidates_only_for_equal_unmatched_folders() {
    let before = vec![
        Path::new("before/DSC0001.CR3").into(),
        Path::new("before/DSC0002.CR3").into(),
    ];
    let after = vec![
        Path::new("after/export-001.JPG").into(),
        Path::new("after/export-002.JPG").into(),
    ];
    let result = match_paths(&before, &after);
    assert_eq!(result.matched.len(), 2);
    assert!(result.order_fallback_used);
    assert!(!result.diagnostics.is_empty());
}

#[test]
fn scene_aware_split_is_stable_and_never_leaks_a_group() {
    let mut first = dataset_with_pairs(10);
    let mut second = first.clone();
    let config = TrainingConfig::default();
    assign_splits(&mut first, &config).unwrap();
    assign_splits(&mut second, &config).unwrap();
    assert_eq!(
        first
            .pairs
            .iter()
            .map(|pair| pair.split)
            .collect::<Vec<_>>(),
        second
            .pairs
            .iter()
            .map(|pair| pair.split)
            .collect::<Vec<_>>()
    );
    for scene in 0..5 {
        let splits = first
            .pairs
            .iter()
            .filter(|pair| pair.scene_group_id.as_deref() == Some(&format!("scene-{scene}")))
            .map(|pair| pair.split)
            .collect::<Vec<_>>();
        assert!(splits.windows(2).all(|values| values[0] == values[1]));
    }
    assert!(first
        .pairs
        .iter()
        .any(|pair| pair.split == TrainingSplit::Validation));
    assert!(first
        .pairs
        .iter()
        .any(|pair| pair.split == TrainingSplit::Train));
}

#[test]
fn target_cache_identity_tracks_pair_and_renderer_dependencies_not_trainer_config() {
    let mut pair = training_pair(Path::new("source.png"), Path::new("reference.jpg"), "cache");
    let first = target_cache_identity(&pair, "mask-v1");
    let _different_trainer_config = TrainingConfig {
        regularization: 0.9,
        epochs: 400,
        ..Default::default()
    };
    assert_eq!(first, target_cache_identity(&pair, "mask-v1"));
    pair.reference_fingerprint = "f".repeat(64);
    assert_ne!(first, target_cache_identity(&pair, "mask-v1"));
    pair.reference_fingerprint = "b".repeat(64);
    assert_ne!(first, target_cache_identity(&pair, "mask-v2"));
}

#[test]
fn regularized_model_learns_adaptive_unseen_predictions_and_persists_normalization() {
    let examples = examples();
    let artifact = RegularizedLinearTrainer
        .train(&examples, &TrainingConfig::default(), "synthetic-v1")
        .unwrap();
    let dark = predict_controls(&artifact.model, &feature(20, 0.18));
    let bright = predict_controls(&artifact.model, &feature(21, 0.78));
    assert!(dark.exposure_ev > bright.exposure_ev + 0.65);
    assert!((dark.exposure_ev - (1.2 - 2.0 * 0.18)).abs() < 0.25);
    assert!((bright.exposure_ev - (1.2 - 2.0 * 0.78)).abs() < 0.25);
    assert!((-3.0..=3.0).contains(&dark.exposure_ev));
    assert!((-3.0..=3.0).contains(&bright.exposure_ev));
    assert!(
        artifact.metrics.validation.mean_recipe_mae
            < artifact.metrics.mean_baseline.mean_recipe_mae
    );
    let StyleModelKind::LinearV1(model) = &artifact.model.model;
    assert_eq!(model.feature_means.len(), STYLE_FEATURE_NAMES.len());
    assert_eq!(model.feature_scales.len(), STYLE_FEATURE_NAMES.len());
    assert!(model
        .feature_scales
        .iter()
        .all(|value| value.is_finite() && *value > 0.0));
}

#[test]
fn bright_airy_moody_and_neutral_styles_remain_distinct() {
    let styles = [
        ("airy", 0.85, 320.0, 24.0, 8.0),
        ("moody", -0.55, -280.0, -30.0, -10.0),
        ("neutral", 0.05, 20.0, 1.0, 0.0),
    ];
    let predictions = styles.map(|(name, exposure, temperature, shadows, saturation)| {
        let examples = (0..10)
            .map(|index| TrainingExample {
                pair_id: format!("{name}-{index}"),
                features: feature(index, 0.12 + index as f32 * 0.075),
                target: PredictedCreativeAdjustments {
                    exposure_ev: exposure - index as f32 * 0.015,
                    temperature_delta: temperature,
                    shadows,
                    saturation,
                    ..Default::default()
                },
                confidence: TargetFitConfidence::High,
                split: if index == 8 || index == 9 {
                    TrainingSplit::Validation
                } else {
                    TrainingSplit::Train
                },
            })
            .collect::<Vec<_>>();
        let artifact = RegularizedLinearTrainer
            .train(&examples, &TrainingConfig::default(), name)
            .unwrap();
        predict_controls(&artifact.model, &feature(30, 0.47))
    });
    let [airy, moody, neutral] = predictions;
    assert!(airy.exposure_ev > 0.5 && airy.temperature_delta > 200.0 && airy.shadows > 15.0);
    assert!(
        moody.exposure_ev < -0.3 && moody.temperature_delta < -150.0 && moody.saturation < -5.0
    );
    assert!(neutral.exposure_ev.abs() < 0.2 && neutral.temperature_delta.abs() < 100.0);
}

#[test]
fn trained_package_is_phase_7_compatible_and_versions_are_preserved() {
    let root = tempdir().unwrap();
    let artifact = RegularizedLinearTrainer
        .train(&examples(), &TrainingConfig::default(), "temporary")
        .unwrap();
    let dataset = dataset_with_pairs(10);
    let first = next_style_identity(root.path(), &dataset.style_name, "d123456789abcdef");
    let (first_path, first_package) = export_style_package(
        root.path(),
        &first,
        &dataset,
        &artifact,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(first.style_id, "synthetic-portrait-v1");
    assert_eq!(load_style_package(&first_path).unwrap(), first_package);
    assert!(first_package.metadata.training.is_some());
    assert!(!serde_json::to_string(&first_package.metadata)
        .unwrap()
        .contains("placeholder.png"));

    let second = next_style_identity(root.path(), &dataset.style_name, "d123456789abcdef");
    let (second_path, _) = export_style_package(
        root.path(),
        &second,
        &dataset,
        &artifact,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(second.style_id, "synthetic-portrait-v2");
    assert!(first_path.exists());
    assert!(second_path.exists());
}

#[test]
fn cancelled_package_export_never_publishes_a_style_directory() {
    let root = tempdir().unwrap();
    let artifact = RegularizedLinearTrainer
        .train(&examples(), &TrainingConfig::default(), "temporary")
        .unwrap();
    let dataset = dataset_with_pairs(10);
    let identity = next_style_identity(root.path(), &dataset.style_name, "d123456789abcdef");
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(export_style_package(root.path(), &identity, &dataset, &artifact, &cancel).is_err());
    assert!(!root.path().join(identity.style_id).exists());
}

#[test]
fn dataset_and_cancelled_run_are_persisted_with_small_data_guidance() {
    let root = tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir_all(&output).unwrap();
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Training Test".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    drop(permit);
    let engine = engine();
    let analysis = Arc::new(AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        None,
    ));
    let service = TrainingService::new(
        jobs.repository.clone(),
        analysis,
        engine,
        root.path().join("styles"),
        root.path().join("training-cache"),
    )
    .unwrap();
    let dataset = service
        .create_dataset(CreateTrainingDatasetRequest {
            job_id: job.id.clone(),
            style_name: "  Small Portrait  ".into(),
            photo_type: PhotoType::Portrait,
        })
        .unwrap();
    assert_eq!(dataset.style_name, "Small Portrait");
    assert!(dataset
        .warnings
        .iter()
        .any(|warning| warning.contains("experimental")));
    assert_eq!(service.datasets(&job.id).unwrap(), vec![dataset.clone()]);

    let permit = service
        .reserve(TrainingRequest {
            dataset_id: dataset.dataset_id.clone(),
            request_id: "cancelled-run".into(),
            config: TrainingConfig::default(),
        })
        .unwrap();
    drop(permit);
    let run = service.progress(&dataset.dataset_id).unwrap().unwrap();
    assert_eq!(
        run.status,
        photo_contracts::training::TrainingRunStatus::Cancelled
    );
    assert_eq!(run.stage, photo_contracts::training::TrainingStage::Stopped);
}

#[test]
fn standalone_dataset_accepts_independent_before_after_inputs_without_a_visible_job() {
    let root = tempdir().unwrap();
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let engine = engine();
    let analysis = Arc::new(AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        None,
    ));
    let service = TrainingService::new(
        jobs.repository.clone(),
        analysis,
        engine,
        root.path().join("styles"),
        root.path().join("training-cache"),
    )
    .unwrap();

    let before = root.path().join("before").join("IMG_0001.png");
    let after = root.path().join("after").join("IMG_0001_EDIT.png");
    std::fs::create_dir_all(before.parent().unwrap()).unwrap();
    std::fs::create_dir_all(after.parent().unwrap()).unwrap();
    synthetic_source(&before);
    synthetic_source(&after);

    let dataset = service
        .create_dataset(CreateTrainingDatasetRequest {
            job_id: String::new(),
            style_name: "Standalone Portrait".into(),
            photo_type: PhotoType::Portrait,
        })
        .unwrap();
    let dataset = service
        .add_before_files(photo_core::training::AddTrainingFilesRequest {
            dataset_id: dataset.dataset_id.clone(),
            paths: vec![before.clone()],
        })
        .unwrap();
    let dataset = service
        .add_after_files(photo_core::training::AddTrainingFilesRequest {
            dataset_id: dataset.dataset_id.clone(),
            paths: vec![after.clone()],
        })
        .unwrap();
    let result = service.match_dataset(&dataset.dataset_id).unwrap();

    assert_eq!(result.dataset.job_id, dataset.job_id);
    assert_eq!(
        result.dataset.before_files,
        vec![before.canonicalize().unwrap()]
    );
    assert_eq!(
        result.dataset.after_files,
        vec![after.canonicalize().unwrap()]
    );
    assert_eq!(result.dataset.pairs.len(), 1);
    assert_eq!(result.matching.matched.len(), 1);
    assert_eq!(result.dataset.alignment.unwrap().matched_count, 1);
    assert!(service
        .all_datasets()
        .unwrap()
        .iter()
        .any(|item| item.dataset_id == dataset.dataset_id));
    assert!(jobs.repository.list_jobs(0, 100).unwrap().items.is_empty());
}

fn standalone_service(
    root: &Path,
    optimizer: Option<Arc<dyn TargetRecipeOptimizer>>,
) -> TrainingService {
    let jobs = JobService::new(root.join("data"), root.join("cache")).unwrap();
    let engine = engine();
    let analysis = Arc::new(AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        None,
    ));
    TrainingService::with_components(
        jobs.repository,
        analysis,
        engine.clone(),
        optimizer.unwrap_or_else(|| Arc::new(StagedTargetOptimizer::with_proxy_edge(engine, 64))),
        Arc::new(RegularizedLinearTrainer),
        root.join("styles"),
        root.join("training-cache"),
    )
    .unwrap()
}

fn folder_dataset(root: &Path, service: &TrainingService, count: usize) -> TrainingDataset {
    use photo_core::training::AddTrainingFolderRequest;
    let before = root.join("before");
    let after = root.join("after");
    std::fs::create_dir_all(&before).unwrap();
    std::fs::create_dir_all(&after).unwrap();
    for i in (1..=count).rev() {
        synthetic_source(&before.join(format!("2H1A{}.png", 3374 + i)));
        synthetic_source(&after.join(format!("Sheila ({i} of {count}).png")));
    }
    std::fs::create_dir_all(before.join(".hidden")).unwrap();
    synthetic_source(&before.join(".hidden").join("ignored.png"));
    std::fs::create_dir_all(after.join("photoeditor-cache")).unwrap();
    synthetic_source(&after.join("photoeditor-cache").join("ignored.png"));
    std::fs::write(after.join("readme.txt"), "unsupported").unwrap();
    let dataset = service
        .create_dataset(CreateTrainingDatasetRequest {
            job_id: String::new(),
            style_name: "Folder style".into(),
            photo_type: PhotoType::Portrait,
        })
        .unwrap();
    service
        .add_before_folder(AddTrainingFolderRequest {
            dataset_id: dataset.dataset_id.clone(),
            folder: before,
        })
        .unwrap();
    service
        .add_after_folder(AddTrainingFolderRequest {
            dataset_id: dataset.dataset_id,
            folder: after,
        })
        .unwrap()
}

#[test]
fn all_47_folder_images_reach_structural_validation_in_natural_order_and_persist() {
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let service = standalone_service(root.path(), None);
    let dataset = folder_dataset(root.path(), &service, 47);
    assert_eq!(
        (dataset.before_files.len(), dataset.after_files.len()),
        (47, 47)
    );
    for (i, path) in dataset.after_files.iter().enumerate() {
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            format!("Sheila ({} of 47).png", i + 1)
        );
    }
    let result = service
        .match_and_validate(&dataset.dataset_id, "47-run")
        .unwrap();
    assert_eq!(result.pairs.len(), 47);
    let alignment = result.alignment.as_ref().unwrap();
    assert_eq!(alignment.matched_count, 47);
    assert!(alignment.start_aligned && alignment.end_aligned && alignment.order_fallback_used);
    assert!(alignment.unmatched_before.is_empty() && alignment.unmatched_after.is_empty());
    let progress = service.matching_progress("47-run").unwrap().unwrap();
    assert_eq!((progress.processed, progress.total), (47, 47));
    assert_eq!(progress.status, "complete");
    assert_eq!(
        standalone_service(root.path(), None)
            .dataset(&dataset.dataset_id)
            .unwrap(),
        result
    );
    std::fs::remove_file(&result.before_files[0]).unwrap();
    assert!(service
        .match_and_validate(&dataset.dataset_id, "missing")
        .is_err());
    assert_eq!(service.dataset(&dataset.dataset_id).unwrap(), result);
    assert_eq!(
        service
            .matching_progress("missing")
            .unwrap()
            .unwrap()
            .status,
        "failed"
    );
}

struct BlockingValidation {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
impl TargetRecipeOptimizer for BlockingValidation {
    fn version(&self) -> &str {
        TARGET_OPTIMIZER_VERSION
    }
    fn validate_pair(
        &self,
        _: &TrainingPair,
        token: &CancellationToken,
    ) -> photo_contracts::ProcessingResult<PairValidation> {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        token.check()?;
        Ok(PairValidation {
            status: PairValidationStatus::Ready,
            geometry: GeometryRelationship::ExactOrNear,
            ..Default::default()
        })
    }
    fn estimate(
        &self,
        _: &TrainingPair,
        _: &CancellationToken,
    ) -> photo_contracts::ProcessingResult<TargetRecipeResult> {
        panic!("Matching must not estimate targets")
    }
    fn rendered_loss(
        &self,
        _: &TrainingPair,
        _: PredictedCreativeAdjustments,
        _: &CancellationToken,
    ) -> photo_contracts::ProcessingResult<f32> {
        panic!("Matching must not train")
    }
}

#[test]
fn matching_progress_cancellation_and_duplicate_reservation_preserve_prior_dataset() {
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let service = standalone_service(
        root.path(),
        Some(Arc::new(BlockingValidation {
            entered: entered_tx,
            release: std::sync::Mutex::new(release_rx),
        })),
    );
    let dataset = folder_dataset(root.path(), &service, 3);
    let dataset = standalone_service(root.path(), None)
        .match_and_validate(&dataset.dataset_id, "prior-valid")
        .unwrap();
    assert_eq!(dataset.alignment.as_ref().unwrap().matched_count, 3);
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| service.match_and_validate(&dataset.dataset_id, "cancel-run"));
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let progress = service.matching_progress("cancel-run").unwrap().unwrap();
        assert_eq!(progress.stage, "structural_validation");
        assert_eq!((progress.processed, progress.total), (0, 3));
        assert!(service
            .match_and_validate(&dataset.dataset_id, "duplicate")
            .is_err());
        assert!(service
            .add_before_files(photo_core::training::AddTrainingFilesRequest {
                dataset_id: dataset.dataset_id.clone(),
                paths: vec![]
            })
            .is_err());
        release_tx.send(()).unwrap();
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert_eq!(
            service
                .matching_progress("cancel-run")
                .unwrap()
                .unwrap()
                .processed,
            1
        );
        service.cancel_matching("cancel-run").unwrap();
        release_tx.send(()).unwrap();
        assert!(worker.join().unwrap().is_err());
    });
    assert_eq!(service.dataset(&dataset.dataset_id).unwrap(), dataset);
    assert_eq!(
        service
            .matching_progress("cancel-run")
            .unwrap()
            .unwrap()
            .status,
        "cancelled"
    );
}

#[test]
fn thousand_renamed_candidates_are_natural_and_count_mismatch_never_shifts() {
    let before = (1..=1000)
        .rev()
        .map(|i| format!("2H1A{}.CR3", 3374 + i).into())
        .collect::<Vec<_>>();
    let after = (1..=1000)
        .rev()
        .map(|i| format!("Sheila ({i} of 1000).jpg").into())
        .collect::<Vec<_>>();
    let matched = match_paths(&before, &after);
    assert_eq!(matched.matched.len(), 1000);
    for (i, pair) in matched.matched.iter().enumerate() {
        assert_eq!(
            pair.reference_path.to_str().unwrap(),
            format!("Sheila ({} of 1000).jpg", i + 1)
        );
    }
    let missing = match_paths(&before, &after[1..]);
    assert!(missing.matched.is_empty());
    assert_eq!(missing.unmatched_sources.len(), 1000);
    assert_eq!(missing.unmatched_references.len(), 999);
    let progress = photo_core::training::matching_task::MatchingProgress {
        request_id: "large".into(),
        dataset_id: "dataset".into(),
        status: "running".into(),
        stage: "structural_validation".into(),
        processed: 999,
        total: 1000,
        error: None,
    };
    assert!(serde_json::to_vec(&progress).unwrap().len() < 512);
}

#[test]
fn wrong_photo_is_rejected_and_manual_order_mapping_survives_rematching() {
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let service = standalone_service(root.path(), None);
    let dataset = folder_dataset(root.path(), &service, 2);
    let manual = service
        .add_path_pair(photo_core::training::AddTrainingPathPairRequest {
            dataset_id: dataset.dataset_id.clone(),
            before_path: dataset.before_files[0].clone(),
            after_path: dataset.after_files[1].clone(),
        })
        .unwrap();
    let result = service
        .match_and_validate(&dataset.dataset_id, "manual")
        .unwrap();
    assert!(result
        .pairs
        .iter()
        .any(|p| p.source_path == manual.pairs[0].source_path
            && p.reference_path == manual.pairs[0].reference_path));
    let previews = service
        .previews(&dataset.dataset_id, &result.pairs[0].pair_id)
        .unwrap();
    assert!(previews.source_data.starts_with("data:image/"));
    assert!(previews.reference_data.starts_with("data:image/"));
    assert!(previews.target_data.is_none());
    image::RgbImage::from_pixel(128, 256, image::Rgb([220, 20, 30]))
        .save(&manual.pairs[0].reference_path)
        .unwrap();
    let result = service
        .match_and_validate(&dataset.dataset_id, "wrong")
        .unwrap();
    assert_eq!(
        result.pairs[0].validation.status,
        PairValidationStatus::Rejected
    );
    assert_eq!(result.alignment.unwrap().matched_count, 0);
}

#[test]
fn concurrent_before_and_after_imports_do_not_overwrite_each_other() {
    use photo_core::training::AddTrainingFilesRequest;
    let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let service = standalone_service(root.path(), None);
    let dataset = folder_dataset(root.path(), &service, 1);
    let before = root.path().join("extra-before.png");
    let after = root.path().join("extra-after.png");
    synthetic_source(&before);
    synthetic_source(&after);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            service.add_before_files(AddTrainingFilesRequest {
                dataset_id: dataset.dataset_id.clone(),
                paths: vec![before],
            })
        });
        let b = scope.spawn(|| {
            service.add_after_files(AddTrainingFilesRequest {
                dataset_id: dataset.dataset_id.clone(),
                paths: vec![after],
            })
        });
        a.join().unwrap().unwrap();
        b.join().unwrap().unwrap();
    });
    let stored = service.dataset(&dataset.dataset_id).unwrap();
    assert_eq!(
        (stored.before_files.len(), stored.after_files.len()),
        (2, 2)
    );
}
