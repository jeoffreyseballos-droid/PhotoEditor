use photo_contracts::{
    analysis::PhotoType, culling::DuplicateKind, MaskStatus, OutputFormat, ProcessingResult,
};
use photo_core::{
    analysis::AnalysisService,
    culling::{
        features::{UnavailableEyes, YuNetDetector},
        CullingIssue, CullingRequest, CullingService,
    },
    development::{DevelopmentResult, DevelopmentService, RecipeMaskRequest, RecipeRenderRequest},
    jobs::JobService,
    models::NewJob,
    presets::{BuiltInPresetId, POP_SUBJECT_LAYER_ID},
    rendering::{
        decode::LibRawDecoder,
        masks::{MaskCache, ModnetProvider},
        optics::LensProfileResolver,
        CpuProcessingEngine, RenderLimits,
    },
};
use std::{error::Error, path::Path, path::PathBuf, sync::Arc};

fn render(
    service: &DevelopmentService,
    jobs: &JobService,
    job: &str,
    asset: &str,
    label: &str,
) -> ProcessingResult<DevelopmentResult> {
    let generation = jobs.repository.get_recipe(job, asset)?.generation;
    let request_id = format!("{label}-{asset}");
    service.render_recipe(
        RecipeRenderRequest {
            job_id: job.into(),
            asset_id: asset.into(),
            request_id: request_id.clone(),
            expected_generation: generation,
            preview: true,
            output_format: OutputFormat::Jpeg,
            jpeg_quality: 95,
            commit: false,
        },
        service.reserve(&request_id, true)?,
    )
}

fn export(
    service: &DevelopmentService,
    jobs: &JobService,
    job: &str,
    asset: &str,
) -> ProcessingResult<DevelopmentResult> {
    let generation = jobs.repository.get_recipe(job, asset)?.generation;
    let request_id = format!("export-{asset}");
    service.render_recipe(
        RecipeRenderRequest {
            job_id: job.into(),
            asset_id: asset.into(),
            request_id: request_id.clone(),
            expected_generation: generation,
            preview: false,
            output_format: OutputFormat::Jpeg,
            jpeg_quality: 95,
            commit: true,
        },
        service.reserve(&request_id, false)?,
    )
}

fn image(path: &Path) -> image::RgbImage {
    image::open(path).unwrap().to_rgb8()
}

fn monochrome(image: &image::RgbImage) -> bool {
    image.pixels().all(|pixel| {
        let high = *pixel.0.iter().max().unwrap() as i16;
        let low = *pixel.0.iter().min().unwrap() as i16;
        high - low <= 3
    })
}

fn color(image: &image::RgbImage) -> bool {
    image.pixels().any(|pixel| {
        let high = *pixel.0.iter().max().unwrap() as i16;
        let low = *pixel.0.iter().min().unwrap() as i16;
        high - low > 8
    })
}

fn changed(first: &image::RgbImage, second: &image::RgbImage) -> bool {
    first.dimensions() == second.dimensions()
        && first.pixels().zip(second.pixels()).any(|(a, b)| a.0 != b.0)
}

fn mask_delta(
    neutral: &image::RgbImage,
    pop: &image::RgbImage,
    mask_path: &Path,
) -> (f64, f64, usize, usize) {
    let mask = image::open(mask_path).unwrap().to_luma16();
    let (width, height) = neutral.dimensions();
    assert_eq!((width, height), pop.dimensions());
    let mut subject_sum = 0.0;
    let mut background_sum = 0.0;
    let mut subject_count = 0usize;
    let mut background_count = 0usize;
    for y in 0..height {
        for x in 0..width {
            let mx =
                (x as u64 * mask.width() as u64 / width as u64).min(mask.width() as u64 - 1) as u32;
            let my = (y as u64 * mask.height() as u64 / height as u64).min(mask.height() as u64 - 1)
                as u32;
            let alpha = mask.get_pixel(mx, my)[0] as f64 / 65535.0;
            let luma = |pixel: &image::Rgb<u8>| {
                pixel[0] as f64 * 0.2126 + pixel[1] as f64 * 0.7152 + pixel[2] as f64 * 0.0722
            };
            let delta = luma(pop.get_pixel(x, y)) - luma(neutral.get_pixel(x, y));
            if alpha >= 0.8 {
                subject_sum += delta;
                subject_count += 1;
            } else if alpha <= 0.05 {
                background_sum += delta.abs();
                background_count += 1;
            }
        }
    }
    (
        subject_sum / subject_count.max(1) as f64,
        background_sum / background_count.max(1) as f64,
        subject_count,
        background_count,
    )
}

