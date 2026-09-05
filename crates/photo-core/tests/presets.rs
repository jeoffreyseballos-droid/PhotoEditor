use photo_contracts::{
    BasicAdjustments, CancellationToken, EditRecipe, LocalAdjustments, OutputFormat,
    ProcessingResult, RecipeGlobal, RecipeOrigin, RenderAdjustments, RenderRequest,
};
use photo_core::{
    development::{DevelopmentService, RecipeRenderRequest},
    jobs::JobService,
    models::NewJob,
    presets::*,
    rendering::{
        decode::{Decoded, RawDecoder},
        masks::{MaskCache, SegmentationProvider, SoftMask},
        optics::LensProfileResolver,
        pixels::FloatImage,
        CpuProcessingEngine, RenderLimits,
    },
    repository::JobRepository,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

fn setup() -> (tempfile::TempDir, JobService, String, Vec<String>) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for name in ["a.png", "b.png", "c.png"] {
        image::RgbImage::from_pixel(64, 32, image::Rgb([100u8, 65, 40]))
            .save(input.join(name))
            .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Preset workflow".into(),
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
        .collect();
    (root, jobs, job.id, ids)
}

fn setup_count(count: usize) -> (tempfile::TempDir, JobService, String, Vec<String>) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for index in 0..count {
        image::RgbImage::from_pixel(16, 12, image::Rgb([80 + index as u8, 65, 40]))
            .save(input.join(format!("photo-{index:02}.png")))
            .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Preset scope".into(),
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
        .collect();
    (root, jobs, job.id, ids)
}

fn neutral(asset: &str) -> EditRecipe {
    EditRecipe::neutral("recipe".into(), asset.into(), "2026-01-01T00:00:00Z".into())
}

#[test]
fn definitions_and_resolver_match_the_exact_preset_contract() {
    let definitions = built_in_presets();
    assert_eq!(definitions.len(), 3);
    assert_eq!(definitions[0].id, BuiltInPresetId::Pop);
    assert_eq!(definitions[1].id, BuiltInPresetId::Warm);
    assert_eq!(definitions[2].id, BuiltInPresetId::BlackAndWhite);

    let mut source = neutral("asset");
    source.global.optics.enabled = true;
    source.global.geometry.rotation_degrees = 90.0;
    source.global.basic.exposure_ev = 1.4;
    source.global.basic.vibrance = 80.0;
    source.global.presence.clarity = 50.0;

    let pop = resolve_built_in_preset(&source, BuiltInPresetId::Pop).unwrap();
    let expected_global = RecipeGlobal {
        optics: source.global.optics,
        geometry: source.global.geometry.clone(),
        ..Default::default()
    };
    assert_eq!(pop.global, expected_global);
    assert_eq!(pop.local_layers.len(), 1);
    let subject = &pop.local_layers[0];
    assert_eq!(subject.id, POP_SUBJECT_LAYER_ID);
    assert_eq!(subject.mask_type, photo_contracts::MaskType::Subject);
    assert!(subject.mask_reference.is_none());
    assert_eq!(
        subject.adjustments,
        LocalAdjustments {
            exposure_ev: 0.35,
            ..Default::default()
        }
    );

    let warm = resolve_built_in_preset(&source, BuiltInPresetId::Warm).unwrap();
    assert!(warm.local_layers.is_empty());
    assert_eq!(warm.global.basic.temperature, 7000.0);
    assert_eq!(warm.global.basic.tint, 2.0);
    assert_eq!(warm.global.basic.vibrance, 4.0);
    assert_eq!(warm.global.basic.exposure_ev, 0.0);
    let warm_expected = RecipeGlobal {
        basic: BasicAdjustments {
            temperature: 7000.0,
            tint: 2.0,
            vibrance: 4.0,
            ..Default::default()
        },
        ..expected_global.clone()
    };
    assert_eq!(warm.global, warm_expected);

    let monochrome = resolve_built_in_preset(&source, BuiltInPresetId::BlackAndWhite).unwrap();
    assert!(monochrome.local_layers.is_empty());
    assert_eq!(monochrome.global.basic.saturation, -100.0);
    let monochrome_expected = RecipeGlobal {
        basic: BasicAdjustments {
            saturation: -100.0,
            ..Default::default()
        },
        ..expected_global
    };
    assert_eq!(monochrome.global, monochrome_expected);
}

#[test]
fn pop_unresolved_mask_disables_only_the_local_adjustment() {
    struct RasterOnly;
    impl RawDecoder for RasterOnly {
        fn id(&self) -> &str {
            "preset-test"
        }
        fn decode(
            &self,
            _: &std::path::Path,
            _: bool,
            _: RenderLimits,
            _: &photo_contracts::CancellationToken,
        ) -> photo_contracts::ProcessingResult<photo_core::rendering::decode::Decoded> {
            panic!("PNG test input must use the raster decoder")
        }
    }
    let (root, jobs, job, ids) = setup();
    let source = jobs.repository.asset(&job, &ids[0]).unwrap();
    let pop = resolve_built_in_preset(&neutral(&ids[0]), BuiltInPresetId::Pop).unwrap();
    let engine = CpuProcessingEngine::new(Box::new(RasterOnly), RenderLimits::default());
    let effective = engine
        .effective_recipe(&pop, &source.original_path, &Default::default())
        .unwrap();
    assert_eq!(effective.unresolved_masks, vec![POP_SUBJECT_LAYER_ID]);
    assert_eq!(effective.adjustments.exposure_ev, 0.0);
    assert!(!effective.adjustments.local_layers[0].enabled);
    assert_eq!(
        effective.adjustments.local_layers[0]
            .adjustments
            .exposure_ev,
        0.35
    );
    drop(root);
}

#[test]
fn selected_batch_is_valid_idempotent_replaceable_and_persistent() {
    let (root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    repo.culling_select(&job, &[(ids[0].clone(), true), (ids[1].clone(), true)])
        .unwrap();

    let current = repo.get_recipe(&job, &ids[0]).unwrap();
    let mut manual = current.recipe;
    manual.global.optics.enabled = true;
    manual.global.geometry.rotation_degrees = 90.0;
    manual.global.basic.exposure_ev = 1.2;
    repo.save_recipe(&job, &ids[0], &manual, current.generation, None)
        .unwrap();

    let first = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Pop, &ids[..2])
        .unwrap();
    assert_eq!(first.selected_asset_ids, ids[..2]);
    assert_eq!(first.recipes_updated, 2);
    assert_eq!(first.recipes_unchanged, 0);
    for id in &ids[..2] {
        let state = repo.get_recipe(&job, id).unwrap();
        state.recipe.validated().unwrap();
        assert_eq!(
            applied_built_in_preset(&state.recipe),
            Some(BuiltInPresetId::Pop)
        );
        assert_eq!(state.recipe.provenance.origin, RecipeOrigin::System);
        assert_eq!(
            state.recipe.provenance.created_by.as_deref(),
            Some(BUILT_IN_PRESET_SOURCE)
        );
        assert_eq!(state.recipe.provenance.style_id.as_deref(), Some("pop"));
        assert_eq!(state.recipe.provenance.model_version.as_deref(), Some("1"));
        assert_eq!(state.recipe.global.basic.exposure_ev, 0.0);
        assert_eq!(state.recipe.local_layers[0].adjustments.exposure_ev, 0.35);
    }
    let objective = repo.get_recipe(&job, &ids[0]).unwrap();
    assert!(objective.recipe.global.optics.enabled);
    assert_eq!(objective.recipe.global.geometry.rotation_degrees, 90.0);
    let generations: Vec<_> = ids[..2]
        .iter()
        .map(|id| repo.get_recipe(&job, id).unwrap().generation)
        .collect();

    let repeated = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Pop, &ids[..2])
        .unwrap();
    assert_eq!(repeated.recipes_updated, 0);
    assert_eq!(repeated.recipes_unchanged, 2);
    assert_eq!(
        ids[..2]
            .iter()
            .map(|id| repo.get_recipe(&job, id).unwrap().generation)
            .collect::<Vec<_>>(),
        generations
    );
    for id in &ids[..2] {
        assert_eq!(
            repo.get_recipe(&job, id).unwrap().recipe.local_layers[0]
                .adjustments
                .exposure_ev,
            0.35
        );
    }

    let warm = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Warm, &ids[..2])
        .unwrap();
    assert_eq!(warm.recipes_updated, 2);
    for id in &ids[..2] {
        let recipe = repo.get_recipe(&job, id).unwrap().recipe;
        assert!(recipe.local_layers.is_empty());
        assert_eq!(recipe.global.basic.exposure_ev, 0.0);
        assert_eq!(recipe.global.basic.temperature, 7000.0);
        assert_eq!(recipe.global.basic.tint, 2.0);
        assert_eq!(recipe.global.basic.vibrance, 4.0);
    }
    assert!(applied_built_in_preset(&repo.get_recipe(&job, &ids[2]).unwrap().recipe).is_none());

    let reopened = JobRepository::open(root.path().join("data/jobs.sqlite3")).unwrap();
    let state = reopened.preset_editing_state(&job).unwrap();
    assert_eq!(state.selected_asset_ids, ids[..2]);
    assert_eq!(state.applied_preset, Some(BuiltInPresetId::Warm));
    assert_eq!(state.applied_count, 2);
    for id in &ids[..2] {
        assert_eq!(
            applied_built_in_preset(&reopened.get_recipe(&job, id).unwrap().recipe),
            Some(BuiltInPresetId::Warm)
        );
    }
}

