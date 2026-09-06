use photo_contracts::{
    analysis::{PhotoAnalysis, PhotoType},
    batch_context::*,
    trained_style::{StyleModelKind, StyleResolver},
    EditRecipe, RecipeOrigin,
};
use photo_core::{
    analysis::{AnalysisRequest, AnalysisService},
    batch_context::BatchContextService,
    jobs::JobService,
    models::NewJob,
    presets::{resolve_built_in_preset, BuiltInPresetId},
    rendering::{
        decode::{Decoded, RawDecoder},
        CpuProcessingEngine, RenderLimits,
    },
    repository::JobRepository,
    trained_styles::{
        features::build_features,
        package::{load_style_package, validate_loaded_package},
        resolve_prediction_to_recipe,
        resolver::LinearStyleResolver,
        StyleApplyRequest, TrainedStyleService,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

struct NoRaw;

impl RawDecoder for NoRaw {
    fn id(&self) -> &str {
        "trained-style-test"
    }

    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &photo_contracts::CancellationToken,
    ) -> photo_contracts::ProcessingResult<Decoded> {
        panic!("PNG test input must use the raster decoder")
    }
}

struct FailOneResolver {
    asset_id: String,
}

impl StyleResolver for FailOneResolver {
    fn backend_id(&self) -> &str {
        "trained-style-test-failure"
    }

    fn resolve(
        &self,
        package: &photo_contracts::trained_style::LoadedStylePackage,
        features: &photo_contracts::trained_style::StyleFeatureVector,
    ) -> Result<
        photo_contracts::trained_style::StylePrediction,
        photo_contracts::trained_style::StyleError,
    > {
        if features.asset_id == self.asset_id {
            return Err(
                photo_contracts::trained_style::StyleError::InvalidPrediction(
                    "Synthetic per-asset resolver failure".into(),
                ),
            );
        }
        LinearStyleResolver.resolve(package, features)
    }
}

fn style_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("styles/adaptive-natural-development")
}

fn style_root() -> PathBuf {
    style_directory().parent().unwrap().to_path_buf()
}

fn analysis(asset: &str, median: f64, warm: f64) -> PhotoAnalysis {
    let mut analysis =
        PhotoAnalysis::parse(include_str!("../../../src/test/analysis-fixture.json")).unwrap();
    analysis.asset_id = asset.into();
    analysis.analysis_id = format!("analysis-{asset}");
    analysis.source_fingerprint = "a".repeat(64);
    analysis.common.exposure.median_luminance = median;
    analysis.common.exposure.percentiles.p01 = (median * 0.8).clamp(0.0, 1.0);
    analysis.common.exposure.percentiles.p05 = (median * 0.8).clamp(0.0, 1.0);
    analysis.common.exposure.percentiles.p25 = (median * 0.9).clamp(0.0, 1.0);
    analysis.common.exposure.percentiles.p50 = median;
    analysis.common.exposure.percentiles.p75 = (median + 0.1).clamp(median, 1.0);
    analysis.common.exposure.percentiles.p95 = (median + 0.2).clamp(median, 1.0);
    analysis.common.exposure.percentiles.p99 = (median + 0.2).clamp(median, 1.0);
    analysis.common.color.warm_cool_balance = warm;
    analysis
}

