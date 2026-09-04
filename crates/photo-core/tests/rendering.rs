use image::ImageDecoder;
use photo_contracts::*;
use photo_core::{
    development::{DevelopmentRequest, DevelopmentService},
    jobs::JobService,
    models::NewJob,
    rendering::{
        self,
        decode::{Decoded, LibRawDecoder, RawDecoder},
        pixels::{self, FloatImage},
        CpuProcessingEngine, RenderLimits,
    },
};
use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tempfile::{tempdir, TempDir};
struct MockRaw {
    count: Arc<AtomicUsize>,
}
impl RawDecoder for MockRaw {
    fn id(&self) -> &str {
        "test-only-not-camera-support"
    }
    fn decode(
        &self,
        _: &Path,
        preview: bool,
        _: RenderLimits,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        cancel.check()?;
        self.count.fetch_add(1, Ordering::AcqRel);
        let (w, h) = if preview { (40, 30) } else { (80, 60) };
        let mut image = FloatImage::blank(w, h, 10000)?;
        image.pixels.fill([0.18, 0.3, 0.4]);
        Ok(Decoded {
            image,
            warnings: vec![],
        })
    }
}
fn engine() -> CpuProcessingEngine {
    CpuProcessingEngine::new(
        Box::new(MockRaw {
            count: Arc::new(AtomicUsize::new(0)),
        }),
        RenderLimits::default(),
    )
}
fn request(root: &Path, source: &Path, format: OutputFormat) -> RenderRequest {
    RenderRequest {
        asset_id: "test".into(),
        original: source.into(),
        adjustments: Default::default(),
        source_metadata: Default::default(),
        destination: root.join(format!("export.{}", format.extension())),
        output_format: format,
        preview: false,
        jpeg_quality: 95,
    }
}
fn raster(path: &Path, w: u32, h: u32) {
    image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 220) as u8, (y % 210) as u8, 130])
    })
    .save(path)
    .unwrap();
}
fn approximate(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}

