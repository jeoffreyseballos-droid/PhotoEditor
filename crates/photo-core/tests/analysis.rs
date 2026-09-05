use photo_contracts::{analysis::*, *};
use photo_core::{
    analysis::{cache_identity, measure, AnalysisRequest, AnalysisService, AnalysisStatus},
    jobs::JobService,
    models::NewJob,
    rendering::{
        decode::{Decoded, RawDecoder},
        masks::{MaskCache, SegmentationProvider, SoftMask},
        pixels::FloatImage,
        CpuProcessingEngine, RenderLimits,
    },
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 3]) -> FloatImage {
    FloatImage {
        width: w,
        height: h,
        pixels: (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect(),
    }
}
fn source(i: &FloatImage) -> AnalysisSource {
    AnalysisSource {
        width: i.width,
        height: i.height,
        metadata_width: None,
        metadata_height: None,
        exif_orientation: None,
        camera_make: None,
        camera_model: None,
        lens: None,
        focal_length: None,
        aperture: None,
        shutter_speed: None,
        iso: None,
        capture_timestamp: None,
        raw: false,
        color_representation: "linear-sRGB".into(),
        decoder: "test".into(),
    }
}
fn measured(i: &FloatImage) -> CommonAnalysis {
    measure::measure(i, source(i), vec![], &CancellationToken::default()).unwrap()
}
#[test]
fn exposure_clipping_percentiles_and_thresholds() {
    let dark = measured(&image(64, 64, |_, _| [0.01; 3]));
    let bright = measured(&image(64, 64, |_, _| [0.9; 3]));
    assert!(dark.exposure.mean_luminance < bright.exposure.mean_luminance);
    assert!(dark.exposure.median_luminance < bright.exposure.median_luminance);
    let black = measured(&image(64, 64, |_, _| [0.; 3]));
    let white = measured(&image(64, 64, |_, _| [1.; 3]));
    assert_eq!(black.exposure.shadow_clip_fraction, 1.);
    assert_eq!(white.exposure.highlight_clip_fraction, 1.);
    let clipped = measured(&image(64, 64, |x, _| {
        if x < 16 {
            [0.; 3]
        } else if x >= 48 {
            [1.; 3]
        } else {
            [0.3; 3]
        }
    }));
    assert_eq!(clipped.exposure.shadow_clip_fraction, 0.25);
    assert_eq!(clipped.exposure.highlight_clip_fraction, 0.25);
    let p = clipped.exposure.percentiles;
    assert!([p.p01, p.p05, p.p25, p.p50, p.p75, p.p95, p.p99]
        .windows(2)
        .all(|p| p[0] <= p[1] && p[0].is_finite()));
    for (v, class) in [
        (0.0249, ExposureClass::StronglyUnderexposed),
        (0.025, ExposureClass::Underexposed),
        (0.0999, ExposureClass::Underexposed),
        (0.1, ExposureClass::Balanced),
        (0.55, ExposureClass::Overexposed),
        (0.8, ExposureClass::StronglyOverexposed),
    ] {
        assert_eq!(measure::exposure_class(v), class);
    }
}
#[test]
fn dynamic_range_and_color_measure_conditions_not_edit_decisions() {
    let low = measured(&image(64, 64, |x, _| [0.25 + x as f32 / 6400.; 3]));
    let high = measured(&image(64, 64, |x, _| [x as f32 / 63.; 3]));
    assert!(high.dynamic_range.percentile_range > low.dynamic_range.percentile_range);
    let warm = measured(&image(64, 64, |_, _| [0.7, 0.4, 0.1]));
    let cool = measured(&image(64, 64, |_, _| [0.1, 0.4, 0.7]));
    let neutral = measured(&image(64, 64, |_, _| [0.4; 3]));
    assert!(warm.color.warm_cool_balance > 0.2);
    assert!(cool.color.warm_cool_balance < -0.2);
    assert!(neutral.color.warm_cool_balance.abs() < 1e-8);
    let green = measured(&image(64, 64, |_, _| [0.2, 0.7, 0.2]));
    let magenta = measured(&image(64, 64, |_, _| [0.7, 0.2, 0.7]));
    assert!(green.color.green_magenta_balance > 0.);
    assert!(magenta.color.green_magenta_balance < 0.);
    assert!(warm.color.mean_saturation > neutral.color.mean_saturation);
}
#[test]
fn proxy_noise_and_sharpness_are_finite_and_ordered() {
    let clean = measured(&image(128, 128, |_, _| [0.3; 3]));
    let noisy = measured(&image(128, 128, |x, y| {
        let seed = (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)).wrapping_mul(83492791);
        let r = ((seed % 1000) as f32 / 999. - 0.5) * 0.06;
        [0.3 + r, 0.3 - r / 2., 0.3 + r / 3.]
    }));
    let n = noisy.detail.noise.value().unwrap();
    assert!(n.luminance_sigma > clean.detail.noise.value().unwrap().luminance_sigma);
    assert!(n.chroma_sigma > 0.);
    assert!((0. ..=1.).contains(&n.severity));
    let sharp = measured(&image(
        128,
        128,
        |x, _| if x < 64 { [0.1; 3] } else { [0.9; 3] },
    ));
    let blurred = measured(&image(128, 128, |x, _| {
        [0.1 + 0.8 * ((x as f32 - 48.) / 32.).clamp(0., 1.); 3]
    }));
    assert!(sharp.detail.laplacian_rms > blurred.detail.laplacian_rms * 3.);
    assert!(matches!(
        sharp.detail.blur_likelihood,
        Observation::Unavailable { .. }
    ));
}
#[test]
fn horizon_sign_support_and_absence() {
    for degrees in [0f64, 2., -3.] {
        let i = image(320, 200, |x, y| {
            if y as f64 > 100. + (x as f64 - 160.) * degrees.to_radians().tan() {
                [0.1; 3]
            } else {
                [0.8; 3]
            }
        });
        let found = measure::line(&i, false, &CancellationToken::default()).unwrap();
        let line = found
            .value()
            .unwrap_or_else(|| panic!("missing {degrees}: {found:?}"));
        assert!(
            (line.angle_degrees - degrees).abs() <= 0.6,
            "{degrees} -> {line:?}"
        );
        assert!((line.position - 0.5).abs() < 0.03);
        assert!(line.support_fraction > 0.35);
    }
    assert!(matches!(
        measure::line(
            &image(100, 100, |_, _| [0.3; 3]),
            false,
            &CancellationToken::default()
        )
        .unwrap(),
        Observation::Unavailable { .. }
    ));
}
#[test]
fn subject_geometry_relationships_and_no_mask_pixels() {
    let i = image(100, 100, |x, _| if x < 50 { [0.1; 3] } else { [0.5; 3] });
    let mask = SoftMask {
        width: 100,
        height: 100,
        values: (0..10000)
            .map(|i| if i % 100 < 50 { 1. } else { 0. })
            .collect(),
    };
    let s = measure::subject(&i, &mask, "mask-id".into(), &CancellationToken::default()).unwrap();
    let m = s.measurements.value().unwrap();
    assert!((m.geometry.area_fraction - 0.5).abs() < 0.001);
    assert!((m.geometry.bbox.width - 0.5).abs() < 0.001);
    assert!((m.geometry.centroid.x - 0.25).abs() < 0.001);
    assert!(m.subject_background_ev_difference < -2.);
    assert!(m.subject.mean_luminance < m.background.mean_luminance);
    let json = serde_json::to_string(&s).unwrap();
    assert!(!json.contains("values"));
    assert!(!json.contains("pixels"));
    assert!(json.len() < 2500);
    let empty = SoftMask {
        values: vec![0.; 10000],
        ..mask
    };
    let s = measure::subject(&i, &empty, "mask-id".into(), &CancellationToken::default()).unwrap();
    assert_eq!(s.subject_present.value(), Some(&false));
    assert!(s.measurements.value().is_none());
}
#[test]
fn tiny_nonfinite_and_cancelled_measurements_fail_safely() {
    for i in [
        image(2, 2, |_, _| [0.3; 3]),
        image(64, 64, |_, _| [f32::NAN; 3]),
    ] {
        assert!(measure::measure(&i, source(&i), vec![], &CancellationToken::default()).is_err());
    }
    let token = CancellationToken::default();
    token.cancel();
    let i = image(64, 64, |_, _| [0.3; 3]);
    assert_eq!(
        measure::measure(&i, source(&i), vec![], &token)
            .unwrap_err()
            .code,
        ProcessingErrorCode::Cancelled
    );
}