fn context(asset: &str, exposure_delta: f64, warm_delta: f64) -> BatchContext {
    let scene = "1".repeat(64);
    let lighting = "2".repeat(64);
    BatchContext {
        schema_version: BATCH_CONTEXT_SCHEMA_VERSION,
        batch_id: "b".repeat(64),
        job_id: "job".into(),
        photo_type: PhotoType::Portrait,
        selected_asset_ids: vec![asset.into()],
        selection_identity: "c".repeat(64),
        created_at: "2026-09-05T00:00:00Z".into(),
        analysis_version: "analysis-v1".into(),
        grouping_version: "grouping-v1".into(),
        scene_groups: vec![BatchGroup {
            group_id: scene.clone(),
            asset_ids: vec![asset.into()],
            confidence: 0.85,
            reference_candidate_ids: vec![],
        }],
        lighting_groups: vec![BatchGroup {
            group_id: lighting.clone(),
            asset_ids: vec![asset.into()],
            confidence: 0.85,
            reference_candidate_ids: vec![],
        }],
        sequence_groups: vec![],
        asset_contexts: vec![AssetBatchContext {
            asset_id: asset.into(),
            availability: ContextAvailability::Available,
            scene_group_id: Some(scene),
            lighting_group_id: Some(lighting),
            sequence_group_id: None,
            reference_asset_id: None,
            exposure_delta_from_group: Some(ExposureRelationship {
                delta_ev: exposure_delta,
                confidence: 0.8,
            }),
            wb_delta_from_group: Some(WhiteBalanceRelationship {
                warm_cool_delta: warm_delta,
                green_magenta_delta: 0.0,
                confidence: 0.8,
            }),
            group_confidence: 0.85,
            consistency_notes: vec![ConsistencyNote {
                code: ConsistencyNoteCode::NearExposureMedian,
                message: "Test source relationship".into(),
            }],
        }],
        reference_candidates: vec![],
        diagnostics: BatchDiagnostics {
            available_assets: 1,
            partial_assets: 0,
            unavailable_assets: 0,
            candidate_comparisons: 0,
            candidate_limit_per_asset: 64,
            timings: BatchStageTimings::default(),
            warnings: vec![],
        },
    }
}

fn predict(
    analysis: &PhotoAnalysis,
    context: &BatchContext,
) -> photo_contracts::trained_style::StylePrediction {
    let package = load_style_package(&style_directory()).unwrap();
    let features = build_features(analysis, &context.asset_contexts[0], &context.batch_id).unwrap();
    LinearStyleResolver.resolve(&package, &features).unwrap()
}

#[test]
fn valid_package_loads_and_corruption_is_detected() {
    let package = load_style_package(&style_directory()).unwrap();
    assert_eq!(package.manifest.style_id, "adaptive-natural-development");
    assert!(package.metadata.development_only);
    assert!(!package.metadata.trained_from_user_photos);

    let temp = tempfile::tempdir().unwrap();
    for name in [
        "style.json",
        "model.json",
        "rules.json",
        "metadata.json",
        "checksums.json",
    ] {
        std::fs::copy(style_directory().join(name), temp.path().join(name)).unwrap();
    }
    std::fs::write(
        temp.path().join("metadata.json"),
        r#"{"schema_version":1,"description":"tampered"}"#,
    )
    .unwrap();
    assert!(load_style_package(temp.path()).is_err());
}

#[test]
fn unsupported_package_and_feature_versions_fail_before_inference() {
    let mut package = load_style_package(&style_directory()).unwrap();
    package.manifest.package_schema_version = 2;
    assert!(validate_loaded_package(&package).is_err());

    let mut package = load_style_package(&style_directory()).unwrap();
    package.model.feature_schema = "future-feature-schema".into();
    assert!(validate_loaded_package(&package).is_err());
}

#[test]
fn same_style_is_adaptive_for_dark_bright_warm_and_cool_sources() {
    let dark = analysis("dark", 0.12, 0.0);
    let bright = analysis("bright", 0.78, 0.0);
    let dark_prediction = predict(&dark, &context("dark", -0.6, 0.0));
    let bright_prediction = predict(&bright, &context("bright", 0.3, 0.0));
    assert!(
        dark_prediction.adjustments.exposure_ev > bright_prediction.adjustments.exposure_ev + 0.5
    );

    let warm = analysis("warm", 0.4, 0.28);
    let cool = analysis("cool", 0.4, -0.28);
    let warm_prediction = predict(&warm, &context("warm", 0.0, 0.18));
    let cool_prediction = predict(&cool, &context("cool", 0.0, -0.18));
    assert!(warm_prediction.adjustments.temperature_delta < 0.0);
    assert!(cool_prediction.adjustments.temperature_delta > 0.0);
}