#[test]
fn neutral_is_stable_and_exposure_is_stops() {
    let p = [0.15, 0.20, 0.3];
    let mut image = FloatImage {
        width: 1,
        height: 1,
        pixels: vec![p],
    };
    let cancel = CancellationToken::default();
    pixels::apply(&mut image, &Default::default(), &cancel).unwrap();
    assert_eq!(image.pixels[0], p);
    let a = RenderAdjustments {
        exposure_ev: 1.,
        ..Default::default()
    };
    pixels::apply(&mut image, &a, &cancel).unwrap();
    for (c, value) in p.iter().enumerate() {
        approximate(image.pixels[0][c], value * 2.);
    }
    let a = RenderAdjustments {
        exposure_ev: 5.,
        ..Default::default()
    };
    pixels::apply(&mut image, &a, &cancel).unwrap();
    assert!(image.pixels[0][2] > 1.);
}
#[test]
fn validation_rejects_nonfinite_out_of_range_and_bad_crop() {
    let base = serde_json::to_value(RenderAdjustments::default()).unwrap();
    for field in [
        "contrast",
        "highlights",
        "shadows",
        "whites",
        "blacks",
        "saturation",
        "vibrance",
        "tint",
    ] {
        for value in [-101., 101.] {
            let mut json = base.clone();
            json[field] = serde_json::json!(value);
            assert!(serde_json::from_value::<RenderAdjustments>(json)
                .unwrap()
                .validated()
                .is_err());
        }
    }
    for t in [1999., 12001., f32::NAN, f32::INFINITY] {
        assert!(RenderAdjustments {
            temperature: t,
            ..Default::default()
        }
        .validated()
        .is_err());
    }
    for crop in [
        Crop {
            x: 0.8,
            y: 0.,
            width: 0.3,
            height: 1.,
        },
        Crop {
            x: 0.,
            y: 0.,
            width: 0.,
            height: 1.,
        },
        Crop {
            x: 0.,
            y: -0.1,
            width: 1.,
            height: 1.,
        },
    ] {
        assert!(RenderAdjustments {
            crop,
            ..Default::default()
        }
        .validated()
        .is_err());
    }
    assert_eq!(
        RenderAdjustments {
            rotation_degrees: 450.,
            ..Default::default()
        }
        .validated()
        .unwrap()
        .rotation_degrees,
        90.
    );
}
#[test]
fn wb_warms_cools_and_tints_without_overlay() {
    let run = |temperature, tint| {
        let mut i = FloatImage {
            width: 1,
            height: 1,
            pixels: vec![[0.3; 3]],
        };
        pixels::apply(
            &mut i,
            &RenderAdjustments {
                temperature,
                tint,
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
        i.pixels[0]
    };
    let warm = run(9000., 0.);
    let cool = run(3500., 0.);
    assert!(warm[0] / warm[2] > 1.);
    assert!(cool[0] / cool[2] < 1.);
    let tint = run(6500., 60.);
    assert!(tint[1] < (tint[0] + tint[2]) / 2.);
}
#[test]
fn tone_zones_are_not_global_brightness_aliases() {
    let apply = |a| {
        let mut i = FloatImage {
            width: 2,
            height: 1,
            pixels: vec![[0.03; 3], [0.85; 3]],
        };
        pixels::apply(&mut i, &a, &Default::default()).unwrap();
        i.pixels
    };
    let shadows = apply(RenderAdjustments {
        shadows: 80.,
        ..Default::default()
    });
    assert!(shadows[0][0] / 0.03 > shadows[1][0] / 0.85);
    let highlights = apply(RenderAdjustments {
        highlights: -80.,
        ..Default::default()
    });
    assert!(highlights[0][0] / 0.03 > highlights[1][0] / 0.85);
}
#[test]
fn vibrance_protects_saturated_colors_and_saturation_can_remove_color() {
    let original = FloatImage {
        width: 1,
        height: 1,
        pixels: vec![[0.8, 0., 0.]],
    };
    let mut vibrant = original.clone();
    pixels::apply(
        &mut vibrant,
        &RenderAdjustments {
            vibrance: 100.,
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap();
    assert_eq!(vibrant.pixels, original.pixels);
    pixels::apply(
        &mut vibrant,
        &RenderAdjustments {
            saturation: -100.,
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap();
    approximate(vibrant.pixels[0][0], vibrant.pixels[0][1]);
}
#[test]
fn geometry_rotates_and_crops_deterministically() {
    let image = FloatImage {
        width: 3,
        height: 2,
        pixels: (0..6).map(|i| [i as f32; 3]).collect(),
    };
    let rotated = pixels::geometry(
        image.clone(),
        &RenderAdjustments {
            rotation_degrees: 90.,
            ..Default::default()
        },
        100,
        &Default::default(),
    )
    .unwrap();
    assert_eq!((rotated.width, rotated.height), (2, 3));
    approximate(rotated.pixels[0][0], 3.);
    let crop = pixels::geometry(
        image,
        &RenderAdjustments {
            crop: Crop {
                x: 1. / 3.,
                y: 0.,
                width: 2. / 3.,
                height: 1.,
            },
            ..Default::default()
        },
        100,
        &Default::default(),
    )
    .unwrap();
    assert_eq!((crop.width, crop.height), (2, 2));
    approximate(crop.pixels[0][0], 1.);
}
#[test]
fn spatial_stages_are_finite_and_cancel_ready() {
    let mut image = FloatImage::blank(12, 8, 1000).unwrap();
    image.pixels.fill([0.2; 3]);
    image.pixels[10] = [0.25; 3];
    pixels::apply(
        &mut image,
        &RenderAdjustments {
            noise_reduction: 100.,
            sharpening: 40.,
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap();
    assert!(image.pixels.iter().flatten().all(|v| v.is_finite()));
    assert!(image.pixels[10][0] < 0.25);
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert_eq!(
        pixels::apply(&mut image, &Default::default(), &cancel)
            .unwrap_err()
            .code,
        ProcessingErrorCode::Cancelled
    );
}
#[test]
fn cache_keys_include_parameters_identity_and_backend() {
    let a = RenderAdjustments::default();
    let key = rendering::preview_key("source-a", &a, "backend-a").unwrap();
    assert_eq!(
        key,
        rendering::preview_key("source-a", &a, "backend-a").unwrap()
    );
    for other in [
        rendering::preview_key("source-b", &a, "backend-a").unwrap(),
        rendering::preview_key("source-a", &a, "backend-b").unwrap(),
        rendering::preview_key(
            "source-a",
            &RenderAdjustments {
                exposure_ev: 1.,
                ..a
            },
            "backend-a",
        )
        .unwrap(),
    ] {
        assert_ne!(key, other);
    }
}
#[test]
fn jpeg_and_16bit_tiff_export_embed_icc_and_preserve_source() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.png");
    raster(&source, 88, 66);
    let bytes = fs::read(&source).unwrap();
    for format in [OutputFormat::Jpeg, OutputFormat::Tiff] {
        let r = request(root.path(), &source, format);
        let result = engine().render(&r, &Default::default()).unwrap();
        assert_eq!((result.width, result.height), (88, 66));
        let mut decoder = image::ImageReader::open(&result.rendered_image)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        assert!(!decoder.icc_profile().unwrap().unwrap().is_empty());
        if format == OutputFormat::Tiff {
            assert_eq!(decoder.color_type(), image::ColorType::Rgb16);
        }
        assert_eq!(fs::read(&source).unwrap(), bytes);
    }
}
#[test]
fn existing_destinations_and_sources_cannot_be_overwritten() {
    let root = tempdir().unwrap();
    let source = root.path().join("source.jpg");
    raster(&source, 30, 20);
    let before = fs::read(&source).unwrap();
    let mut r = request(root.path(), &source, OutputFormat::Jpeg);
    r.destination = source.clone();
    assert!(engine().render(&r, &Default::default()).is_err());
    assert_eq!(fs::read(source).unwrap(), before);
}
#[test]
fn corrupt_unsupported_missing_decoder_and_budget_fail_structurally() {
    let root = tempdir().unwrap();
    let source = root.path().join("bad.jpg");
    fs::write(&source, b"invalid").unwrap();
    assert_eq!(
        engine()
            .render(
                &request(root.path(), &source, OutputFormat::Jpeg),
                &Default::default()
            )
            .unwrap_err()
            .code,
        ProcessingErrorCode::CorruptSource
    );
    let source = root.path().join("image.heic");
    fs::write(&source, b"unsupported").unwrap();
    assert_eq!(
        engine()
            .render(
                &request(root.path(), &source, OutputFormat::Jpeg),
                &Default::default()
            )
            .unwrap_err()
            .code,
        ProcessingErrorCode::UnsupportedRenderFormat
    );
    let decoder = LibRawDecoder {
        helper: root.path().join("missing-helper"),
        scratch: root.path().join("scratch"),
    };
    assert_eq!(
        decoder
            .decode(&source, false, RenderLimits::default(), &Default::default())
            .err()
            .unwrap()
            .code,
        ProcessingErrorCode::DecoderUnavailable
    );
    assert!(FloatImage::blank(100, 100, 10).is_err());
}
#[test]
fn raw_proxy_cache_reused_but_export_decodes_full_source() {
    let root = tempdir().unwrap();
    let source = root.path().join("abstract.cr3");
    fs::write(&source, b"mock only").unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let engine = CpuProcessingEngine::new(
        Box::new(MockRaw {
            count: count.clone(),
        }),
        RenderLimits::default(),
    );
    let mut r = request(root.path(), &source, OutputFormat::Jpeg);
    r.preview = true;
    let p = engine.render(&r, &Default::default()).unwrap();
    assert_eq!((p.width, p.height), (40, 30));
    r.destination = root.path().join("second.jpg");
    r.adjustments.exposure_ev = 1.;
    engine.render(&r, &Default::default()).unwrap();
    assert_eq!(count.load(Ordering::Acquire), 1);
    r.preview = false;
    r.destination = root.path().join("full.jpg");
    let p = engine.render(&r, &Default::default()).unwrap();
    assert_eq!((p.width, p.height), (80, 60));
    assert_eq!(count.load(Ordering::Acquire), 2);
}
fn setup() -> (TempDir, JobService, DevelopmentService, String, String) {
    let root = tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    raster(&input.join("portrait.png"), 40, 30);
    let jobs = JobService::new(root.path().join("data"), root.path().join("source-cache")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Development".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&job.id, permit).unwrap();
    let asset = jobs.repository.assets(&job.id, 0, 1).unwrap().items[0]
        .id
        .clone();
    let dev = DevelopmentService::new(
        jobs.repository.clone(),
        Arc::new(engine()),
        root.path().join("render-cache"),
        None,
    )
    .unwrap();
    (root, jobs, dev, job.id, asset)
}
fn dev_request(job: &str, asset: &str, preview: bool) -> DevelopmentRequest {
    DevelopmentRequest {
        job_id: job.into(),
        asset_id: asset.into(),
        request_id: uuid::Uuid::new_v4().to_string(),
        adjustments: Default::default(),
        preview,
        output_format: OutputFormat::Jpeg,
        jpeg_quality: 95,
    }
}
#[test]
fn persistence_preview_regeneration_exports_collisions_and_checkpoint() {
    let (root, jobs, dev, job, asset) = setup();
    let adjustments = RenderAdjustments {
        exposure_ev: 1.2,
        ..Default::default()
    };
    dev.save(&job, &asset, &adjustments).unwrap();
    assert_eq!(dev.load(&job, &asset).unwrap().adjustments, adjustments);
    let reopened =
        photo_core::repository::JobRepository::open(root.path().join("data/jobs.sqlite3")).unwrap();
    assert_eq!(
        reopened.development(&job, &asset).unwrap().adjustments,
        adjustments
    );
    let r = dev_request(&job, &asset, true);
    let p = dev
        .run(r.clone(), dev.reserve(&r.request_id, true).unwrap())
        .unwrap();
    assert!(p
        .preview_data
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
    let cache = p.state.preview_path.unwrap();
    fs::remove_file(cache).unwrap();
    let r = dev_request(&job, &asset, true);
    dev.run(r.clone(), dev.reserve(&r.request_id, true).unwrap())
        .unwrap();
    let mut names = vec![];
    for _ in 0..2 {
        let r = dev_request(&job, &asset, false);
        let result = dev
            .run(r.clone(), dev.reserve(&r.request_id, false).unwrap())
            .unwrap();
        assert_eq!(result.state.state, "exported");
        let path = result.state.export_path.unwrap();
        assert!(path.is_file());
        names.push(path.file_name().unwrap().to_string_lossy().to_string());
    }
    assert_eq!(names, ["portrait-edited.jpg", "portrait-edited-2.jpg"]);
    assert_eq!(jobs.repository.assets(&job, 0, 10).unwrap().total, 1);
}
#[test]
fn cancelled_requests_and_bounded_queue_do_not_publish() {
    let (_root, _jobs, dev, job, asset) = setup();
    let one = dev.reserve("one", true).unwrap();
    let two = dev.reserve("two", true).unwrap();
    assert!(one.token.is_cancelled());
    assert!(dev.reserve("three", true).is_err());
    assert!(dev.reserve("export", false).is_err());
    drop(one);
    dev.cancel("two").unwrap();
    let mut r = dev_request(&job, &asset, true);
    r.request_id = "two".into();
    assert_eq!(
        dev.run(r, two).err().unwrap().code,
        ProcessingErrorCode::Cancelled
    );
    assert!(dev.load(&job, &asset).unwrap().export_path.is_none());
}

fn synthetic_dng(path: &Path) {
    use tiff::{
        encoder::{colortype::Gray16, TiffEncoder},
        tags::Tag,
    };
    let mut file = fs::File::create(path).unwrap();
    let mut encoder = TiffEncoder::new(&mut file).unwrap();
    let mut image = encoder.new_image::<Gray16>(128, 96).unwrap();
    let tags = image.encoder();
    tags.write_tag(Tag::PhotometricInterpretation, 32803u16)
        .unwrap();
    tags.write_tag(Tag::Make, "PhotoEditor Test Camera")
        .unwrap();
    tags.write_tag(Tag::Model, "Synthetic Bayer").unwrap();
    tags.write_tag(
        Tag::ImageDescription,
        "C:/private/source/path - do not copy",
    )
    .unwrap();
    tags.write_tag(Tag::Unknown(50706), [1u8, 4, 0, 0].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50707), [1u8, 1, 0, 0].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50708), "PhotoEditor synthetic Bayer fixture")
        .unwrap();
    tags.write_tag(Tag::Unknown(33421), [2u16, 2].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(33422), [0u8, 1, 1, 2].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50717), 4095u32).unwrap();
    tags.write_tag(Tag::Unknown(50714), 64u16).unwrap();
    tags.write_tag(
        Tag::Unknown(50721),
        [1f64, 0., 0., 0., 1., 0., 0., 0., 1.].as_slice(),
    )
    .unwrap();
    tags.write_tag(Tag::Unknown(50728), [1f64, 1., 1.].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50778), 21u16).unwrap();
    let pixels: Vec<u16> = (0..128 * 96)
        .map(|i| 512 + ((i % 128) * 12) as u16)
        .collect();
    image.write_data(&pixels).unwrap();
}
#[test]
fn real_libraw_decodes_generated_dng_not_a_camera_compatibility_claim() {
    let helper = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.resources/raw")
        .join(if cfg!(windows) {
            "photo-raw-helper.exe"
        } else {
            "photo-raw-helper"
        });
    assert!(
        helper.is_file(),
        "Run npm run prepare:native before native integration tests"
    );
    let root = tempdir().unwrap();
    let source = root.path().join("synthetic 日本語.dng");
    synthetic_dng(&source);
    let source = source.canonicalize().unwrap();
    let original = fs::read(&source).unwrap();
    let decoder = LibRawDecoder {
        helper,
        scratch: root.path().join("scratch"),
    };
    let decoded = decoder
        .decode(&source, false, RenderLimits::default(), &Default::default())
        .unwrap();
    assert!(decoded.image.width >= 120);
    assert!(decoded.image.height >= 88);
    assert!(decoded.image.pixels.iter().flatten().any(|v| *v > 0.));
    assert_eq!(original, fs::read(source).unwrap());
}
#[test]
fn tiff_can_be_reopened_with_high_precision_pixels() {
    let root = tempdir().unwrap();
    let source = root.path().join("sixteen.tif");
    let values: Vec<u16> = (0..32 * 24 * 3).map(|i| (i * 23) as u16).collect();
    {
        let mut f = fs::File::create(&source).unwrap();
        tiff::encoder::TiffEncoder::new(&mut f)
            .unwrap()
            .write_image::<tiff::encoder::colortype::RGB16>(32, 24, &values)
            .unwrap();
    }
    let r = request(root.path(), &source, OutputFormat::Tiff);
    engine().render(&r, &Default::default()).unwrap();
    let mut t = tiff::decoder::Decoder::new(BufReader::new(fs::File::open(r.destination).unwrap()))
        .unwrap();
    assert!(matches!(
        t.read_image().unwrap(),
        tiff::decoder::DecodingResult::U16(_)
    ));
}

#[test]
fn export_metadata_allowlist_keeps_camera_but_omits_description_and_original_changes() {
    let (root, jobs, _dev, job, _asset) = setup();
    let input = jobs.repository.get_job(&job).unwrap().input_path;
    let source = input.join("camera 日本語.dng");
    synthetic_dng(&source);
    let (_job, permit) = jobs.resume(&job).unwrap();
    jobs.scan(&job, permit).unwrap();
    let asset = jobs
        .repository
        .assets(&job, 0, 10)
        .unwrap()
        .items
        .into_iter()
        .find(|a| a.file_type == photo_core::models::FileType::Dng)
        .unwrap();
    let original = fs::read(&source).unwrap();
    let metadata = photo_core::external::ExifTool::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.resources/exiftool"),
    );
    let dev = DevelopmentService::new(
        jobs.repository.clone(),
        Arc::new(engine()),
        root.path().join("metadata-cache"),
        Some(metadata),
    )
    .unwrap();
    for format in [OutputFormat::Jpeg, OutputFormat::Tiff] {
        let mut r = dev_request(&job, &asset.id, false);
        r.output_format = format;
        let result = dev
            .run(r.clone(), dev.reserve(&r.request_id, false).unwrap())
            .unwrap();
        assert!(
            !result
                .state
                .warnings
                .iter()
                .any(|w| w.contains("not preserved")),
            "{:?}",
            result.state.warnings
        );
        let path = result.state.export_path.unwrap();
        let file = fs::File::open(&path).unwrap();
        let exif = exif::Reader::new()
            .read_from_container(&mut BufReader::new(file))
            .unwrap();
        assert!(exif
            .get_field(exif::Tag::Make, exif::In::PRIMARY)
            .unwrap()
            .display_value()
            .to_string()
            .contains("PhotoEditor Test Camera"));
        assert!(exif
            .get_field(exif::Tag::ImageDescription, exif::In::PRIMARY)
            .is_none());
        assert!(exif
            .get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
            .is_none());
        let mut decoder = image::ImageReader::open(path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        assert!(decoder.icc_profile().unwrap().is_some());
    }
    assert_eq!(original, fs::read(source).unwrap());
}
#[test]
fn failed_export_retains_asset_and_interrupted_recovery_retains_edits() {
    let (root, jobs, dev, job, asset) = setup();
    let source = jobs.repository.asset(&job, &asset).unwrap().original_path;
    fs::write(source, b"corrupt supported source").unwrap();
    let r = dev_request(&job, &asset, false);
    assert!(dev
        .run(r.clone(), dev.reserve(&r.request_id, false).unwrap())
        .is_err());
    let saved = dev.load(&job, &asset).unwrap();
    assert_eq!(saved.state, "failed");
    assert!(saved.export_path.is_none());
    assert!(saved.error.is_some());
    let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
    db.execute("UPDATE development_state SET state='rendering_export'", [])
        .unwrap();
    jobs.repository.recover_interrupted().unwrap();
    assert_eq!(dev.load(&job, &asset).unwrap().state, "interrupted");
    assert_eq!(jobs.repository.assets(&job, 0, 10).unwrap().total, 1);
}
#[test]
fn embedded_profile_and_orientation_are_applied_before_adjustments() {
    use image::{ExtendedColorType, ImageEncoder};
    let root = tempdir().unwrap();
    let source = root.path().join("linear-profile.jpg");
    let curve = lcms2::ToneCurve::new(1.);
    let xy = |x, y| lcms2::CIExyY { x, y, Y: 1. };
    let profile = lcms2::Profile::new_rgb(
        &xy(0.3127, 0.329),
        &lcms2::CIExyYTRIPLE {
            Red: xy(0.64, 0.33),
            Green: xy(0.3, 0.6),
            Blue: xy(0.15, 0.06),
        },
        &[&curve, &curve, &curve],
    )
    .unwrap();
    let mut file = fs::File::create(&source).unwrap();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 100);
    encoder.set_icc_profile(profile.icc().unwrap()).unwrap();
    // TIFF EXIF with a single Orientation=6 tag (90 degrees clockwise).
    let exif = vec![
        b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    encoder.set_exif_metadata(exif).unwrap();
    encoder
        .encode(&[128u8; 18], 3, 2, ExtendedColorType::Rgb8)
        .unwrap();
    drop(file);
    let decoded =
        rendering::decode::raster(&source, RenderLimits::default(), &Default::default()).unwrap();
    assert_eq!((decoded.image.width, decoded.image.height), (2, 3));
    assert!(
        (decoded.image.pixels[0][0] - 0.5).abs() < 0.02,
        "ICC linear source should remain ~0.5, not sRGB-decoded ~0.21"
    );
}

#[test]
#[ignore = "42 MP full-resolution memory/disk acceptance; run explicitly in release mode"]
fn large_tiff_full_resolution_export_uses_original_dimensions() {
    let root = tempdir().unwrap();
    let source = root.path().join("large.tif");
    {
        let mut file = fs::File::create(&source).unwrap();
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut file).unwrap();
        let mut image = encoder
            .new_image::<tiff::encoder::colortype::RGB8>(6000, 7000)
            .unwrap();
        image.rows_per_strip(32).unwrap();
        while image.next_strip_sample_count() > 0 {
            image
                .write_strip(&vec![128u8; image.next_strip_sample_count() as usize])
                .unwrap();
        }
        image.finish().unwrap();
    }
    assert!(source.metadata().unwrap().len() > 120_000_000);
    let r = request(root.path(), &source, OutputFormat::Tiff);
    let result = engine().render(&r, &Default::default()).unwrap();
    assert_eq!((result.width, result.height), (6000, 7000));
    let mut decoder =
        tiff::decoder::Decoder::new(BufReader::new(fs::File::open(&r.destination).unwrap()))
            .unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (6000, 7000));
    assert_eq!(decoder.colortype().unwrap(), tiff::ColorType::RGB(16));
    assert!(r.destination.metadata().unwrap().len() > 250_000_000);
}