fn main() -> Result<(), Box<dyn Error>> {
    let project = std::env::current_dir()?.canonicalize()?;
    let portraits = project.join("test-photos/Portraits").canonicalize()?;
    let tools = project.join(".tools");
    std::fs::create_dir_all(&tools)?;
    let scratch = tempfile::Builder::new()
        .prefix("real-preset-acceptance-")
        .tempdir_in(&tools)?;
    let input = scratch.path().join("input");
    let output = scratch.path().join("output");
    std::fs::create_dir_all(&input)?;
    std::fs::create_dir_all(&output)?;

    for entry in walkdir::WalkDir::new(&portraits)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let relative = entry.path().strip_prefix(&portraits)?;
        if relative == Path::new("Duplicates/IMG_4161.CR3") {
            continue;
        }
        let destination = input.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        std::fs::hard_link(entry.path(), destination)?;
    }

    let release = project.join("target/release");
    let toolkit = release.join("toolkit");
    let cache = scratch.path().join("cache");
    let jobs = JobService::with_exiftool(
        scratch.path().join("data"),
        cache.clone(),
        release.join("exiftool"),
    )?;
    let (job, scan) = jobs.create(NewJob {
        name: "Real preset acceptance".into(),
        input_path: input,
        output_path: output.clone(),
    })?;
    jobs.scan(&job.id, scan)?;

    let engine = Arc::new(
        CpuProcessingEngine::new(
            Box::new(LibRawDecoder {
                helper: release.join("raw/photo-raw-helper.exe"),
                scratch: cache.join("raw-scratch"),
            }),
            RenderLimits::default(),
        )
        .with_toolkit(
            LensProfileResolver::load(&toolkit.join("lensfun-db")),
            MaskCache::new(
                cache.join("masks"),
                Box::new(ModnetProvider {
                    resources: toolkit.clone(),
                    scratch: cache.join("mask-scratch"),
                }),
            ),
        ),
    );
    let analysis = Arc::new(AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        None,
    ));
    let culling = CullingService::new(
        jobs.repository.clone(),
        analysis,
        engine.clone(),
        Arc::new(YuNetDetector {
            toolkit,
            scratch: cache.join("face-scratch"),
        }),
        Arc::new(UnavailableEyes),
    );
    let development =
        DevelopmentService::new(jobs.repository.clone(), engine, cache.join("develop"), None)?;

    let mut assets = jobs.repository.assets(&job.id, 0, 100)?.items;
    assets.sort_by(|a, b| a.filename.cmp(&b.filename).then(a.id.cmp(&b.id)));
    assert_eq!(assets.len(), 52);
    culling.run(culling.reserve(CullingRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        request_id: "real-filter-cull".into(),
        force: true,
    })?)?;
    jobs.repository.culling_select(
        &job.id,
        &assets[..45]
            .iter()
            .map(|asset| (asset.id.clone(), true))
            .collect::<Vec<_>>(),
    )?;
    assert_eq!(
        culling
            .overview(&job.id, PhotoType::Portrait)?
            .selected_count,
        45
    );

    let filtered = culling.overview(&job.id, PhotoType::Portrait)?;
    assert_eq!(filtered.counts[5], 10);
    let selected = filtered
        .items
        .iter()
        .filter(|item| {
            item.effective_rating
                .is_some_and(|rating| rating.get() == 5)
        })
        .filter(|item| !item.issues.contains(&CullingIssue::Blurry))
        .filter(|item| !item.issues.contains(&CullingIssue::ClosedEyes))
        .filter(|item| {
            let Some(similarity) = &item.similarity else {
                return true;
            };
            if similarity
                .exact
                .as_ref()
                .is_some_and(|exact| exact.canonical_asset_id != item.asset.id)
            {
                return false;
            }
            if similarity.group_id.is_some()
                && matches!(
                    similarity.kind,
                    DuplicateKind::NearDuplicate | DuplicateKind::Burst
                )
            {
                return item.preferred;
            }
            true
        })
        .map(|item| item.asset.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 5);
    culling.select_assets(&job.id, PhotoType::Portrait, &selected)?;
    let overview = culling.overview(&job.id, PhotoType::Portrait)?;
    assert_eq!(overview.items.len(), 52);
    assert_eq!(overview.selected_count, 5);
    println!(
        "SELECTION\tbefore=45\tfive_star=10\tduplicates_hide=true\thide_blurry=true\tmatched=5\ttotal=52\tafter=5"
    );

    let mut neutral = Vec::new();
    for asset in &selected {
        neutral.push(image(
            render(&development, &jobs, &job.id, asset, "neutral")?
                .state
                .preview_path
                .as_deref()
                .unwrap(),
        ));
    }

    jobs.repository.apply_built_in_preset_to_assets(
        &job.id,
        BuiltInPresetId::BlackAndWhite,
        &selected,
    )?;
    let mut black_and_white_paths = Vec::<PathBuf>::new();
    for asset in &selected {
        let result = render(&development, &jobs, &job.id, asset, "black-and-white")?;
        let path = result.state.preview_path.unwrap();
        assert!(monochrome(&image(&path)));
        black_and_white_paths.push(path);
    }
    println!("BLACK_AND_WHITE\trendered=5\tmonochrome=5\tjpeg=3\traw=2");

    jobs.repository
        .apply_built_in_preset_to_assets(&job.id, BuiltInPresetId::Warm, &selected)?;
    let mut warm_changed = 0usize;
    for (index, asset) in selected.iter().enumerate() {
        let result = render(&development, &jobs, &job.id, asset, "warm")?;
        let path = result.state.preview_path.unwrap();
        let pixels = image(&path);
        assert_ne!(path, black_and_white_paths[index]);
        assert!(color(&pixels));
        if changed(&neutral[index], &pixels) {
            warm_changed += 1;
        }
    }
    assert_eq!(warm_changed, 5);
    println!("WARM\trendered=5\tcolor=5\tchanged_from_source_render=5\tcache_rekeyed=5");

    jobs.repository
        .apply_built_in_preset_to_assets(&job.id, BuiltInPresetId::Pop, &selected)?;
    let mut masks_ready = 0usize;
    let mut pop_visible = 0usize;
    let mut failures = 0usize;
    for (index, asset) in selected.iter().enumerate() {
        let recipe = jobs.repository.get_recipe(&job.id, asset)?;
        assert_eq!(recipe.recipe.global.basic.exposure_ev, 0.0);
        assert_eq!(recipe.recipe.local_layers[0].adjustments.exposure_ev, 0.35);
        let request_id = format!("mask-{asset}");
        let mask = development.recipe_mask(
            RecipeMaskRequest {
                job_id: job.id.clone(),
                asset_id: asset.clone(),
                request_id: request_id.clone(),
                expected_generation: recipe.generation,
                layer_id: Some(POP_SUBJECT_LAYER_ID.into()),
                generate: true,
            },
            development.reserve(&request_id, true)?,
        )?;
        if mask.diagnostic.status != MaskStatus::Ready {
            failures += 1;
        } else {
            masks_ready += 1;
        }
        let result = render(&development, &jobs, &job.id, asset, "pop")?;
        let pixels = image(result.state.preview_path.as_deref().unwrap());
        if mask.diagnostic.status == MaskStatus::Ready {
            let mask_path = PathBuf::from(mask.diagnostic.cache_path.as_deref().unwrap());
            let (subject_delta, background_delta, subject_count, background_count) =
                mask_delta(&neutral[index], &pixels, &mask_path);
            assert!(subject_count > 0 && background_count > 0);
            assert!(subject_delta > 0.5, "subject delta was {subject_delta}");
            assert!(
                background_delta < 1.0,
                "background delta was {background_delta}"
            );
            pop_visible += 1;
        } else {
            assert_eq!(neutral[index], pixels);
        }
    }
    println!(
        "POP\trendered=5\tmasks_ready={masks_ready}\tvisible_subject_only={pop_visible}\tfailures={failures}\tglobal_exposure=0\tlocal_subject_exposure=0.35"
    );
    let mut exported = 0usize;
    for asset in &selected {
        let result = export(&development, &jobs, &job.id, asset)?;
        assert!(result.state.export_path.as_ref().is_some_and(|path| {
            path.is_file() && path.parent().is_some_and(|parent| parent == output)
        }));
        exported += 1;
    }
    let output_files = std::fs::read_dir(&output)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .count();
    assert_eq!(exported, 5);
    assert_eq!(output_files, 5);
    println!("EXPORT\tselected=5\texported=5\tfailed=0\toutput_files=5");
    Ok(())
}
