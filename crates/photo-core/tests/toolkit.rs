use photo_contracts::*;
use photo_core::{
    development::{DevelopmentService, MaskRequest},
    jobs::JobService,
    models::NewJob,
    rendering::{
        self,
        decode::{Decoded, RawDecoder},
        masks::{self, MaskCache, ModnetProvider, SegmentationProvider, SoftMask},
        optics::{LensProfileResolver, OpticalMap},
        pixels::{self, FloatImage},
        tools, CpuProcessingEngine, RenderLimits,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
fn token() -> CancellationToken {
    CancellationToken::default()
}
fn image(w: u32, h: u32) -> FloatImage {
    FloatImage {
        width: w,
        height: h,
        pixels: (0..w * h)
            .map(|i| {
                let x = (i % w) as f32 / w as f32;
                let v = 0.1 + 0.5 * x + if i % 7 == 0 { 0.06 } else { 0. };
                [v, v * 0.8, v * 0.6]
            })
            .collect(),
    }
}
fn layer(kind: MaskType) -> LocalAdjustmentLayer {
    LocalAdjustmentLayer {
        id: format!("{kind:?}"),
        mask_type: kind,
        enabled: true,
        strength: 1.,
        invert: false,
        confidence: None,
        mask_reference: None,
        adjustments: Default::default(),
    }
}
fn half_mask() -> SoftMask {
    SoftMask {
        width: 4,
        height: 1,
        values: vec![1., 1., 0., 0.],
    }
}
fn approx(x: f32, y: f32) {
    assert!((x - y).abs() < 1e-5, "{x} != {y}");
}
#[test]
fn neutral_curve_hsl_presence_detail_and_vignette_preserve_float_pixels() {
    let source = image(64, 32);
    let mut output = source.clone();
    let a = RenderAdjustments::default();
    tools::color(&mut output, &a, &token()).unwrap();
    tools::presence(&mut output, a.presence, &token()).unwrap();
    tools::detail(&mut output, a.detail, &token()).unwrap();
    tools::vignette(&mut output, a.effects.vignette, &token()).unwrap();
    assert_eq!(source.pixels, output.pixels);
    for x in [-0.1, 0., 0.1, 0.8, 1., 2.] {
        assert_eq!(tools::curve_value(x, &a.curve.master), x);
    }
}
#[test]
fn rgb_curve_changes_only_target_channel() {
    let mut output = image(3, 1);
    let source = output.clone();
    let mut a = RenderAdjustments::default();
    a.curve.red.insert(1, CurvePoint { x: 0.5, y: 0.7 });
    tools::color(&mut output, &a, &token()).unwrap();
    assert!(output.pixels[1][0] > source.pixels[1][0]);
    approx(output.pixels[1][1], source.pixels[1][1]);
    approx(output.pixels[1][2], source.pixels[1][2]);
}
#[test]
fn hsl_hue_saturation_and_luminance_have_separate_semantics() {
    let source = FloatImage {
        width: 1,
        height: 1,
        pixels: vec![[0.5, 0.05, 0.02]],
    };
    let run = |band: HslBand| {
        let mut a = RenderAdjustments::default();
        a.hsl[0] = band;
        let mut p = source.clone();
        tools::color(&mut p, &a, &token()).unwrap();
        p.pixels[0]
    };
    let hue = run(HslBand {
        hue: 100.,
        ..Default::default()
    });
    assert!(hue[1] > source.pixels[0][1]);
    let sat = run(HslBand {
        saturation: -100.,
        ..Default::default()
    });
    assert!(sat[0] - sat[2] < 0.48);
    let lum = run(HslBand {
        luminance: 100.,
        ..Default::default()
    });
    assert!(lum[0] > 0.5);
    approx(lum[0] / lum[2], 25.);
}
#[test]
fn hue_band_boundaries_and_red_wrap_are_smooth() {
    for h in [0., 30., 60., 120., 180., 240., 275., 315., 360.] {
        let a = tools::hue_weights(h - 0.001);
        let b = tools::hue_weights(h + 0.001);
        approx(a.iter().sum(), 1.);
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.001));
    }
}
#[test]
fn presence_tools_are_distinct_spatial_operators() {
    let source = image(128, 64);
    let run = |p| {
        let mut im = source.clone();
        tools::presence(&mut im, p, &token()).unwrap();
        im.pixels
    };
    let texture = run(Presence {
        texture: 50.,
        ..Default::default()
    });
    let clarity = run(Presence {
        clarity: 50.,
        ..Default::default()
    });
    let dehaze = run(Presence {
        dehaze: 50.,
        ..Default::default()
    });
    assert_ne!(texture, source.pixels);
    assert_ne!(texture, clarity);
    assert_ne!(clarity, dehaze);
}
#[test]
fn expanded_detail_is_finite_distinct_and_conservative() {
    let mut im = image(32, 16);
    let source = im.clone();
    let mut d = Detail::default();
    d.sharpening.amount = 50.;
    d.sharpening.radius = 2.;
    d.noise.luminance = 25.;
    d.noise.color = 30.;
    tools::detail(&mut im, d, &token()).unwrap();
    assert_ne!(im.pixels, source.pixels);
    assert!(im.pixels.iter().flatten().all(|v| v.is_finite()));
}
#[test]
fn creative_vignette_is_centered_on_final_cropped_canvas() {
    let mut im = FloatImage {
        width: 9,
        height: 9,
        pixels: vec![[0.3; 3]; 81],
    };
    tools::vignette(
        &mut im,
        Vignette {
            amount: -60.,
            ..Default::default()
        },
        &token(),
    )
    .unwrap();
    approx(im.pixels[40][0], 0.3);
    assert!(im.pixels[0][0] < 0.3);
    assert_eq!(im.pixels[0], im.pixels[80]);
}
#[test]
fn subject_and_background_are_exact_complements_with_soft_edges() {
    let mask = SoftMask {
        width: 3,
        height: 1,
        values: vec![0., 0.4, 1.],
    };
    let map = OpticalMap::default();
    for x in 0..3 {
        let s = masks::layer_weight(&mask, &map, &layer(MaskType::Subject), x, 0, 3, 1);
        let b = masks::layer_weight(&mask, &map, &layer(MaskType::Background), x, 0, 3, 1);
        approx(s + b, 1.);
    }
    approx(
        masks::layer_weight(&mask, &map, &layer(MaskType::Subject), 1, 0, 3, 1),
        0.4,
    );
}
#[test]
fn local_exposure_strength_disabled_and_inversion_affect_only_masked_pixels() {
    let source = FloatImage {
        width: 4,
        height: 1,
        pixels: vec![[0.2; 3]; 4],
    };
    let mut l = layer(MaskType::Subject);
    l.adjustments.exposure_ev = 1.;
    let run = |l: LocalAdjustmentLayer| {
        let mut im = source.clone();
        masks::apply_layers(
            &mut im,
            &[l],
            &half_mask(),
            "key",
            &OpticalMap::default(),
            &token(),
        )
        .unwrap();
        im
    };
    let out = run(l.clone());
    approx(out.pixels[0][0], 0.4);
    approx(out.pixels[3][0], 0.2);
    l.strength = 0.5;
    approx(run(l.clone()).pixels[0][0], 0.3);
    l.strength = 0.;
    assert_eq!(run(l.clone()).pixels, source.pixels);
    l.strength = 1.;
    l.enabled = false;
    assert_eq!(run(l.clone()).pixels, source.pixels);
    l.enabled = true;
    l.invert = true;
    approx(run(l).pixels[3][0], 0.4);
}
#[test]
fn local_white_balance_preserves_unmasked_region() {
    let mut im = FloatImage {
        width: 4,
        height: 1,
        pixels: vec![[0.2; 3]; 4],
    };
    let mut l = layer(MaskType::Subject);
    l.adjustments.temperature = 7500.;
    masks::apply_layers(
        &mut im,
        &[l],
        &half_mask(),
        "key",
        &OpticalMap::default(),
        &token(),
    )
    .unwrap();
    assert!(im.pixels[0][0] > im.pixels[0][2]);
    assert_eq!(im.pixels[3], [0.2; 3]);
}
#[test]
fn global_and_ordered_local_layers_compose_deterministically() {
    let run = || {
        let mut im = FloatImage {
            width: 4,
            height: 1,
            pixels: vec![[0.1; 3]; 4],
        };
        pixels::apply(
            &mut im,
            &RenderAdjustments {
                exposure_ev: 1.,
                ..Default::default()
            },
            &token(),
        )
        .unwrap();
        let mut s = layer(MaskType::Subject);
        s.adjustments.exposure_ev = 1.;
        let mut b = layer(MaskType::Background);
        b.adjustments.exposure_ev = -1.;
        masks::apply_layers(
            &mut im,
            &[s, b],
            &half_mask(),
            "key",
            &OpticalMap::default(),
            &token(),
        )
        .unwrap();
        im
    };
    let a = run();
    assert_eq!(a.pixels, run().pixels);
    approx(a.pixels[0][0], 0.4);
    approx(a.pixels[3][0], 0.1);
}
struct Provider {
    count: Arc<AtomicUsize>,
    fail: bool,
}
impl SegmentationProvider for Provider {
    fn version(&self) -> &str {
        "synthetic-v1"
    }
    fn infer(&self, _: &FloatImage, _: &CancellationToken) -> ProcessingResult<SoftMask> {
        self.count.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(rendering::internal("synthetic inference failure"))
        } else {
            Ok(half_mask())
        }
    }
}
struct Raw;
impl RawDecoder for Raw {
    fn id(&self) -> &str {
        "synthetic-raw"
    }
    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        Ok(Decoded {
            image: image(60, 40),
            warnings: vec![],
        })
    }
}
fn resources() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.resources/toolkit")
}
fn fixture_engine(root: &Path, fail: bool, count: Arc<AtomicUsize>) -> CpuProcessingEngine {
    CpuProcessingEngine::new(Box::new(Raw), RenderLimits::default()).with_toolkit(
        LensProfileResolver::unavailable("fixture"),
        MaskCache::new(root.join("masks"), Box::new(Provider { count, fail })),
    )
}
#[test]
fn mask_cache_identity_excludes_creative_parameters_and_tracks_source_model_decoder() {
    let key = masks::cache_key("source", "decode-v1", "model-v1");
    assert_eq!(key.len(), 64);
    assert_ne!(key, masks::cache_key("changed", "decode-v1", "model-v1"));
    assert_ne!(key, masks::cache_key("source", "decode-v2", "model-v1"));
    assert_ne!(key, masks::cache_key("source", "decode-v1", "model-v2"));
}
#[test]
fn mask_cache_persists_soft_alpha_and_does_not_repeat_inference() {
    let root = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let cache = MaskCache::new(
        root.path().into(),
        Box::new(Provider {
            count: count.clone(),
            fail: false,
        }),
    );
    let source = image(8, 8);
    let first = cache
        .generate("source", "decoder", &source, &token())
        .unwrap();
    assert_eq!(first.status, MaskStatus::Ready);
    assert!(first.confidence.is_none());
    cache
        .generate("source", "decoder", &source, &token())
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let (mask, diag) = cache.load("source", "decoder");
    assert_eq!(mask.unwrap().values, half_mask().values);
    assert_eq!(diag.reference, first.reference);
    std::fs::remove_file(first.cache_path.unwrap()).unwrap();
    assert_eq!(cache.load("source", "decoder").1.status, MaskStatus::Stale);
    cache
        .generate("source", "decoder", &source, &token())
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
fn render_request(root: &Path) -> RenderRequest {
    let source = root.join("fixture.cr3");
    std::fs::write(&source, b"synthetic source for mock decoder").unwrap();
    RenderRequest {
        asset_id: "fixture".into(),
        original: source,
        adjustments: Default::default(),
        source_metadata: Default::default(),
        destination: root.join("export.tif"),
        output_format: OutputFormat::Tiff,
        preview: false,
        jpeg_quality: 95,
    }
}
#[test]
fn failed_segmentation_and_missing_profile_do_not_block_global_export() {
    let root = tempfile::tempdir().unwrap();
    let engine = fixture_engine(root.path(), true, Arc::new(AtomicUsize::new(0)));
    let mut request = render_request(root.path());
    request.adjustments.exposure_ev = 0.4;
    request.adjustments.optics.enabled = true;
    request.adjustments.local_layers = vec![layer(MaskType::Subject)];
    let (d, _) = engine
        .mask_preview(
            &request.original,
            &Default::default(),
            &request.adjustments,
            None,
            true,
            &token(),
        )
        .unwrap();
    assert_eq!(d.status, MaskStatus::Failed);
    let result = engine.render(&request, &token()).unwrap();
    assert!(result.rendered_image.exists());
    assert_eq!(result.diagnostics.lens.state, LensMatch::ProfileUnavailable);
}
#[test]
fn overlays_never_change_export_pixels_and_exposure_does_not_regenerate_masks() {
    let root = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let engine = fixture_engine(root.path(), false, count.clone());
    let mut r = render_request(root.path());
    let mut l = layer(MaskType::Subject);
    l.adjustments.exposure_ev = 0.5;
    r.adjustments.local_layers = vec![l.clone()];
    engine
        .mask_preview(
            &r.original,
            &Default::default(),
            &r.adjustments,
            None,
            true,
            &token(),
        )
        .unwrap();
    engine.render(&r, &token()).unwrap();
    let before = std::fs::read(&r.destination).unwrap();
    let (_, overlay) = engine
        .mask_preview(
            &r.original,
            &Default::default(),
            &r.adjustments,
            Some(&l),
            false,
            &token(),
        )
        .unwrap();
    assert!(overlay.unwrap().starts_with(b"\x89PNG"));
    r.destination = root.path().join("after-overlay.tif");
    engine.render(&r, &token()).unwrap();
    assert_eq!(before, std::fs::read(&r.destination).unwrap());
    r.adjustments.exposure_ev = 1.;
    engine
        .mask_preview(
            &r.original,
            &Default::default(),
            &r.adjustments,
            None,
            true,
            &token(),
        )
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
#[test]
fn subject_metadata_persists_in_sqlite_without_pixel_arrays() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir_all(&input).unwrap();
    std::fs::create_dir_all(&output).unwrap();
    image::RgbImage::from_pixel(60, 40, image::Rgb([90, 120, 100]))
        .save(input.join("test.jpg"))
        .unwrap();
    let jobs = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "fixture".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&job.id, permit).unwrap();
    let asset = jobs.assets(&job.id, 0, 10).unwrap().items.remove(0);
    let engine = Arc::new(fixture_engine(
        root.path(),
        false,
        Arc::new(AtomicUsize::new(0)),
    ));
    let service = DevelopmentService::new(
        jobs.repository.clone(),
        engine,
        root.path().join("development"),
        None,
    )
    .unwrap();
    let a = RenderAdjustments {
        local_layers: vec![layer(MaskType::Subject)],
        ..Default::default()
    };
    service.save(&job.id, &asset.id, &a).unwrap();
    let result = service
        .mask(
            MaskRequest {
                job_id: job.id.clone(),
                asset_id: asset.id.clone(),
                request_id: "mask".into(),
                adjustments: a.clone(),
                layer_id: None,
                generate: true,
            },
            service.reserve("mask", true).unwrap(),
        )
        .unwrap();
    let state = service.load(&job.id, &asset.id).unwrap();
    assert_eq!(state.adjustments, a);
    assert_eq!(
        state.diagnostics.mask.reference,
        result.diagnostic.reference
    );
    assert!(serde_json::to_string(&state).unwrap().len() < 6000);
}
fn lens_metadata() -> OpticsMetadata {
    OpticsMetadata {
        camera_make: Some("Canon".into()),
        camera_model: Some("Canon EOS 650D".into()),
        lens_make: Some("Canon".into()),
        lens_model: Some("EF-S 10-22mm f/3.5-4.5 USM".into()),
        focal_length: Some(10.),
        aperture: Some(3.5),
        focus_distance: Some(10.),
    }
}
#[test]
fn actual_pinned_lens_database_resolves_exact_calibration() {
    let db = LensProfileResolver::load(&resources().join("lensfun-db"));
    let (map, d) = db.resolve(
        &lens_metadata(),
        Optics {
            enabled: true,
            ..Default::default()
        },
        120,
        80,
    );
    assert_eq!(d.state, LensMatch::ExactMatch, "{d:?}");
    assert_eq!(d.applied.len(), 3);
    assert!(map.active());
    let a = map.apply(image(120, 80), &token()).unwrap();
    let b = map.apply(image(120, 80), &token()).unwrap();
    assert_eq!(a.pixels, b.pixels);
}
#[test]
fn zero_profile_strength_does_not_resample_or_illuminate_pixels() {
    let db = LensProfileResolver::load(&resources().join("lensfun-db"));
    let (map, d) = db.resolve(
        &lens_metadata(),
        Optics {
            enabled: true,
            distortion_strength: 0.,
            vignette_strength: 0.,
            chromatic_aberration: false,
            ..Default::default()
        },
        120,
        80,
    );
    assert!(!map.active());
    assert!(d.applied.is_empty());
    let source = image(120, 80);
    assert_eq!(
        map.apply(source.clone(), &token()).unwrap().pixels,
        source.pixels
    );
}
#[test]
fn optics_disabled_unknown_ambiguous_and_unavailable_are_nonfatal() {
    let db = LensProfileResolver::load(&resources().join("lensfun-db"));
    let (_, d) = db.resolve(&lens_metadata(), Default::default(), 120, 80);
    assert_eq!(d.state, LensMatch::CorrectionDisabled);
    let mut m = lens_metadata();
    m.lens_model = Some("Not a known lens".into());
    assert_eq!(
        db.resolve(
            &m,
            Optics {
                enabled: true,
                ..Default::default()
            },
            120,
            80
        )
        .1
        .state,
        LensMatch::NoProfile
    );
    m = lens_metadata();
    m.camera_model = None;
    let (map, d) = db.resolve(
        &m,
        Optics {
            enabled: true,
            ..Default::default()
        },
        120,
        80,
    );
    assert_eq!(d.state, LensMatch::ApproximateMatch);
    assert!(!map.active());
    let missing = LensProfileResolver::load(&resources().join("missing"));
    assert_eq!(
        missing
            .resolve(
                &m,
                Optics {
                    enabled: true,
                    ..Default::default()
                },
                120,
                80
            )
            .1
            .state,
        LensMatch::ProfileUnavailable
    );
}
#[test]
fn distortion_bounds_and_mask_coordinates_share_the_same_map_after_rotation_crop() {
    let map = OpticalMap::manual(Optics {
        manual_distortion: 35.,
        ..Default::default()
    });
    let mask = SoftMask {
        width: 120,
        height: 80,
        values: (0..9600)
            .map(|i| if i % 120 < 60 { 1. } else { 0. })
            .collect(),
    };
    let source = FloatImage {
        width: 120,
        height: 80,
        pixels: mask.values.iter().map(|v| [*v; 3]).collect(),
    };
    let corrected = map.apply(source, &token()).unwrap();
    let mut alpha = FloatImage::blank(120, 80, 9600).unwrap();
    for y in 0..80 {
        for x in 0..120 {
            let weight = masks::layer_weight(&mask, &map, &layer(MaskType::Subject), x, y, 120, 80);
            alpha.pixels[(y * 120 + x) as usize] = [weight; 3];
        }
    }
    for (a, b) in corrected.pixels.iter().zip(&alpha.pixels) {
        approx(a[0], b[0]);
    }
    let a = RenderAdjustments {
        rotation_degrees: 90.,
        crop: Crop {
            x: 0.1,
            y: 0.1,
            width: 0.8,
            height: 0.8,
        },
        ..Default::default()
    };
    let rendered = pixels::geometry(corrected, &a, 20000, &token()).unwrap();
    let overlay = pixels::geometry(alpha, &a, 20000, &token()).unwrap();
    assert_eq!(rendered.pixels, overlay.pixels);
    let (x, y) = map.source_coordinate(0., 0., 120, 80, 1);
    assert!(x.is_finite() && y.is_finite());
    assert!(x < 0. || y < 0.);
}
#[test]
fn real_modnet_cpu_model_loads_and_infers_bounded_soft_alpha() {
    let root = tempfile::tempdir().unwrap();
    let provider = ModnetProvider {
        resources: resources(),
        scratch: root.path().into(),
    };
    let source = image(96, 64);
    let start = std::time::Instant::now();
    let first = provider.infer(&source, &token()).unwrap();
    let second = provider.infer(&source, &token()).unwrap();
    assert_eq!((first.width, first.height), (768, 512));
    assert!(first
        .values
        .iter()
        .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
    assert_eq!(first.values, second.values);
    println!(
        "Two real CPU MODNet synthetic-fixture inferences: {:?}",
        start.elapsed()
    );
}
#[test]
fn phase_two_database_migrates_without_changing_existing_recipes_or_checkpoints() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("old.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_ingestion_warnings.sql"),
        include_str!("../migrations/003_development.sql"),
    ] {
        db.execute_batch(sql).unwrap();
    }
    db.pragma_update(None, "user_version", 3).unwrap();
    db.execute_batch("INSERT INTO jobs(id,name,input_path,output_path,created_at,updated_at,status) VALUES ('legacy','Legacy','input','output','created','updated','ready'); INSERT INTO assets(id,job_id,original_path,filename,file_type,file_size,fingerprint,metadata_json,preview_status,created_at) VALUES ('asset','legacy','original.cr3','original.cr3','cr3',100,'fingerprint','{}','unavailable','created'); INSERT INTO processing_state(job_id,asset_id,stage,updated_at) VALUES ('legacy','asset','exported','updated'); INSERT INTO development_state(job_id,asset_id,adjustments_json,revision,state,export_path,updated_at) VALUES('legacy','asset','{\"exposure_ev\":1.2,\"sharpening\":14}',7,'exported','old-export.jpg','updated');").unwrap();
    let repo = photo_core::repository::JobRepository::open(path.clone()).unwrap();
    let state = repo.development("legacy", "asset").unwrap();
    assert_eq!(state.adjustments.exposure_ev, 1.2);
    assert_eq!(state.adjustments.sharpening, 14.);
    assert_eq!(state.revision, 7);
    assert!(state.adjustments.local_layers.is_empty());
    assert_eq!(state.export_path, Some(PathBuf::from("old-export.jpg")));
    assert_eq!(state.state, "exported");
    assert_eq!(state.diagnostics.mask.status, MaskStatus::Unavailable);
    photo_core::repository::JobRepository::open(path).unwrap();
}
#[test]
fn corrupt_database_and_unsupported_calibration_do_not_apply_wrong_optics() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("bad.xml"), "<lensdatabase>").unwrap();
    let db = LensProfileResolver::load(root.path());
    let (_, d) = db.resolve(
        &lens_metadata(),
        Optics {
            enabled: true,
            ..Default::default()
        },
        120,
        80,
    );
    assert_eq!(d.state, LensMatch::ProfileUnavailable);
    let db = LensProfileResolver::load(&resources().join("lensfun-db"));
    let mut m = lens_metadata();
    m.focal_length = Some(13.37);
    let (map, d) = db.resolve(
        &m,
        Optics {
            enabled: true,
            ..Default::default()
        },
        120,
        80,
    );
    assert_eq!(d.state, LensMatch::ApproximateMatch);
    assert!(d.applied.is_empty());
    assert!(!map.active());
}
#[test]
fn stale_mask_references_and_custom_masks_are_not_applied() {
    let source = image(4, 1);
    for kind in [MaskType::Subject, MaskType::Custom] {
        let mut l = layer(kind);
        l.adjustments.exposure_ev = 2.;
        l.mask_reference = Some("old-mask".into());
        let mut output = source.clone();
        masks::apply_layers(
            &mut output,
            &[l],
            &half_mask(),
            "current-mask",
            &OpticalMap::default(),
            &token(),
        )
        .unwrap();
        assert_eq!(source.pixels, output.pixels);
    }
    assert!(SoftMask {
        width: 1,
        height: 1,
        values: vec![f32::NAN]
    }
    .validated()
    .is_err());
}
#[test]
fn preview_and_export_share_all_toolkit_stages_for_same_resolution_fixture() {
    let root = tempfile::tempdir().unwrap();
    let engine = fixture_engine(root.path(), false, Arc::new(AtomicUsize::new(0)));
    let mut r = render_request(root.path());
    r.adjustments.exposure_ev = 0.2;
    r.adjustments.hsl[0].hue = 20.;
    r.adjustments.hsl[1].saturation = -15.;
    r.adjustments
        .curve
        .master
        .insert(1, CurvePoint { x: 0.5, y: 0.55 });
    r.adjustments.presence = Presence {
        texture: 10.,
        clarity: 8.,
        dehaze: 5.,
    };
    r.adjustments.detail.sharpening.amount = 20.;
    r.adjustments.detail.noise.color = 10.;
    r.adjustments.optics.manual_distortion = 5.;
    r.adjustments.optics.manual_vignette = 10.;
    r.adjustments.effects.vignette.amount = -15.;
    r.adjustments.rotation_degrees = 90.;
    r.adjustments.crop = Crop {
        x: 0.1,
        y: 0.1,
        width: 0.8,
        height: 0.8,
    };
    let mut l = layer(MaskType::Subject);
    l.adjustments.exposure_ev = 0.3;
    l.adjustments.temperature = 6700.;
    l.adjustments.presence.texture = -10.;
    r.adjustments.local_layers = vec![l];
    engine
        .mask_preview(
            &r.original,
            &r.source_metadata,
            &r.adjustments,
            None,
            true,
            &token(),
        )
        .unwrap();
    engine.render(&r, &token()).unwrap();
    let export = std::fs::read(&r.destination).unwrap();
    r.preview = true;
    r.destination = root.path().join("same-resolution-preview.tif");
    engine.render(&r, &token()).unwrap();
    assert_eq!(export, std::fs::read(&r.destination).unwrap());
}