#[path = "support/synthetic.rs"]
mod synthetic;
#[test]
fn real_raw_normalized_input_and_small_batch_timings() {
    use photo_core::rendering::decode::LibRawDecoder;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    synthetic::synthetic_dng(&input.join("synthetic.dng"));
    for name in ["normal.png", "batch-b.png", "batch-c.png"] {
        image::RgbImage::from_fn(1800, 1200, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 120u8])
        })
        .save(input.join(name))
        .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("thumbs")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Synthetic timing".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&job.id, permit).unwrap();
    let helper = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.resources/raw")
        .join(if cfg!(windows) {
            "photo-raw-helper.exe"
        } else {
            "photo-raw-helper"
        });
    assert!(helper.is_file());
    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(LibRawDecoder {
            helper,
            scratch: root.path().join("scratch"),
        }),
        RenderLimits::default(),
    ));
    let service = AnalysisService::new(jobs.repository.clone(), engine, None);
    let start = std::time::Instant::now();
    for asset in jobs.repository.assets(&job.id, 0, 10).unwrap().items {
        let before = std::fs::read(&asset.original_path).unwrap();
        let t = std::time::Instant::now();
        let result = run(&service, &job.id, &asset.id, PhotoType::Landscape)
            .analysis
            .unwrap();
        println!(
            "ANALYSIS_TIMING {}: {} ms; input {}x{}",
            asset.filename,
            t.elapsed().as_millis(),
            result.common.source.width,
            result.common.source.height
        );
        result.validate().unwrap();
        assert!(result.common.source.width <= 1600);
        assert!(
            (result.common.composition.aspect_ratio - 1.5).abs() < 0.01 || result.common.source.raw
        );
        if result.common.source.raw {
            assert!(result.common.source.width < 128);
            assert!(result.common.source.width >= 60);
        }
        assert_eq!(std::fs::read(&asset.original_path).unwrap(), before);
    }
    println!(
        "ANALYSIS_TIMING four-image batch: {} ms",
        start.elapsed().as_millis()
    );
}
struct NoRaw;
impl RawDecoder for NoRaw {
    fn id(&self) -> &str {
        "analysis-test"
    }
    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        panic!("not RAW")
    }
}
struct HalfMask(Arc<AtomicUsize>, bool);
impl SegmentationProvider for HalfMask {
    fn version(&self) -> &str {
        "half-test-v1"
    }
    fn infer(&self, _: &FloatImage, _: &CancellationToken) -> ProcessingResult<SoftMask> {
        self.0.fetch_add(1, Ordering::SeqCst);
        if self.1 {
            return Err(photo_core::rendering::internal(
                "Synthetic provider failure",
            ));
        }
        Ok(SoftMask {
            width: 4,
            height: 2,
            values: vec![1., 1., 0., 0., 1., 1., 0., 0.],
        })
    }
}
fn setup(
    fail: bool,
) -> (
    tempfile::TempDir,
    JobService,
    AnalysisService,
    String,
    Vec<String>,
    Arc<AtomicUsize>,
) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for name in ["a.png", "b.png", "c.png"] {
        image::RgbImage::from_fn(128, 64, |x, _| {
            if x < 64 {
                image::Rgb([70u8; 3])
            } else {
                image::Rgb([180u8; 3])
            }
        })
        .save(input.join(name))
        .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("thumbs")).unwrap();
    let (job, p) = jobs
        .create(NewJob {
            name: "Analysis".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&job.id, p).unwrap();
    let ids = jobs
        .repository
        .assets(&job.id, 0, 100)
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.id)
        .collect();
    let calls = Arc::new(AtomicUsize::new(0));
    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ));
    let service = AnalysisService::new(
        jobs.repository.clone(),
        engine,
        Some(MaskCache::new(
            root.path().join("analysis-masks"),
            Box::new(HalfMask(calls.clone(), fail)),
        )),
    );
    (root, jobs, service, job.id, ids, calls)
}
fn request(job: &str, asset: &str, kind: PhotoType) -> AnalysisRequest {
    AnalysisRequest {
        job_id: job.into(),
        asset_id: asset.into(),
        photo_type: kind,
        request_id: uuid::Uuid::new_v4().to_string(),
    }
}
fn run(
    service: &AnalysisService,
    job: &str,
    asset: &str,
    kind: PhotoType,
) -> photo_core::analysis::AnalysisState {
    service
        .analyze_asset(service.reserve(request(job, asset, kind)).unwrap())
        .unwrap()
}
#[test]
fn persist_reload_cache_photo_types_and_recipe_independence() {
    let (_root, jobs, service, job, ids, calls) = setup(false);
    let asset = &ids[0];
    let repo = &jobs.repository;
    let initial = repo.get_recipe(&job, asset).unwrap();
    let source_path = repo.asset(&job, asset).unwrap().original_path;
    let source_before = std::fs::read(&source_path).unwrap();
    let first = run(&service, &job, asset, PhotoType::Portrait);
    let a = first.analysis.unwrap();
    assert!(!first.cached);
    a.validate().unwrap();
    assert_eq!(a.schema_version, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(repo.get_recipe(&job, asset).unwrap().recipe, initial.recipe);
    assert_eq!(std::fs::read(&source_path).unwrap(), source_before);
    for stage in 0..3 {
        let current = repo.get_recipe(&job, asset).unwrap();
        let mut recipe = current.recipe.clone();
        match stage {
            0 => recipe.global.basic.exposure_ev = 1.2,
            1 => recipe.global.color_mixer.red.hue = 15.,
            _ => recipe.local_layers.push(RecipeLayer {
                id: "subject".into(),
                mask_type: MaskType::Subject,
                enabled: true,
                strength: 1.,
                invert: false,
                confidence: None,
                mask_reference: None,
                adjustments: LocalAdjustments {
                    exposure_ev: 0.7,
                    ..Default::default()
                },
            }),
        };
        repo.save_recipe(&job, asset, &recipe, current.generation, None)
            .unwrap();
        let cached = run(&service, &job, asset, PhotoType::Portrait);
        assert!(cached.cached);
        assert_eq!(cached.analysis.unwrap(), a);
    }
    assert_eq!(
        service
            .get_analysis(&job, asset, PhotoType::Portrait)
            .unwrap()
            .analysis
            .unwrap(),
        a
    );
    for kind in [PhotoType::Landscape, PhotoType::RealEstate] {
        let b = run(&service, &job, asset, kind).analysis.unwrap();
        assert_eq!(b.common, a.common);
        assert!(b.diagnostics.common_cache_reused);
        assert!(matches!(
            b.subjects.measurements,
            Observation::NotApplicable { .. }
        ));
        b.validate().unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let exported = service
        .export_analysis(&job, asset, PhotoType::Portrait)
        .unwrap();
    let reloaded = PhotoAnalysis::parse(&std::fs::read_to_string(&exported).unwrap()).unwrap();
    assert_eq!(reloaded, a);
    assert_ne!(
        exported,
        service
            .export_analysis(&job, asset, PhotoType::Portrait)
            .unwrap()
    );
}
#[test]
fn source_version_and_explicit_invalidation() {
    let (_root, jobs, service, job, ids, _) = setup(false);
    let id = &ids[0];
    let old = run(&service, &job, id, PhotoType::Landscape)
        .analysis
        .unwrap();
    let path = jobs.repository.asset(&job, id).unwrap().original_path;
    image::RgbImage::from_pixel(129, 64, image::Rgb([240u8; 3]))
        .save(path)
        .unwrap();
    assert!(service
        .get_analysis(&job, id, PhotoType::Landscape)
        .unwrap()
        .analysis
        .is_none());
    let new = run(&service, &job, id, PhotoType::Landscape)
        .analysis
        .unwrap();
    assert_ne!(old.source_fingerprint, new.source_fingerprint);
    let k = cache_identity("s", "e", "d", PhotoType::Portrait, "m");
    assert_ne!(k, cache_identity("s", "e2", "d", PhotoType::Portrait, "m"));
    assert_ne!(k, cache_identity("s", "e", "d", PhotoType::Portrait, "m2"));
    assert_ne!(k, cache_identity("s", "e", "d", PhotoType::Landscape, "m"));
    service.invalidate_analysis(&job, id).unwrap();
    assert_eq!(
        service
            .get_analysis(&job, id, PhotoType::Landscape)
            .unwrap()
            .status,
        AnalysisStatus::NotAnalyzed
    );
}
#[test]
fn failure_is_partial_and_cancelled_queue_is_bounded() {
    let (_root, _jobs, service, job, ids, _) = setup(true);
    let a = run(&service, &job, &ids[0], PhotoType::Portrait);
    assert_eq!(a.status, AnalysisStatus::Warning);
    let a = a.analysis.unwrap();
    assert!(a.subjects.measurements.value().is_none());
    assert!(a.common.exposure.mean_luminance > 0.);
    let r = request(&job, &ids[1], PhotoType::Landscape);
    let id = r.request_id.clone();
    let p1 = service.reserve(r).unwrap();
    let p2 = service
        .reserve(request(&job, &ids[2], PhotoType::Landscape))
        .unwrap();
    assert!(service
        .reserve(request(&job, &ids[0], PhotoType::Landscape))
        .is_err());
    assert!(service.invalidate_analysis(&job, &ids[1]).is_err());
    service.cancel(&id).unwrap();
    assert_eq!(
        service.analyze_asset(p1).unwrap_err().code,
        ProcessingErrorCode::Cancelled
    );
    assert!(service
        .get_analysis(&job, &ids[1], PhotoType::Landscape)
        .unwrap()
        .analysis
        .is_none());
    drop(p2);
    assert!(run(&service, &job, &ids[1], PhotoType::Landscape)
        .analysis
        .is_some());
}
#[test]
fn schema_safe_loading_rejects_future_invalid_and_deterministic_payloads() {
    let (_root, _jobs, service, job, ids, _) = setup(false);
    let a = run(&service, &job, &ids[0], PhotoType::Portrait)
        .analysis
        .unwrap();
    let json = a.canonical_json().unwrap();
    let frontend_fixture =
        PhotoAnalysis::parse(include_str!("../../../src/test/analysis-fixture.json")).unwrap();
    assert_eq!(frontend_fixture.common, a.common);
    assert_eq!(PhotoAnalysis::parse(&json).unwrap(), a);
    let mut v = serde_json::to_value(&a).unwrap();
    v["schema_version"] = 99.into();
    assert_eq!(
        PhotoAnalysis::parse(&v.to_string()).unwrap_err(),
        AnalysisError::UnsupportedVersion(99)
    );
    let mut bad = a.clone();
    bad.common.exposure.shadow_clip_fraction = 1.1;
    assert!(bad.validate().is_err());
    bad = a.clone();
    bad.common.exposure.mean_luminance = f64::NAN;
    assert!(bad.validate().is_err());
    service.invalidate_analysis(&job, &ids[0]).unwrap();
    let b = run(&service, &job, &ids[0], PhotoType::Portrait)
        .analysis
        .unwrap();
    assert_eq!(a.common, b.common);
    assert_eq!(a.subjects, b.subjects);
}

struct WaitingMask {
    entered: std::sync::mpsc::Sender<()>,
    release: Arc<std::sync::atomic::AtomicBool>,
}
impl SegmentationProvider for WaitingMask {
    fn version(&self) -> &str {
        "waiting-v1"
    }
    fn infer(&self, _: &FloatImage, cancel: &CancellationToken) -> ProcessingResult<SoftMask> {
        self.entered.send(()).unwrap();
        let start = std::time::Instant::now();
        while !self.release.load(Ordering::SeqCst) {
            cancel.check()?;
            assert!(start.elapsed().as_secs() < 5, "test worker not released");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        cancel.check()?;
        Ok(SoftMask {
            width: 2,
            height: 2,
            values: vec![1., 0., 1., 0.],
        })
    }
}
#[test]
fn cancellation_during_provider_and_source_change_never_publish_partial_analysis() {
    for change_source in [false, true] {
        let (root, jobs, _service, job, ids, _) = setup(false);
        let (tx, rx) = std::sync::mpsc::channel();
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let service = Arc::new(AnalysisService::new(
            jobs.repository.clone(),
            Arc::new(CpuProcessingEngine::new(
                Box::new(NoRaw),
                RenderLimits::default(),
            )),
            Some(MaskCache::new(
                root.path().join("waiting-mask"),
                Box::new(WaitingMask {
                    entered: tx,
                    release: release.clone(),
                }),
            )),
        ));
        let r = request(&job, &ids[0], PhotoType::Portrait);
        let rid = r.request_id.clone();
        let permit = service.reserve(r).unwrap();
        let worker = service.clone();
        let thread = std::thread::spawn(move || worker.analyze_asset(permit));
        rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        if change_source {
            let path = jobs.repository.asset(&job, &ids[0]).unwrap().original_path;
            image::RgbImage::from_pixel(130, 64, image::Rgb([200u8; 3]))
                .save(path)
                .unwrap();
            release.store(true, Ordering::SeqCst);
        } else {
            service.cancel(&rid).unwrap();
        }
        let error = thread.join().unwrap().unwrap_err();
        assert_eq!(
            error.code,
            if change_source {
                ProcessingErrorCode::SourceChanged
            } else {
                ProcessingErrorCode::Cancelled
            }
        );
        assert!(service
            .get_analysis(&job, &ids[0], PhotoType::Portrait)
            .unwrap()
            .analysis
            .is_none());
        if !change_source {
            assert!(!root.path().join("waiting-mask").exists());
        }
    }
}
#[test]
fn persisted_corruption_fails_safely_and_invalidation_recovers() {
    let (root, jobs, service, job, ids, _) = setup(false);
    let before = jobs.repository.get_recipe(&job, &ids[0]).unwrap();
    run(&service, &job, &ids[0], PhotoType::Landscape);
    let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
    db.execute("UPDATE photo_analysis SET payload='{}'", [])
        .unwrap();
    assert!(service
        .get_analysis(&job, &ids[0], PhotoType::Landscape)
        .is_err());
    assert_eq!(
        jobs.repository.get_recipe(&job, &ids[0]).unwrap().recipe,
        before.recipe
    );
    service.invalidate_analysis(&job, &ids[0]).unwrap();
    assert!(run(&service, &job, &ids[0], PhotoType::Landscape)
        .analysis
        .is_some());
}

struct VersionedMask(&'static str, Arc<AtomicUsize>);
impl SegmentationProvider for VersionedMask {
    fn version(&self) -> &str {
        self.0
    }
    fn infer(&self, image: &FloatImage, cancel: &CancellationToken) -> ProcessingResult<SoftMask> {
        HalfMask(self.1.clone(), false).infer(image, cancel)
    }
}
#[test]
fn model_version_change_recomputes_subject_but_reuses_common_source_measurements() {
    let (root, jobs, service, job, ids, calls) = setup(false);
    let first = run(&service, &job, &ids[0], PhotoType::Portrait)
        .analysis
        .unwrap();
    let changed = AnalysisService::new(
        jobs.repository.clone(),
        Arc::new(CpuProcessingEngine::new(
            Box::new(NoRaw),
            RenderLimits::default(),
        )),
        Some(MaskCache::new(
            root.path().join("analysis-masks"),
            Box::new(VersionedMask("half-test-v2", calls.clone())),
        )),
    );
    assert!(changed
        .get_analysis(&job, &ids[0], PhotoType::Portrait)
        .unwrap()
        .analysis
        .is_none());
    let next = run(&changed, &job, &ids[0], PhotoType::Portrait)
        .analysis
        .unwrap();
    assert_ne!(first.analysis_id, next.analysis_id);
    assert_eq!(first.common, next.common);
    assert!(next.diagnostics.common_cache_reused);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(next.diagnostics.providers[0].version, "half-test-v2");
}
#[test]
fn analysis_masks_cannot_activate_unresolved_renderer_layers() {
    use photo_core::rendering::optics::LensProfileResolver;
    let (root, jobs, _service, job, ids, calls) = setup(false);
    let engine = Arc::new(
        CpuProcessingEngine::new(Box::new(NoRaw), RenderLimits::default()).with_toolkit(
            LensProfileResolver::unavailable("test"),
            MaskCache::new(
                root.path().join("renderer-masks"),
                Box::new(HalfMask(calls.clone(), false)),
            ),
        ),
    );
    let service = AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        Some(MaskCache::new(
            root.path().join("isolated-analysis-masks"),
            Box::new(HalfMask(calls.clone(), false)),
        )),
    );
    let state = jobs.repository.get_recipe(&job, &ids[0]).unwrap();
    let mut recipe = state.recipe;
    recipe.local_layers.push(RecipeLayer {
        id: "subject".into(),
        mask_type: MaskType::Subject,
        enabled: true,
        strength: 1.,
        invert: false,
        confidence: None,
        mask_reference: None,
        adjustments: LocalAdjustments {
            exposure_ev: 1.,
            ..Default::default()
        },
    });
    let recipe = jobs
        .repository
        .save_recipe(&job, &ids[0], &recipe, state.generation, None)
        .unwrap()
        .recipe;
    let path = jobs.repository.asset(&job, &ids[0]).unwrap().original_path;
    let before = engine
        .effective_recipe(&recipe, &path, &Default::default())
        .unwrap();
    assert_eq!(before.unresolved_masks, vec!["subject"]);
    let analysis = run(&service, &job, &ids[0], PhotoType::Portrait)
        .analysis
        .unwrap();
    assert!(analysis.subjects.measurements.value().is_some());
    let after = engine
        .effective_recipe(&recipe, &path, &Default::default())
        .unwrap();
    assert_eq!(before.dependency_hash, after.dependency_hash);
    assert_eq!(before.unresolved_masks, after.unresolved_masks);
    assert!(!root.path().join("renderer-masks").exists());
    assert_eq!(
        jobs.repository.get_recipe(&job, &ids[0]).unwrap().recipe,
        recipe
    );
}
