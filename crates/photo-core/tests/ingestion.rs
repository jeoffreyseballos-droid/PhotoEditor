//! All raster/container fixtures are synthetic and generated locally; no camera photos.
use photo_contracts::{formats::PHOTO_FORMATS, GpuInfo, GpuProbe};
use photo_core::{
    discovery::{inspect_file, supported_type, FileDiscovery},
    external::{metadata_from_json, ExifTool},
    jobs::JobService,
    metadata::{BasicMetadataExtractor, MetadataExtractor},
    models::NewJob,
    resources::{memory_classification, snapshot_with},
    thumbnails::{EmbeddedPreviewProvider, ThumbnailService},
    warnings::{IngestionWarning, WarningCategory},
};
use std::{
    fs,
    io::{Cursor, Seek, SeekFrom, Write},
    path::Path,
};

fn helper() -> ExifTool {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.resources/exiftool");
    assert!(
        root.join("LICENSE").is_file(),
        "Run npm ci and npm run prepare:native before bundled-helper tests"
    );
    ExifTool::new(root)
}
fn jpeg() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::RgbImage::from_pixel(48, 32, image::Rgb([40, 100, 160]))
        .write_to(&mut bytes, image::ImageFormat::Jpeg)
        .unwrap();
    bytes.into_inner()
}
fn atom(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = ((data.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend(name);
    v.extend(data);
    v
}
fn tiff(entries: &[(u16, u16, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = b"II\x2a\0\x08\0\0\0".to_vec();
    out.extend((entries.len() as u16).to_le_bytes());
    let mut extra: Vec<u8> = Vec::new();
    let base = 8 + 2 + entries.len() * 12 + 4;
    for (tag, kind, count, data) in entries {
        out.extend(tag.to_le_bytes());
        out.extend(kind.to_le_bytes());
        out.extend(count.to_le_bytes());
        if data.len() <= 4 {
            out.extend(data);
            out.extend(vec![0; 4 - data.len()]);
        } else {
            out.extend(((base + extra.len()) as u32).to_le_bytes());
            extra.extend(data);
        }
    }
    out.extend(0u32.to_le_bytes());
    out.extend(extra);
    out
}
fn synthetic_cr3(preview: bool) -> Vec<u8> {
    let mut result = atom(b"ftyp", b"crx \0\0\0\0crx isom");
    let ifd0 = tiff(&[
        (0x100, 4, 1, 6000u32.to_le_bytes().to_vec()),
        (0x101, 4, 1, 4000u32.to_le_bytes().to_vec()),
        (0x10f, 2, 6, b"Canon\0".to_vec()),
        (0x110, 2, 13, b"EOS Test RAW\0".to_vec()),
        (0x112, 3, 1, 6u16.to_le_bytes().to_vec()),
    ]);
    let exif = tiff(&[
        (0x8827, 3, 1, 400u16.to_le_bytes().to_vec()),
        (0xa434, 2, 10, b"Test Lens\0".to_vec()),
        (0x9003, 2, 20, b"2026:09:03 12:34:56\0".to_vec()),
    ]);
    let mut uuid = vec![
        0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a,
        0x48,
    ];
    uuid.extend(atom(b"CNCV", b"CanonCR3_001/00"));
    uuid.extend(atom(b"CMT1", &ifd0));
    uuid.extend(atom(b"CMT2", &exif));
    result.extend(atom(b"moov", &atom(b"uuid", &uuid)));
    if preview {
        let mut prvw = vec![
            0xea, 0xf4, 0x2b, 0x5e, 0x1c, 0x98, 0x4b, 0x88, 0xb9, 0xfb, 0xb7, 0xdc, 0x40, 0x6e,
            0x4d, 0x16,
        ];
        prvw.resize(48, 0);
        prvw.extend(jpeg());
        result.extend(atom(b"uuid", &prvw));
    }
    result
}

#[test]
fn registry_covers_only_the_requested_still_photo_families() {
    let expected = [
        "cr3", "cr2", "nef", "arw", "dng", "raf", "orf", "rw2", "pef", "jpg", "jpeg", "tif",
        "tiff", "png", "heic", "heif",
    ];
    assert_eq!(PHOTO_FORMATS.len(), expected.len());
    for ext in expected {
        for ext in [ext.to_owned(), ext.to_uppercase()] {
            let format = supported_type(Path::new(&format!("image.{ext}")))
                .unwrap()
                .format();
            assert!(format.discoverable);
            assert!(format.editable_future);
        }
    }
    for ext in [
        "mp4",
        "mov",
        "avi",
        "mkv",
        "gif",
        "svg",
        "ico",
        "webp",
        "pdf",
        "psd",
        "psb",
        "lrcat",
        "lrcat-data",
        "cocatalog",
        "cosessiondb",
        "luminar",
        "afphoto",
        "afdesign",
        "xmp",
    ] {
        assert!(supported_type(Path::new(&format!("image.{ext}"))).is_none());
    }
}

#[test]
fn directories_and_non_photos_are_quiet_and_all_still_extensions_are_discovered() {
    let root = tempfile::tempdir().unwrap();
    for format in PHOTO_FORMATS {
        fs::write(
            root.path().join(format!("photo.{}", format.extension)),
            b"synthetic corrupt file retained",
        )
        .unwrap();
    }
    for name in ["folder.jpg", "nested", "nested/another.CR3"] {
        fs::create_dir_all(root.path().join(name)).unwrap();
    }
    for name in ["movie.mp4", "image.psd", "document.afphoto", "image.gif"] {
        fs::write(root.path().join(name), b"ignored").unwrap();
    }
    let files = FileDiscovery::new(root.path(), vec![])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(files.len(), 16);
    assert!(files.iter().all(|f| f.original_path.is_file()));
}

#[test]
fn animated_png_and_heif_sequences_are_not_photos() {
    let root = tempfile::tempdir().unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(8u32.to_be_bytes());
    png.extend(b"acTL");
    png.extend([0; 12]);
    fs::write(root.path().join("animated.png"), png).unwrap();
    fs::write(
        root.path().join("sequence.heif"),
        atom(b"ftyp", b"msf1\0\0\0\0msf1"),
    )
    .unwrap();
    assert_eq!(FileDiscovery::new(root.path(), vec![]).unwrap().count(), 0);
}

#[test]
fn nested_output_exclusion_survives_rescan_and_path_aliases() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("shoot");
    let output = input.join("Output");
    fs::create_dir_all(output.join("deep")).unwrap();
    fs::write(input.join("original.jpg"), jpeg()).unwrap();
    fs::write(output.join("deep/export.jpg"), jpeg()).unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let request = NewJob {
        name: "Nested output".into(),
        input_path: input.join("."),
        output_path: output.join("."),
    };
    let (job, permit) = service.create(request.clone()).unwrap();
    service.scan(&job.id, permit).unwrap();
    fs::write(output.join("export2.jpg"), jpeg()).unwrap();
    let (_, permit) = service.resume(&job.id).unwrap();
    service.scan(&job.id, permit).unwrap();
    let stored = service.repository.get_job(&job.id).unwrap();
    assert_eq!(stored.asset_count, 1);
    assert_eq!(stored.warnings.total(), 0);
    assert!(service
        .create(NewJob {
            output_path: input.clone(),
            ..request.clone()
        })
        .is_err());
    assert!(service
        .create(NewJob {
            input_path: output.clone(),
            output_path: input.clone(),
            ..request.clone()
        })
        .is_err());
    #[cfg(windows)]
    {
        assert!(service
            .create(NewJob {
                output_path: std::path::PathBuf::from(input.to_string_lossy().to_uppercase()),
                ..request
            })
            .is_err());
    }
}

#[cfg(unix)]
#[test]
fn symlink_folder_identity_cannot_bypass_overlap_validation() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    fs::create_dir(&input).unwrap();
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&input, &alias).unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    assert!(service
        .create(NewJob {
            name: "Alias".into(),
            input_path: input,
            output_path: alias
        })
        .is_err());
}

#[test]
fn bundled_helper_reads_cr3_container_metadata_and_preview_without_raw_pixels() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Camera — 日本語.CR3");
    fs::write(&path, synthetic_cr3(true)).unwrap();
    let before = fs::read(&path).unwrap();
    let tool = helper();
    let file = inspect_file(&path).unwrap();
    let result = tool.extract(&file);
    assert_eq!(
        result.metadata.camera_make.as_deref(),
        Some("Canon"),
        "{:?} / {:?}",
        result.warning,
        result.metadata
    );
    assert_eq!(
        result.metadata.camera_model.as_deref(),
        Some("EOS Test RAW")
    );
    assert_eq!(result.metadata.lens.as_deref(), Some("Test Lens"));
    assert_eq!(result.metadata.iso, Some(400));
    assert_eq!(result.metadata.width, Some(6000));
    assert_eq!(result.metadata.orientation, Some(6));
    assert_eq!(tool.jpeg_preview(&path).unwrap(), Some(jpeg()));
    let mut thumbnails = ThumbnailService::new(root.path().join("cache")).unwrap();
    thumbnails.set_preview_provider(Box::new(tool));
    let thumb = thumbnails.generate(&file, result.metadata.orientation);
    assert_eq!(thumb.status, "ready");
    let (w, h) = image::image_dimensions(thumb.path.unwrap()).unwrap();
    assert_eq!(w * 3, h * 2);
    assert!(h <= 384);
    assert_eq!(before, fs::read(path).unwrap());
}