#[test]
fn three_frame_scene_gets_individual_directionally_consistent_recipes() {
    let frames = [(-0.8, 0.18), (-0.3, 0.32), (0.0, 0.46)];
    let predictions = frames
        .iter()
        .enumerate()
        .map(|(index, (delta, median))| {
            let id = format!("frame-{index}");
            predict(&analysis(&id, *median, 0.0), &context(&id, *delta, 0.0))
        })
        .collect::<Vec<_>>();
    assert!(predictions[0].adjustments.exposure_ev > predictions[1].adjustments.exposure_ev);
    assert!(predictions[1].adjustments.exposure_ev > predictions[2].adjustments.exposure_ev);
    assert!(predictions
        .iter()
        .all(|prediction| prediction.adjustments.exposure_ev.is_finite()));
}

#[test]
fn batch_context_changes_prediction_without_forcing_identical_edits() {
    let source = analysis("frame", 0.35, 0.0);
    let darker_in_group = predict(&source, &context("frame", -0.7, 0.0));
    let brighter_in_group = predict(&source, &context("frame", 0.4, 0.0));
    assert!(darker_in_group.adjustments.exposure_ev > brighter_in_group.adjustments.exposure_ev);
}

#[test]
fn output_bounds_are_enforced_and_nonfinite_model_output_is_rejected() {
    let source = analysis("bounded", 0.4, 0.0);
    let context = context("bounded", 0.0, 0.0);
    let features = build_features(&source, &context.asset_contexts[0], &context.batch_id).unwrap();
    let mut package = load_style_package(&style_directory()).unwrap();
    let StyleModelKind::LinearV1(model) = &mut package.model.model;
    model.outputs[0].intercept = 500.0;
    let prediction = LinearStyleResolver.resolve(&package, &features).unwrap();
    assert_eq!(prediction.adjustments.exposure_ev, 1.5);
    assert!(prediction
        .diagnostics
        .bounded_controls
        .contains(&photo_contracts::trained_style::StyleControl::ExposureEv));

    let StyleModelKind::LinearV1(model) = &mut package.model.model;
    model.outputs[0].intercept = f32::NAN;
    assert!(LinearStyleResolver.resolve(&package, &features).is_err());
}

#[test]
fn prediction_becomes_a_valid_idempotent_recipe_and_preserves_objective_controls() {
    let package = load_style_package(&style_directory()).unwrap();
    let analysis = analysis("asset", 0.16, 0.12);
    let context = context("asset", -0.5, 0.08);
    let features =
        build_features(&analysis, &context.asset_contexts[0], &context.batch_id).unwrap();
    let prediction = LinearStyleResolver.resolve(&package, &features).unwrap();
    let mut original = EditRecipe::neutral(
        "recipe".into(),
        "asset".into(),
        "2026-09-05T00:00:00Z".into(),
    );
    original.global.optics.enabled = true;
    original.global.geometry.rotation_degrees = 90.0;
    original.local_layers = resolve_built_in_preset(&original, BuiltInPresetId::Pop)
        .unwrap()
        .local_layers;
    let first = resolve_prediction_to_recipe(
        &original,
        &package,
        &analysis,
        &context,
        &context.asset_contexts[0],
        &prediction,
    )
    .unwrap();
    let repeated = resolve_prediction_to_recipe(
        &first,
        &package,
        &analysis,
        &context,
        &context.asset_contexts[0],
        &prediction,
    )
    .unwrap();
    assert_eq!(first, repeated);
    assert_eq!(first.provenance.origin, RecipeOrigin::TrainedStyle);
    assert_eq!(
        first.provenance.style_id.as_deref(),
        Some("adaptive-natural-development")
    );
    assert_eq!(
        first.provenance.batch_context_id.as_deref(),
        Some(context.batch_id.as_str())
    );
    assert_eq!(
        first.provenance.analysis_id.as_deref(),
        Some(analysis.analysis_id.as_str())
    );
    assert!(first.global.optics.enabled);
    assert_eq!(first.global.geometry.rotation_degrees, 90.0);
    assert!(first.local_layers.is_empty());
    first.validated().unwrap();

    let warm = resolve_built_in_preset(&first, BuiltInPresetId::Warm).unwrap();
    assert_eq!(warm.provenance.origin, RecipeOrigin::System);
    assert_eq!(warm.global.basic.temperature, 7000.0);
    assert_eq!(warm.global.basic.exposure_ev, 0.0);
}