#[test]
fn applying_requires_a_persisted_selection() {
    let (_root, jobs, job, _ids) = setup();
    let error = jobs
        .repository
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Pop, &[])
        .unwrap_err();
    assert!(error.message.contains("Select at least one"));
}

#[test]
fn explicit_five_asset_scope_replaces_old_45_and_never_mutates_the_other_47() {
    let (_root, jobs, job, ids) = setup_count(52);
    let repo = &jobs.repository;
    let baseline = ids
        .iter()
        .map(|asset| repo.get_recipe(&job, asset).unwrap())
        .collect::<Vec<_>>();
    repo.culling_select(
        &job,
        &ids[..45]
            .iter()
            .map(|asset| (asset.clone(), true))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(repo.selected_editing_asset_ids(&job).unwrap().len(), 45);

    repo.culling_select(
        &job,
        &ids.iter()
            .enumerate()
            .map(|(index, asset)| (asset.clone(), index < 5))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(repo.selected_editing_asset_ids(&job).unwrap(), ids[..5]);
    let stale = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::BlackAndWhite, &ids[..45])
        .unwrap_err();
    assert!(stale.message.contains("selection changed"));

    let black_and_white = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::BlackAndWhite, &ids[..5])
        .unwrap();
    assert_eq!(black_and_white.selected_asset_ids, ids[..5]);
    assert_eq!(black_and_white.recipes_updated, 5);
    for asset in &ids[..5] {
        assert_eq!(
            repo.get_recipe(&job, asset)
                .unwrap()
                .recipe
                .global
                .basic
                .saturation,
            -100.0
        );
    }
    for (index, asset) in ids.iter().enumerate().skip(5) {
        let unchanged = repo.get_recipe(&job, asset).unwrap();
        assert_eq!(unchanged.generation, baseline[index].generation);
        assert_eq!(unchanged.recipe_hash, baseline[index].recipe_hash);
        assert_eq!(unchanged.recipe, baseline[index].recipe);
    }

    let warm = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Warm, &ids[..5])
        .unwrap();
    assert_eq!(warm.recipes_updated, 5);
    for asset in &ids[..5] {
        let recipe = repo.get_recipe(&job, asset).unwrap().recipe;
        assert_eq!(recipe.global.basic.temperature, 7000.0);
        assert_eq!(recipe.global.basic.tint, 2.0);
        assert_eq!(recipe.global.basic.vibrance, 4.0);
    }
    for (index, asset) in ids.iter().enumerate().skip(5) {
        assert_eq!(
            repo.get_recipe(&job, asset).unwrap().recipe,
            baseline[index].recipe
        );
    }

    let pop = repo
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Pop, &ids[..5])
        .unwrap();
    assert_eq!(pop.recipes_updated, 5);
    for asset in &ids[..5] {
        let recipe = repo.get_recipe(&job, asset).unwrap().recipe;
        assert_eq!(recipe.local_layers.len(), 1);
        assert_eq!(recipe.local_layers[0].adjustments.exposure_ev, 0.35);
    }
    for (index, asset) in ids.iter().enumerate().skip(5) {
        assert_eq!(
            repo.get_recipe(&job, asset).unwrap().recipe,
            baseline[index].recipe
        );
    }
}