#[test]
fn missing_raw_preview_and_heif_codec_preserve_metadata_and_asset_with_capability_warning() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(input.join("metadata-only.cr3"), synthetic_cr3(false)).unwrap();
    // Minimal still-HEIF container, with no pixel item or embedded JPEG.
    fs::write(
        input.join("no-codec.heic"),
        atom(b"ftyp", b"heic\0\0\0\0mif1heic"),
    )
    .unwrap();
    let helper_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.resources/exiftool");
    let service = JobService::with_exiftool(
        root.path().join("data"),
        root.path().join("cache"),
        helper_root,
    )
    .unwrap();
    let (job, permit) = service
        .create(NewJob {
            name: "Partial capabilities".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    service.scan(&job.id, permit).unwrap();
    let assets = service.repository.assets(&job.id, 0, 100).unwrap();
    assert_eq!(assets.total, 2);
    let raw = assets.items.iter().find(|a| a.file_type.is_raw()).unwrap();
    assert_eq!(raw.metadata.camera_make.as_deref(), Some("Canon"));
    for asset in assets.items {
        assert_ne!(asset.preview_status, "ready");
        assert!(asset
            .warnings
            .iter()
            .any(|w| w.category == WarningCategory::Preview));
    }
    assert_eq!(
        service
            .repository
            .get_job(&job.id)
            .unwrap()
            .warnings
            .preview,
        2
    );
}

#[test]
fn missing_helper_is_a_diagnostic_not_a_lost_asset() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("photo.cr3");
    fs::write(&path, synthetic_cr3(false)).unwrap();
    let result = ExifTool::new(root.path().join("missing")).extract(&inspect_file(&path).unwrap());
    assert!(result.warning.unwrap().contains("unavailable"));
}