#[test]
fn service_applies_style_to_exactly_the_five_selected_assets() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for index in 0..52 {
        image::RgbImage::from_fn(128, 64, |x, y| {
            image::Rgb([
                40u8.saturating_add((index % 40) as u8),
                70u8.saturating_add((x / 8) as u8),
                90u8.saturating_add((y / 4) as u8),
            ])
        })
        .save(input.join(format!("photo-{index:02}.png")))
        .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Trained style selection scope".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&job.id, permit).unwrap();
    let ids = jobs
        .repository
        .assets(&job.id, 0, 100)
        .unwrap()
        .items
        .into_iter()
        .map(|asset| asset.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 52);
    let baseline = ids
        .iter()
        .map(|asset| jobs.repository.get_recipe(&job.id, asset).unwrap())
        .collect::<Vec<_>>();
    jobs.repository
        .culling_select(
            &job.id,
            &ids.iter()
                .enumerate()
                .map(|(index, asset)| (asset.clone(), index < 5))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        jobs.repository.selected_editing_asset_ids(&job.id).unwrap(),
        ids[..5]
    );

    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ));
    let analysis = Arc::new(AnalysisService::new(jobs.repository.clone(), engine, None));
    for asset in &ids[..5] {
        let request = AnalysisRequest {
            job_id: job.id.clone(),
            asset_id: asset.clone(),
            photo_type: PhotoType::Portrait,
            request_id: format!("analysis-{asset}"),
        };
        analysis
            .analyze_asset(analysis.reserve(request).unwrap())
            .unwrap();
    }
    let batch_context = Arc::new(BatchContextService::new(
        jobs.repository.clone(),
        analysis.clone(),
    ));
    let service = TrainedStyleService::new(
        jobs.repository.clone(),
        analysis,
        batch_context,
        &style_root(),
    )
    .unwrap();
    let result = service
        .apply(
            service
                .reserve(StyleApplyRequest {
                    job_id: job.id.clone(),
                    photo_type: PhotoType::Portrait,
                    style_id: "adaptive-natural-development".into(),
                    selected_asset_ids: ids[..5].to_vec(),
                    request_id: "style-selection-scope".into(),
                })
                .unwrap(),
        )
        .unwrap();

    assert_eq!(result.selected_asset_ids, ids[..5]);
    assert_eq!(result.predictions_attempted, 5);
    assert_eq!(result.predictions_succeeded, 5);
    assert_eq!(result.predictions_failed, 0);
    assert_eq!(result.recipes_updated, 5);
    assert!(result.needs_review.is_empty());
    assert_eq!(result.inferences.len(), 5);
    for asset in &ids[..5] {
        let recipe = jobs.repository.get_recipe(&job.id, asset).unwrap();
        assert_eq!(
            recipe.recipe.provenance.style_id.as_deref(),
            Some("adaptive-natural-development")
        );
        assert_eq!(recipe.recipe.provenance.origin, RecipeOrigin::TrainedStyle);
    }
    for (index, asset) in ids.iter().enumerate().skip(5) {
        let unchanged = jobs.repository.get_recipe(&job.id, asset).unwrap();
        assert_eq!(unchanged.generation, baseline[index].generation);
        assert_eq!(unchanged.recipe_hash, baseline[index].recipe_hash);
        assert_eq!(unchanged.recipe, baseline[index].recipe);
    }
    let reopened = JobRepository::open(root.path().join("data/jobs.sqlite3")).unwrap();
    let reopened_engine = Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ));
    let reopened_analysis = Arc::new(AnalysisService::new(
        reopened.clone(),
        reopened_engine,
        None,
    ));
    let reopened_context = Arc::new(BatchContextService::new(
        reopened.clone(),
        reopened_analysis.clone(),
    ));
    let reopened_service =
        TrainedStyleService::new(reopened, reopened_analysis, reopened_context, &style_root())
            .unwrap();
    let state = reopened_service
        .state(&job.id, PhotoType::Portrait)
        .unwrap();
    assert_eq!(
        state.applied_style.unwrap().style_id,
        "adaptive-natural-development"
    );
    assert_eq!(state.applied_count, 5);
    assert!(state.stale_asset_ids.is_empty());
    assert_eq!(state.inferences.len(), 5);
}