struct ColorRaw;
impl RawDecoder for ColorRaw {
    fn id(&self) -> &str {
        "preset-color-raw-test"
    }
    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        cancel.check()?;
        Ok(Decoded {
            image: FloatImage {
                width: 48,
                height: 32,
                pixels: vec![[0.12, 0.35, 0.7]; 48 * 32],
            },
            warnings: vec![],
        })
    }
}

struct CountingSubjectMask {
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl SegmentationProvider for CountingSubjectMask {
    fn version(&self) -> &str {
        "preset-subject-mask-v1"
    }
    fn infer(&self, _: &FloatImage, _: &CancellationToken) -> ProcessingResult<SoftMask> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail {
            return Err(photo_core::rendering::internal("No subject found"));
        }
        Ok(SoftMask {
            width: 4,
            height: 1,
            values: vec![1.0, 1.0, 0.0, 0.0],
        })
    }
}

fn assert_monochrome(path: &Path) {
    let image = image::open(path).unwrap().to_rgb8();
    assert!(image.pixels().all(|pixel| {
        let high = *pixel.0.iter().max().unwrap() as i16;
        let low = *pixel.0.iter().min().unwrap() as i16;
        high - low <= 2
    }));
}

#[test]
fn black_and_white_preview_pixels_are_monochrome_for_jpeg_and_raw_sources() {
    let root = tempfile::tempdir().unwrap();
    let jpeg = root.path().join("source.jpg");
    image::RgbImage::from_fn(80, 48, |x, y| {
        image::Rgb([(40 + x * 2) as u8, (30 + y * 3) as u8, 180])
    })
    .save(&jpeg)
    .unwrap();
    let raw = root.path().join("source.nef");
    std::fs::write(&raw, b"synthetic raw container").unwrap();
    let engine = CpuProcessingEngine::new(Box::new(ColorRaw), RenderLimits::default());
    for (index, source) in [jpeg, raw].iter().enumerate() {
        let recipe = resolve_built_in_preset(
            &neutral(&format!("asset-{index}")),
            BuiltInPresetId::BlackAndWhite,
        )
        .unwrap();
        let destination = root.path().join(format!("monochrome-{index}.jpg"));
        engine
            .render_recipe(
                &recipe,
                &RenderRequest {
                    asset_id: format!("asset-{index}"),
                    original: source.clone(),
                    adjustments: RenderAdjustments::default(),
                    source_metadata: Default::default(),
                    destination: destination.clone(),
                    output_format: OutputFormat::Jpeg,
                    preview: true,
                    jpeg_quality: 95,
                },
                &CancellationToken::default(),
            )
            .unwrap();
        assert_monochrome(&destination);
    }
}