#[test]
fn metadata_mapping_preserves_nullable_exposure_and_color_fields() {
    let value = serde_json::json!({"Make":"Test", "ExposureCompensation":"-1/3", "Orientation":6,"ColorSpace":"sRGB","ProfileDescription":"Test ICC","RawImageWidth":6240,"RawImageHeight":4160,"WhiteBalance":"Daylight","BitsPerSample":"16 16 16", "LensMake":"Test optics"});
    let m = metadata_from_json(&value);
    assert_eq!(m.bit_depth, Some(16));
    assert_eq!(m.raw_width, Some(6240));
    assert_eq!(m.exposure_compensation.as_deref(), Some("-1/3"));
    assert_eq!(m.color_profile.as_deref(), Some("Test ICC"));
    assert!(m.iso.is_none());
    assert!(m.capture_timestamp.is_none());
}

#[test]
fn large_tiff_with_end_of_file_ifd_uses_bounded_strips() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("professional.tiff");
    // Genuine 6000 x 7000 RGB8 raster = 126 MB, stored sparsely as black strips.
    // Its IFD is beyond the old 8 MiB metadata prefix. No source-sized allocation.
    let (w, h, rows) = (6000u32, 7000u32, 100u32);
    let count = h / rows;
    let strip_bytes = w * rows * 3;
    let ifd_offset = 8 + w * h * 3;
    let mut file = fs::File::create(&path).unwrap();
    file.write_all(b"II\x2a\0").unwrap();
    file.write_all(&ifd_offset.to_le_bytes()).unwrap();
    file.set_len(ifd_offset as u64).unwrap();
    file.seek(SeekFrom::Start(ifd_offset as u64)).unwrap();
    let n = 10u16;
    let extra = ifd_offset + 2 + u32::from(n) * 12 + 4;
    file.write_all(&n.to_le_bytes()).unwrap();
    for (tag, kind, n, value) in [
        (256u16, 4u16, 1, w),
        (257, 4, 1, h),
        (258, 3, 3, extra),
        (259, 3, 1, 1),
        (262, 3, 1, 2),
        (273, 4, count, extra + 6),
        (277, 3, 1, 3),
        (278, 4, 1, rows),
        (279, 4, count, extra + 6 + count * 4),
        (284, 3, 1, 1),
    ] {
        file.write_all(&tag.to_le_bytes()).unwrap();
        file.write_all(&kind.to_le_bytes()).unwrap();
        file.write_all(&n.to_le_bytes()).unwrap();
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    file.write_all(&0u32.to_le_bytes()).unwrap();
    file.write_all(&[8, 0, 8, 0, 8, 0]).unwrap();
    for i in 0..count {
        file.write_all(&(8 + i * strip_bytes).to_le_bytes())
            .unwrap();
    }
    for _ in 0..count {
        file.write_all(&strip_bytes.to_le_bytes()).unwrap();
    }
    drop(file);
    let source = inspect_file(&path).unwrap();
    assert!(source.file_size > 116_000_000);
    let metadata = BasicMetadataExtractor.extract(&source);
    assert_eq!(metadata.metadata.width, Some(w));
    assert_eq!(metadata.metadata.height, Some(h));
    let thumb = ThumbnailService::new(root.path().join("cache"))
        .unwrap()
        .generate(&source, None);
    assert_eq!(thumb.status, "ready", "{:?}", thumb.warning);
    let size = image::image_dimensions(thumb.path.unwrap()).unwrap();
    assert!(size.0 <= 384 && size.1 <= 384);
}