#[test]
fn one_failed_prediction_is_nonfatal_and_leaves_that_recipe_untouched() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for index in 0..3 {
        image::RgbImage::from_pixel(128, 64, image::Rgb([60 + index * 20, 80, 100]))
            .save(input.join(format!("photo-{index:02}.png")))
            .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Trained style failure continuation".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&job.id, permit).unwrap();
    let ids = jobs
        .repository
        .assets(&job.id, 0, 100)
        .unwrap()
        .items
        .into_iter()
        .map(|asset| asset.id)
        .collect::<Vec<_>>();
    let baseline = ids
        .iter()
        .map(|asset| jobs.repository.get_recipe(&job.id, asset).unwrap())
        .collect::<Vec<_>>();
    jobs.repository
        .culling_select(
            &job.id,
            &ids.iter()
                .map(|asset| (asset.clone(), true))
                .collect::<Vec<_>>(),
        )
        .unwrap();

    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ));
    let analysis = Arc::new(AnalysisService::new(jobs.repository.clone(), engine, None));
    for asset in &ids {
        let request = AnalysisRequest {
            job_id: job.id.clone(),
            asset_id: asset.clone(),
            photo_type: PhotoType::Portrait,
            request_id: format!("analysis-{asset}"),
        };
        analysis
            .analyze_asset(analysis.reserve(request).unwrap())
            .unwrap();
    }
    let batch_context = Arc::new(BatchContextService::new(
        jobs.repository.clone(),
        analysis.clone(),
    ));
    let catalog =
        photo_core::trained_styles::package::LocalStyleCatalog::load(&style_root()).unwrap();
    let failed_asset = ids[1].clone();
    let service = TrainedStyleService::with_resolver(
        jobs.repository.clone(),
        analysis,
        batch_context,
        catalog,
        Arc::new(FailOneResolver {
            asset_id: failed_asset.clone(),
        }),
    );
    let result = service
        .apply(
            service
                .reserve(StyleApplyRequest {
                    job_id: job.id.clone(),
                    photo_type: PhotoType::Portrait,
                    style_id: "adaptive-natural-development".into(),
                    selected_asset_ids: ids.clone(),
                    request_id: "style-failure-continuation".into(),
                })
                .unwrap(),
        )
        .unwrap();

    assert_eq!(result.predictions_attempted, 3);
    assert_eq!(result.predictions_succeeded, 2);
    assert_eq!(result.predictions_failed, 1);
    assert_eq!(result.recipes_updated, 2);
    assert_eq!(result.needs_review, vec![failed_asset.clone()]);
    assert_eq!(result.inferences.len(), 3);
    assert_eq!(
        result
            .inferences
            .iter()
            .find(|inference| inference.asset_id == failed_asset)
            .unwrap()
            .status,
        "failed"
    );
    let failed_recipe = jobs.repository.get_recipe(&job.id, &failed_asset).unwrap();
    assert_eq!(failed_recipe.generation, baseline[1].generation);
    assert_eq!(failed_recipe.recipe_hash, baseline[1].recipe_hash);
    assert_eq!(failed_recipe.recipe, baseline[1].recipe);
}