#[test]
fn changing_black_and_white_to_warm_rekeys_the_cached_preview_and_restores_color() {
    let (root, jobs, job, ids) = setup();
    jobs.repository
        .culling_select(&job, &[(ids[0].clone(), true)])
        .unwrap();
    let development = DevelopmentService::new(
        jobs.repository.clone(),
        Arc::new(CpuProcessingEngine::new(
            Box::new(ColorRaw),
            RenderLimits::default(),
        )),
        root.path().join("preset-preview-cache"),
        None,
    )
    .unwrap();

    let render = |request_id: &str| {
        let generation = jobs
            .repository
            .get_recipe(&job, &ids[0])
            .unwrap()
            .generation;
        let request = RecipeRenderRequest {
            job_id: job.clone(),
            asset_id: ids[0].clone(),
            request_id: request_id.into(),
            expected_generation: generation,
            preview: true,
            output_format: OutputFormat::Jpeg,
            jpeg_quality: 95,
            commit: false,
        };
        development
            .render_recipe(request, development.reserve(request_id, true).unwrap())
            .unwrap()
    };

    let neutral_preview = render("neutral");
    let neutral_pixels = image::open(neutral_preview.state.preview_path.unwrap())
        .unwrap()
        .to_rgb8();

    jobs.repository
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::BlackAndWhite, &ids[..1])
        .unwrap();
    let black_and_white = render("black-and-white");
    let black_and_white_path = black_and_white.state.preview_path.unwrap();
    assert_monochrome(&black_and_white_path);

    jobs.repository
        .apply_built_in_preset_to_assets(&job, BuiltInPresetId::Warm, &ids[..1])
        .unwrap();
    let warm = render("warm");
    let warm_path = warm.state.preview_path.unwrap();
    assert_ne!(black_and_white_path, warm_path);
    let warm_pixels = image::open(&warm_path).unwrap().to_rgb8();
    assert_ne!(neutral_pixels, warm_pixels);
    assert!(warm_pixels.pixels().any(|pixel| {
        let high = *pixel.0.iter().max().unwrap() as i16;
        let low = *pixel.0.iter().min().unwrap() as i16;
        high - low > 10
    }));
}

