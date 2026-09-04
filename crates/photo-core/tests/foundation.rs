use photo_core::{
    discovery::{
        canonical_directory, inspect_file, stable_asset_id, supported_type, FileDiscovery,
    },
    jobs::JobService,
    metadata::{BasicMetadataExtractor, MetadataExtractor, MetadataResult, METADATA_BUDGET},
    models::{DiscoveredFile, FileType, NewJob},
    repository::JobRepository,
    thumbnails::{EmbeddedPreviewProvider, ExifEmbeddedPreview, ThumbnailService},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::{tempdir, TempDir};

fn jpeg(path: &Path) {
    image::RgbImage::from_pixel(48, 32, image::Rgb([110, 150, 90]))
        .save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

fn setup() -> (TempDir, JobService, NewJob) {
    let root = tempdir().unwrap();
    let input = root.path().join("Input photos — 日本語");
    let output = root.path().join("Output photos");
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(&output).unwrap();
    let service = JobService::new(root.path().join("data"), root.path().join("cache")).unwrap();
    let request = NewJob {
        name: " Test shoot ".into(),
        input_path: input,
        output_path: output,
    };
    (root, service, request)
}

#[test]
fn supported_extensions_are_case_insensitive_and_exact() {
    for (extension, expected) in [
        ("CR3", FileType::Cr3),
        ("NeF", FileType::Nef),
        ("arw", FileType::Arw),
        ("DNG", FileType::Dng),
        ("JPG", FileType::Jpg),
        ("jpeg", FileType::Jpeg),
    ] {
        assert_eq!(
            supported_type(&PathBuf::from(format!("image.{extension}"))),
            Some(expected)
        );
    }
    for name in [
        "image.webp",
        "image.jpg.exe",
        "no-extension",
        ".jpg",
        "image.raw",
    ] {
        assert_eq!(supported_type(Path::new(name)), None);
    }
}

#[test]
fn unicode_paths_and_dot_segments_have_stable_identity() {
    let root = tempdir().unwrap();
    let folder = root.path().join("旅行 photos");
    fs::create_dir(&folder).unwrap();
    let path = folder.join("portrait.JPG");
    jpeg(&path);
    let one = inspect_file(&path).unwrap();
    let two = inspect_file(&folder.join(".").join("portrait.JPG")).unwrap();
    assert_eq!(one.id, two.id);
    assert_eq!(one.fingerprint, two.fingerprint);
    assert_eq!(one.id, stable_asset_id(&fs::canonicalize(path).unwrap()));
    assert!(one.original_path.is_absolute());
    assert!(one.file_size > 0);
    assert!(one.modified_at.is_some());
}

#[test]
fn recursive_discovery_skips_unsupported_and_excluded_folders() {
    let root = tempdir().unwrap();
    let nested = root.path().join("nested");
    let excluded = root.path().join("cache");
    fs::create_dir(&nested).unwrap();
    fs::create_dir(&excluded).unwrap();
    jpeg(&nested.join("one.jpg"));
    fs::write(root.path().join("two.NEF"), b"recognized, not decoded").unwrap();
    fs::write(root.path().join("notes.txt"), b"ignore").unwrap();
    jpeg(&excluded.join("thumbnail.jpg"));
    let files: Vec<_> = FileDiscovery::new(root.path(), vec![fs::canonicalize(excluded).unwrap()])
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_ne!(files[0].id, files[1].id);
}

#[test]
fn rejects_relative_and_file_directories() {
    assert!(canonical_directory(Path::new("relative")).is_err());
    let root = tempdir().unwrap();
    let file = root.path().join("photo.jpg");
    jpeg(&file);
    assert!(canonical_directory(&file).is_err());
}

#[test]
fn jobs_and_assets_persist_with_no_duplicates_after_rescan() {
    let (root, service, request) = setup();
    jpeg(&request.input_path.join("one.jpg"));
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let first = service
        .repository
        .assets(&job.id, 0, 60)
        .unwrap()
        .items
        .remove(0);
    let (_, permit) = service.resume(&job.id).unwrap();
    service.scan(&job.id, permit).unwrap();
    let reopened = JobRepository::open(root.path().join("data").join("jobs.sqlite3")).unwrap();
    let persisted = reopened.get_job(&job.id).unwrap();
    assert_eq!(persisted.name, "Test shoot");
    assert_eq!(persisted.status, "ready");
    assert_eq!(persisted.asset_count, 1);
    let second = reopened.assets(&job.id, 0, 60).unwrap().items.remove(0);
    assert_eq!(first.id, second.id);
    assert_eq!(first.created_at, second.created_at);
}

#[test]
fn same_original_can_belong_to_multiple_jobs() {
    let (_root, service, request) = setup();
    jpeg(&request.input_path.join("one.jpg"));
    let (one, permit) = service.create(request.clone()).unwrap();
    service.scan(&one.id, permit).unwrap();
    let (two, permit) = service.create(request).unwrap();
    service.scan(&two.id, permit).unwrap();
    assert_ne!(one.id, two.id);
    let a = service
        .repository
        .assets(&one.id, 0, 60)
        .unwrap()
        .items
        .remove(0);
    let b = service
        .repository
        .assets(&two.id, 0, 60)
        .unwrap()
        .items
        .remove(0);
    assert_eq!(a.id, b.id);
    assert_eq!(service.repository.get_job(&one.id).unwrap().asset_count, 1);
}

#[test]
fn corrupt_metadata_does_not_prevent_other_photos_from_scanning() {
    let (_root, service, request) = setup();
    fs::write(request.input_path.join("corrupt.jpg"), b"not an image").unwrap();
    fs::write(
        request.input_path.join("unsupported-preview.cr3"),
        b"not a supported EXIF container",
    )
    .unwrap();
    jpeg(&request.input_path.join("healthy.jpg"));
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let job = service.repository.get_job(&job.id).unwrap();
    assert_eq!(job.status, "ready");
    assert_eq!(job.asset_count, 3);
    assert!(job.warning_count >= 1);
    let assets = service.repository.assets(&job.id, 0, 60).unwrap().items;
    let healthy = assets
        .iter()
        .find(|asset| asset.filename == "healthy.jpg")
        .unwrap();
    assert_eq!(healthy.metadata.width, Some(48));
    assert_eq!(healthy.preview_status, "ready");
    assert!(healthy.metadata.camera_model.is_none());
    let corrupt = assets
        .iter()
        .find(|asset| asset.filename == "corrupt.jpg")
        .unwrap();
    assert_eq!(corrupt.preview_status, "failed");
    assert!(corrupt.metadata.width.is_none());
}

struct PanickingMetadata;
impl MetadataExtractor for PanickingMetadata {
    fn extract(&self, _file: &DiscoveredFile) -> MetadataResult {
        panic!("simulate a faulty decoder")
    }
}

#[test]
fn decoder_panics_are_isolated_as_asset_placeholders() {
    let (root, _service, request) = setup();
    jpeg(&request.input_path.join("one.jpg"));
    jpeg(&request.input_path.join("two.jpg"));
    let service = JobService::with_metadata(
        root.path().join("panic-data"),
        root.path().join("panic-cache"),
        Box::new(PanickingMetadata),
    )
    .unwrap();
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    assert_eq!(service.repository.get_job(&job.id).unwrap().asset_count, 2);
    assert_eq!(service.repository.get_job(&job.id).unwrap().status, "ready");
}

#[test]
fn cache_is_reused_and_rebuilt_when_deleted() {
    let (_root, service, request) = setup();
    jpeg(&request.input_path.join("one.jpg"));
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let first = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    let cache = first.thumbnail_path.unwrap();
    let modified = fs::metadata(&cache).unwrap().modified().unwrap();
    service.assets(&job.id, 0, 60).unwrap();
    assert_eq!(fs::metadata(&cache).unwrap().modified().unwrap(), modified);
    fs::remove_file(&cache).unwrap();
    let repaired = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    assert_eq!(repaired.preview_status, "ready");
    assert!(cache.exists());
    assert!(service
        .thumbnail_data(&job.id, &first.id)
        .unwrap()
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
}

#[test]
fn changed_original_invalidates_cache_fingerprint() {
    let root = tempdir().unwrap();
    let path = root.path().join("photo.jpg");
    jpeg(&path);
    let one = inspect_file(&path).unwrap();
    fs::write(&path, b"changed and now corrupt").unwrap();
    let two = inspect_file(&path).unwrap();
    assert_eq!(one.id, two.id);
    assert_ne!(one.fingerprint, two.fingerprint);
    let thumbs = ThumbnailService::new(root.path().join("cache")).unwrap();
    assert_ne!(thumbs.cache_path(&one), thumbs.cache_path(&two));
}

#[test]
fn interrupted_jobs_can_resume_and_overlapping_scans_are_rejected() {
    let (_root, service, request) = setup();
    let (job, permit) = service.create(request.clone()).unwrap();
    assert!(service.create(request).is_err());
    drop(permit); // Simulate process exit before the worker starts.
    service.repository.recover_interrupted().unwrap();
    assert_eq!(
        service.repository.get_job(&job.id).unwrap().status,
        "interrupted"
    );
    let (_, permit) = service.resume(&job.id).unwrap();
    service.scan(&job.id, permit).unwrap();
    assert_eq!(service.repository.get_job(&job.id).unwrap().status, "ready");
}

#[test]
fn rejects_overlapping_folders_and_releases_permit_after_validation_error() {
    let (_root, service, request) = setup();
    let mut bad = request.clone();
    bad.output_path = bad.input_path.clone();
    assert!(service.create(bad).is_err());
    let mut nested = request.clone();
    nested.output_path = nested.input_path.join("exports");
    fs::create_dir(&nested.output_path).unwrap();
    let (_, permit) = service.create(nested).unwrap();
    drop(permit);
    assert!(service.create(request).is_ok());
}

#[test]
fn metadata_reader_is_bounded_for_large_raw_files() {
    let root = tempdir().unwrap();
    let path = root.path().join("large.nef");
    let file = fs::File::create(&path).unwrap();
    file.set_len(METADATA_BUDGET * 8).unwrap();
    let result = BasicMetadataExtractor.extract(&inspect_file(&path).unwrap());
    assert!(result.metadata.camera_model.is_none());
    assert!(result.warning.is_some());
}

#[test]
fn reads_a_standard_tiff_embedded_jpeg_without_raw_development() {
    let root = tempdir().unwrap();
    let preview_path = root.path().join("preview.jpg");
    jpeg(&preview_path);
    let preview = fs::read(preview_path).unwrap();
    // Minimal little-endian TIFF: two LONG tags pointing to an embedded JPEG.
    let offset = 8 + 2 + 2 * 12 + 4;
    let mut tiff = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
    tiff.extend_from_slice(&2u16.to_le_bytes());
    for (tag, value) in [
        (0x0201u16, offset as u32),
        (0x0202u16, preview.len() as u32),
    ] {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&value.to_le_bytes());
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&preview);
    let raw = root.path().join("synthetic.dng");
    fs::write(&raw, tiff).unwrap();
    assert_eq!(
        ExifEmbeddedPreview.jpeg_preview(&raw).unwrap(),
        Some(preview)
    );
    let thumbnails = ThumbnailService::new(root.path().join("cache")).unwrap();
    assert_eq!(
        thumbnails
            .generate(&inspect_file(&raw).unwrap(), None)
            .status,
        "ready"
    );
}

#[test]
fn migration_is_repeatable_and_rejects_newer_database_versions() {
    let root = tempdir().unwrap();
    let path = root.path().join("jobs.sqlite3");
    JobRepository::open(path.clone()).unwrap();
    JobRepository::open(path.clone()).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let version: u32 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    db.pragma_update(None, "user_version", 999).unwrap();
    assert!(JobRepository::open(path).is_err());
}

#[test]
fn pages_are_bounded_and_processing_state_is_persistent() {
    let (root, service, request) = setup();
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    // Seed a large catalog cheaply; no 3,001 full image decodes in a pagination test.
    let db = rusqlite::Connection::open(root.path().join("data").join("jobs.sqlite3")).unwrap();
    db.execute_batch(&format!("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x < 3001) INSERT INTO assets (id,job_id,original_path,filename,file_type,file_size,fingerprint,metadata_json,preview_status,created_at) SELECT printf('%064d',x),'{}',printf('photo-%04d.nef',x),printf('photo-%04d.nef',x),'nef',1024,'fingerprint','{{}}','unavailable','now' FROM n;", job.id)).unwrap();
    // ImageMetadata needs absent fields to deserialize as null; Option fields do so by default.
    let first = service.repository.assets(&job.id, 0, 60).unwrap();
    assert_eq!(first.total, 3001);
    assert_eq!(first.items.len(), 60);
    let last = service.repository.assets(&job.id, 3000, 60).unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(
        service
            .repository
            .assets(&job.id, 0, 10_000)
            .unwrap()
            .items
            .len(),
        100
    );
    service.repository.save_assets(&first.items).unwrap();
    let stage: String = db
        .query_row(
            "SELECT stage FROM processing_state WHERE job_id=?1 LIMIT 1",
            [&job.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stage, "discovered");
}

#[test]
fn corrupt_cache_is_replaced_without_touching_originals_or_output() {
    let (_root, service, request) = setup();
    let original = request.input_path.join("photo.jpg");
    let output = request.output_path.clone();
    jpeg(&original);
    let original_bytes = fs::read(&original).unwrap();
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let asset = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    let cache = asset.thumbnail_path.unwrap();
    fs::write(&cache, b"broken cache").unwrap();
    let repaired = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    assert_eq!(repaired.preview_status, "ready");
    assert!(image::image_dimensions(cache).is_ok());
    assert_eq!(fs::read(original).unwrap(), original_bytes);
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}

#[test]
fn missing_original_after_cache_loss_is_a_safe_placeholder() {
    let (_root, service, request) = setup();
    let original = request.input_path.join("photo.jpg");
    jpeg(&original);
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let asset = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    fs::remove_file(asset.thumbnail_path.unwrap()).unwrap();
    fs::remove_file(original).unwrap();
    let repaired = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    assert_eq!(repaired.preview_status, "failed");
    assert!(repaired.thumbnail_path.is_none());
    assert_eq!(service.repository.get_job(&job.id).unwrap().asset_count, 1);
}

#[test]
fn future_checkpoints_are_preserved_until_the_original_changes() {
    let (root, service, request) = setup();
    let original = request.input_path.join("photo.jpg");
    jpeg(&original);
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let db = rusqlite::Connection::open(root.path().join("data").join("jobs.sqlite3")).unwrap();
    db.execute(
        "UPDATE processing_state SET stage='rendered', recipe_json='{}' WHERE job_id=?1",
        [&job.id],
    )
    .unwrap();
    let (_, permit) = service.resume(&job.id).unwrap();
    service.scan(&job.id, permit).unwrap();
    let stage: String = db
        .query_row(
            "SELECT stage FROM processing_state WHERE job_id=?1",
            [&job.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stage, "rendered");
    fs::write(original, b"different file content").unwrap();
    let (_, permit) = service.resume(&job.id).unwrap();
    service.scan(&job.id, permit).unwrap();
    let (stage, recipe): (String, Option<String>) = db
        .query_row(
            "SELECT stage, recipe_json FROM processing_state WHERE job_id=?1",
            [&job.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stage, "discovered");
    assert!(recipe.is_none());
}

#[test]
fn thumbnail_ipc_rejects_a_tampered_identifier() {
    let (root, service, request) = setup();
    jpeg(&request.input_path.join("photo.jpg"));
    let (job, permit) = service.create(request).unwrap();
    service.scan(&job.id, permit).unwrap();
    let asset = service.assets(&job.id, 0, 60).unwrap().items.remove(0);
    let db = rusqlite::Connection::open(root.path().join("data").join("jobs.sqlite3")).unwrap();
    db.execute(
        "UPDATE assets SET fingerprint='not-a-digest' WHERE job_id=?1",
        [&job.id],
    )
    .unwrap();
    assert!(service.thumbnail_data(&job.id, &asset.id).is_err());
}

#[test]
fn unavailable_root_marks_a_job_failed_and_keeps_recovery_possible() {
    let (_root, service, request) = setup();
    let input = request.input_path.clone();
    let (job, permit) = service.create(request).unwrap();
    fs::remove_dir(input).unwrap();
    assert!(service.scan(&job.id, permit).is_err());
    let stored = service.repository.get_job(&job.id).unwrap();
    assert_eq!(stored.status, "failed");
    assert!(stored.last_error.is_some());
}

#[test]
fn resource_snapshot_has_cpu_and_ram_without_faking_gpu_values() {
    use photo_contracts::ResourceProvider;
    let resources = photo_core::resources::LocalResources.snapshot();
    assert!(resources.logical_cpu_count >= 1);
    assert!(resources.total_ram_bytes > 0);
    assert!(resources.available_ram_bytes <= resources.total_ram_bytes);
    for gpu in &resources.gpus {
        if gpu.memory_model == "unified" || gpu.memory_model == "shared" {
            assert!(gpu.dedicated_vram_bytes.is_none());
        }
    }
    assert!(!resources.os.is_empty());
    assert!(resources.available_disk_bytes.is_none());
}