#[test]
fn warning_categories_persist_and_detail_pages_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(input.join("broken.jpg"), b"corrupt").unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = service
        .create(NewJob {
            name: "Warnings".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    service.scan(&job.id, permit).unwrap();
    let summary = service.repository.get_job(&job.id).unwrap();
    assert_eq!(summary.asset_count, 1);
    assert_eq!(summary.warnings.metadata, 1);
    assert_eq!(summary.warnings.preview, 1);
    assert_eq!(summary.warnings.traversal, 0);
    for category in [
        WarningCategory::Unreadable,
        WarningCategory::Access,
        WarningCategory::Traversal,
    ] {
        service
            .repository
            .save_scan_warning(
                &job.id,
                &IngestionWarning::new(category, "fixture", "Synthetic diagnostic"),
            )
            .unwrap();
    }
    let job = service.repository.get_job(&job.id).unwrap();
    assert_eq!(job.warnings.total(), 6);
    assert_eq!(job.warning_count, 6);
    let first = service.repository.warnings(&job.id, 0, 2).unwrap();
    assert_eq!(first.total, 6);
    assert_eq!(first.items.len(), 2);
    assert_eq!(
        service.repository.warnings(&job.id, 2, 1000).unwrap().limit,
        100
    );
}

struct FailedGpu;
#[cfg(windows)]
#[test]
fn locked_supported_file_is_retained_and_rescan_clears_recovered_warnings() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    let photo = input.join("locked.jpg");
    fs::write(&photo, jpeg()).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&photo)
        .unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = service
        .create(NewJob {
            name: "Locked photo".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    service.scan(&job.id, permit).unwrap();
    let stored = service.repository.get_job(&job.id).unwrap();
    assert_eq!(stored.status, "ready");
    assert_eq!(stored.asset_count, 1);
    assert!(stored.warnings.unreadable + stored.warnings.access > 0);
    drop(lock);
    let (_, permit) = service.resume(&job.id).unwrap();
    service.scan(&job.id, permit).unwrap();
    let recovered = service.repository.get_job(&job.id).unwrap();
    assert_eq!(recovered.asset_count, 1);
    assert_eq!(recovered.warnings.total(), 0);
}

#[test]
fn png_and_16_bit_editor_created_tiff_have_previews() {
    let root = tempfile::tempdir().unwrap();
    let png = root.path().join("still.PNG");
    image::RgbImage::from_pixel(64, 32, image::Rgb([20, 120, 200]))
        .save_with_format(&png, image::ImageFormat::Png)
        .unwrap();
    let tiff = root.path().join("editor-created.tif");
    let mut encoder = tiff::encoder::TiffEncoder::new(fs::File::create(&tiff).unwrap()).unwrap();
    let mut output = encoder
        .new_image::<tiff::encoder::colortype::RGB16>(64, 32)
        .unwrap();
    output
        .encoder()
        .write_tag(
            tiff::tags::Tag::Software,
            "Adobe Photoshop (synthetic test)",
        )
        .unwrap();
    output.write_data(&vec![32000u16; 64 * 32 * 3]).unwrap();
    let thumbnails = ThumbnailService::new(root.path().join("cache")).unwrap();
    for path in [png, tiff] {
        let file = inspect_file(&path).unwrap();
        let metadata = BasicMetadataExtractor.extract(&file);
        assert_eq!(metadata.metadata.width, Some(64));
        assert_eq!(thumbnails.generate(&file, None).status, "ready");
    }
}

#[test]
fn upgrading_phase_one_preserves_jobs_assets_and_future_checkpoints() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("jobs.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    db.execute_batch("INSERT INTO jobs(id,name,input_path,output_path,created_at,updated_at,status) VALUES ('legacy','Legacy','input','output','created','updated','ready'); INSERT INTO assets(id,job_id,original_path,filename,file_type,file_size,fingerprint,metadata_json,preview_status,metadata_warning,created_at) VALUES ('asset','legacy','original.cr3','original.cr3','cr3',100,'fingerprint','{}','unavailable','Old combined message','created'); INSERT INTO processing_state(job_id,asset_id,stage,updated_at) VALUES ('legacy','asset','rendered','updated');").unwrap();
    let repo = photo_core::repository::JobRepository::open(path).unwrap();
    let job = repo.get_job("legacy").unwrap();
    assert_eq!(job.asset_count, 1);
    assert_eq!(job.warnings.metadata, 1);
    let asset = repo.asset("legacy", "asset").unwrap();
    assert_eq!(asset.warnings[0].code, "legacy_inspection");
    assert_eq!(asset.created_at, "created");
    let stage: String = db
        .query_row("SELECT stage FROM processing_state", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stage, "rendered");
}

#[test]
fn old_version_ready_cache_is_repaired_on_page_open() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(input.join("photo.jpg"), jpeg()).unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = service
        .create(NewJob {
            name: "Upgrade".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    service.scan(&job.id, permit).unwrap();
    let asset = service
        .repository
        .assets(&job.id, 0, 1)
        .unwrap()
        .items
        .remove(0);
    let current = asset.thumbnail_path.unwrap();
    let old = current.with_file_name(
        current
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replacen("v2-", "v1-", 1),
    );
    fs::rename(&current, &old).unwrap();
    let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
    db.execute(
        "UPDATE assets SET thumbnail_path=?1",
        [old.to_str().unwrap()],
    )
    .unwrap();
    let repaired = service.assets(&job.id, 0, 1).unwrap().items.remove(0);
    assert_eq!(repaired.thumbnail_path.as_ref(), Some(&current));
    assert!(current.exists());
    assert!(service
        .thumbnail_data(&job.id, &repaired.id)
        .unwrap()
        .is_some());
}

#[test]
fn only_folders_never_create_assets_or_warnings() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    fs::create_dir_all(input.join("named.jpg/named.cr3")).unwrap();
    fs::create_dir(&output).unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let (job, permit) = service
        .create(NewJob {
            name: "Folders only".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    service.scan(&job.id, permit).unwrap();
    let job = service.repository.get_job(&job.id).unwrap();
    assert_eq!(job.asset_count, 0);
    assert_eq!(job.warnings.total(), 0);
}

impl GpuProbe for FailedGpu {
    fn detect(&self) -> Result<Vec<GpuInfo>, String> {
        Err("Not available".into())
    }
}
#[test]
fn gpu_failure_and_memory_classification_do_not_invent_vram() {
    let snapshot = snapshot_with(&FailedGpu);
    assert!(snapshot.gpus.is_empty());
    assert!(snapshot.gpu_memory_bytes.is_none());
    assert!(snapshot.total_ram_bytes > 0);
    assert_eq!(snapshot.gpu_detection, "Not available");
    assert_eq!(memory_classification(Some(true)), ("integrated", "shared"));
    assert_eq!(
        memory_classification(Some(false)),
        ("discrete", "dedicated")
    );
    assert_eq!(memory_classification(None), ("unknown", "unknown"));
}