#[test]
fn pop_generates_or_reuses_one_subject_mask_and_changes_only_subject_preview_pixels() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("portrait.jpg");
    image::RgbImage::from_pixel(96, 48, image::Rgb([80, 70, 60]))
        .save(&source)
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = CpuProcessingEngine::new(Box::new(ColorRaw), RenderLimits::default())
        .with_toolkit(
            LensProfileResolver::unavailable("test"),
            MaskCache::new(
                root.path().join("masks"),
                Box::new(CountingSubjectMask {
                    calls: calls.clone(),
                    fail: false,
                }),
            ),
        );
    let neutral = neutral("portrait");
    let pop = resolve_built_in_preset(&neutral, BuiltInPresetId::Pop).unwrap();
    assert_eq!(pop.global.basic.exposure_ev, 0.0);
    assert_eq!(pop.local_layers[0].adjustments.exposure_ev, 0.35);
    let adjustments = pop.adjustments().unwrap();
    let layer = &adjustments.local_layers[0];
    for _ in 0..2 {
        let (diagnostic, _) = engine
            .mask_preview(
                &source,
                &Default::default(),
                &adjustments,
                Some(layer),
                true,
                &CancellationToken::default(),
            )
            .unwrap();
        assert_eq!(diagnostic.status, photo_contracts::MaskStatus::Ready);
    }
    assert_eq!(calls.load(Ordering::Acquire), 1);

    let render = |name: &str, recipe: &EditRecipe| {
        let destination = root.path().join(format!("{name}.jpg"));
        engine
            .render_recipe(
                recipe,
                &RenderRequest {
                    asset_id: "portrait".into(),
                    original: source.clone(),
                    adjustments: Default::default(),
                    source_metadata: Default::default(),
                    destination: destination.clone(),
                    output_format: OutputFormat::Jpeg,
                    preview: true,
                    jpeg_quality: 100,
                },
                &CancellationToken::default(),
            )
            .unwrap();
        image::open(destination).unwrap().to_rgb8()
    };
    let original = render("neutral", &neutral);
    let edited = render("pop", &pop);
    assert!(edited.get_pixel(12, 24)[0] > original.get_pixel(12, 24)[0] + 8);
    for channel in 0..3 {
        let before = original.get_pixel(84, 24)[channel] as i16;
        let after = edited.get_pixel(84, 24)[channel] as i16;
        assert!((before - after).abs() <= 2);
    }
}

#[test]
fn failed_pop_mask_leaves_the_rendered_photo_unchanged_without_global_fallback() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("portrait.jpg");
    image::RgbImage::from_pixel(64, 32, image::Rgb([95, 70, 45]))
        .save(&source)
        .unwrap();
    let engine = CpuProcessingEngine::new(Box::new(ColorRaw), RenderLimits::default())
        .with_toolkit(
            LensProfileResolver::unavailable("test"),
            MaskCache::new(
                root.path().join("masks"),
                Box::new(CountingSubjectMask {
                    calls: Arc::new(AtomicUsize::new(0)),
                    fail: true,
                }),
            ),
        );
    let neutral = neutral("portrait");
    let pop = resolve_built_in_preset(&neutral, BuiltInPresetId::Pop).unwrap();
    let adjustments = pop.adjustments().unwrap();
    let (diagnostic, _) = engine
        .mask_preview(
            &source,
            &Default::default(),
            &adjustments,
            Some(&adjustments.local_layers[0]),
            true,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(diagnostic.status, photo_contracts::MaskStatus::Failed);
    let effective = engine
        .effective_recipe(&pop, &source, &Default::default())
        .unwrap();
    assert_eq!(effective.adjustments.exposure_ev, 0.0);
    assert!(!effective.adjustments.local_layers[0].enabled);

    let render = |name: &str, recipe: &EditRecipe| {
        let destination = root.path().join(format!("{name}.jpg"));
        engine
            .render_recipe(
                recipe,
                &RenderRequest {
                    asset_id: "portrait".into(),
                    original: source.clone(),
                    adjustments: Default::default(),
                    source_metadata: Default::default(),
                    destination: destination.clone(),
                    output_format: OutputFormat::Jpeg,
                    preview: true,
                    jpeg_quality: 100,
                },
                &CancellationToken::default(),
            )
            .unwrap();
        image::open(destination).unwrap().to_rgb8()
    };
    assert_eq!(render("neutral", &neutral), render("failed-pop", &pop));
}
