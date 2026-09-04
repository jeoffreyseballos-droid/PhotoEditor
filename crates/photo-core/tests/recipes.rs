use photo_contracts::*;
use photo_core::{
    development::{DevelopmentService, RecipeRenderRequest},
    jobs::JobService,
    models::NewJob,
    recipes::{RevisionReason, MAX_REVISIONS},
    rendering::{
        self,
        decode::{Decoded, RawDecoder},
        masks::{MaskCache, SegmentationProvider, SoftMask},
        optics::LensProfileResolver,
        pixels::FloatImage,
        CpuProcessingEngine, RenderLimits,
    },
    repository::JobRepository,
};
use rusqlite::params;
use std::{path::Path, sync::Arc};
struct NoRaw;
impl RawDecoder for NoRaw {
    fn id(&self) -> &str {
        "recipe-test"
    }
    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        panic!("Raster fixture must not decode RAW")
    }
}
struct HalfMask(&'static str);
impl SegmentationProvider for HalfMask {
    fn version(&self) -> &str {
        self.0
    }
    fn infer(&self, _: &FloatImage, _: &CancellationToken) -> ProcessingResult<SoftMask> {
        Ok(SoftMask {
            width: 4,
            height: 1,
            values: vec![1., 1., 0., 0.],
        })
    }
}
fn engine() -> CpuProcessingEngine {
    CpuProcessingEngine::new(Box::new(NoRaw), RenderLimits::default())
}
fn layer(id: &str, kind: MaskType) -> RecipeLayer {
    RecipeLayer {
        id: id.into(),
        mask_type: kind,
        enabled: true,
        strength: 1.,
        invert: false,
        confidence: None,
        mask_reference: None,
        adjustments: LocalAdjustments {
            exposure_ev: 1.,
            ..Default::default()
        },
    }
}
fn setup() -> (tempfile::TempDir, JobService, String, Vec<String>) {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for name in ["a.png", "b.png"] {
        image::RgbImage::from_pixel(64, 32, image::Rgb([100u8, 65, 40]))
            .save(input.join(name))
            .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("thumbnails")).unwrap();
    let (job, permit) = jobs
        .create(NewJob {
            name: "Recipes".into(),
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
        .map(|a| a.id)
        .collect();
    (root, jobs, job.id, ids)
}
#[test]
fn drafts_snapshots_restore_and_restart_are_transactional_and_independent() {
    let (root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    let asset = &ids[0];
    let first = repo.get_recipe(&job, asset).unwrap();
    assert_eq!(first.current_revision, 1);
    assert!(!first.modified);
    let initial_id = repo.recipe_history(&job, asset, 0, 10).unwrap()[0]
        .revision_id
        .clone();
    let mut recipe = first.recipe.clone();
    recipe.global.basic.exposure_ev = 0.7;
    let draft = repo
        .save_recipe(&job, asset, &recipe, first.generation, None)
        .unwrap();
    assert!(draft.modified);
    assert_eq!(repo.recipe_history(&job, asset, 0, 10).unwrap().len(), 1);
    assert!(repo
        .save_recipe(&job, asset, &recipe, first.generation, None)
        .is_err()); // stale writer
    assert_eq!(
        repo.get_recipe(&job, &ids[1])
            .unwrap()
            .recipe
            .global
            .basic
            .exposure_ev,
        0.
    );
    let committed = repo
        .create_revision(&job, asset, draft.generation, RevisionReason::Snapshot)
        .unwrap();
    assert_eq!(committed.current_revision, 2);
    assert!(!committed.modified);
    let restored = repo
        .restore_revision(&job, asset, &initial_id, committed.generation)
        .unwrap();
    assert_eq!(restored.recipe_hash, first.recipe_hash);
    assert_eq!(restored.current_revision, 3);
    let history = repo.recipe_history(&job, asset, 0, 10).unwrap();
    assert_eq!(
        history
            .iter()
            .map(|r| r.revision_number)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    let reopened = JobRepository::open(root.path().join("data/jobs.sqlite3")).unwrap();
    assert_eq!(
        reopened.get_recipe(&job, asset).unwrap().recipe_hash,
        restored.recipe_hash
    );
    assert_eq!(
        reopened.recipe_history(&job, asset, 0, 10).unwrap().len(),
        3
    );
}
#[test]
fn reset_preserves_unsnapshotted_edits_and_duplicate_commits_do_not_explode_history() {
    let (_root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    let mut current = repo.get_recipe(&job, &ids[0]).unwrap();
    for i in 0..30 {
        current.recipe.global.basic.exposure_ev = i as f32 / 10.;
        current = repo
            .save_recipe(&job, &ids[0], &current.recipe, current.generation, None)
            .unwrap();
    }
    assert_eq!(repo.recipe_history(&job, &ids[0], 0, 100).unwrap().len(), 1);
    current.recipe.global = Default::default();
    current = repo
        .save_recipe(
            &job,
            &ids[0],
            &current.recipe,
            current.generation,
            Some(RevisionReason::Reset),
        )
        .unwrap();
    let history = repo.recipe_history(&job, &ids[0], 0, 100).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        repo.revision_recipe(&job, &ids[0], &history[1].revision_id)
            .unwrap()
            .global
            .basic
            .exposure_ev,
        2.9
    );
    for _ in 0..3 {
        current = repo
            .create_revision(&job, &ids[0], current.generation, RevisionReason::Snapshot)
            .unwrap();
    }
    assert_eq!(repo.recipe_history(&job, &ids[0], 0, 100).unwrap().len(), 3);
}
#[test]
fn transaction_failure_rolls_back_current_hash_history_and_checkpoint_projection() {
    let (root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    let current = repo.get_recipe(&job, &ids[0]).unwrap();
    let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_recipe_save BEFORE UPDATE ON asset_recipes BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let mut changed = current.recipe.clone();
    changed.global.basic.exposure_ev = 1.;
    assert!(repo
        .save_recipe(
            &job,
            &ids[0],
            &changed,
            current.generation,
            Some(RevisionReason::Snapshot)
        )
        .is_err());
    let after = repo.get_recipe(&job, &ids[0]).unwrap();
    assert_eq!(after.recipe_hash, current.recipe_hash);
    assert_eq!(after.generation, current.generation);
    assert_eq!(repo.recipe_history(&job, &ids[0], 0, 10).unwrap().len(), 1);
    assert_eq!(
        repo.development(&job, &ids[0])
            .unwrap()
            .adjustments
            .exposure_ev,
        0.
    );
}
#[test]
fn legacy_phase_two_and_toolkit_payloads_migrate_losslessly_and_lazily() {
    for toolkit in [false, true] {
        let (root, jobs, job, ids) = setup();
        let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
        let mut a = RenderAdjustments {
            exposure_ev: 1.2,
            sharpening: 14.,
            noise_reduction: 11.,
            ..Default::default()
        };
        if toolkit {
            a.hsl[3].saturation = -17.;
            a.curve.blue.insert(1, CurvePoint { x: 0.5, y: 0.6 });
            a.presence.clarity = 11.;
            a.detail.noise.color = 20.;
            a.detail.sharpening.masking = 50.;
            a.optics.enabled = true;
            a.optics.manual_distortion = 4.;
            a.effects.vignette.amount = -20.;
            a.local_layers.push(LocalAdjustmentLayer {
                id: "subject".into(),
                mask_type: MaskType::Subject,
                enabled: true,
                strength: 0.6,
                invert: true,
                confidence: None,
                mask_reference: Some("a".repeat(64)),
                adjustments: LocalAdjustments {
                    exposure_ev: 0.3,
                    ..Default::default()
                },
            });
        }
        let json = if toolkit {
            serde_json::to_string(&a).unwrap()
        } else {
            r#"{"schema_version":1,"exposure_ev":1.2,"sharpening":14,"noise_reduction":11}"#.into()
        };
        db.execute("INSERT INTO development_state(job_id,asset_id,adjustments_json,revision,state,updated_at) VALUES(?1,?2,?3,7,'exported','2025-01-01T00:00:00Z')",params![job,ids[0],json]).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM asset_recipes", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        jobs.repository.assets(&job, 0, 100).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM asset_recipes", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        let r = jobs.repository.get_recipe(&job, &ids[0]).unwrap();
        assert_eq!(r.recipe.adjustments().unwrap(), a);
        assert_eq!(r.recipe.provenance.origin, RecipeOrigin::Migrated);
        assert_eq!(r.recipe.created_at, "2025-01-01T00:00:00Z");
        assert_eq!(
            jobs.repository.development(&job, &ids[0]).unwrap().revision,
            7
        );
        assert_eq!(
            db.query_row(
                "SELECT adjustments_json FROM development_state WHERE asset_id=?1",
                [&ids[0]],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            json
        );
    }
}
#[test]
fn corrupt_current_and_legacy_recipes_preserve_assets_and_payload_during_explicit_recovery() {
    for legacy in [true, false] {
        let (root, jobs, job, ids) = setup();
        let repo = &jobs.repository;
        let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
        if legacy {
            db.execute("INSERT INTO development_state(job_id,asset_id,adjustments_json,updated_at) VALUES(?1,?2,'{broken','now')",params![job,ids[0]]).unwrap();
        } else {
            repo.get_recipe(&job, &ids[0]).unwrap();
            db.execute(
                "UPDATE asset_recipes SET recipe_json='{broken' WHERE asset_id=?1",
                [&ids[0]],
            )
            .unwrap();
        }
        let state = repo.get_recipe(&job, &ids[0]).unwrap();
        assert_eq!(
            state.error.as_ref().unwrap().code,
            RecipeErrorCode::CorruptStoredRecipe
        );
        assert_eq!(repo.assets(&job, 0, 100).unwrap().total, 2);
        assert_eq!(
            repo.development(&job, &ids[0])
                .unwrap()
                .adjustments
                .exposure_ev,
            0.
        );
        assert!(repo
            .save_recipe(&job, &ids[0], &state.recipe, state.generation, None)
            .is_err());
        let fixed = repo
            .save_recipe(
                &job,
                &ids[0],
                &state.recipe,
                state.generation,
                Some(RevisionReason::Reset),
            )
            .unwrap();
        assert!(fixed.error.is_none());
        let count: u32 = db
            .query_row(
                "SELECT COUNT(*) FROM recipe_recovery WHERE payload='{broken'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 1);
    }
}
#[test]
fn imports_and_exports_are_portable_nonclobber_and_cross_asset_safe() {
    let (root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    let mut a = repo.get_recipe(&job, &ids[0]).unwrap();
    a.recipe.global.basic.exposure_ev = 0.4;
    a.recipe
        .local_layers
        .push(layer("subject", MaskType::Subject));
    a.recipe.local_layers[0].mask_reference = Some(MaskReference {
        content_id: "a".repeat(64),
        source_fingerprint: Some("b".repeat(64)),
        model_id: Some("modnet".into()),
        model_version: Some("source".into()),
        geometry_version: Some(MASK_GEOMETRY_VERSION.into()),
    });
    a = repo
        .save_recipe(
            &job,
            &ids[0],
            &a.recipe,
            a.generation,
            Some(RevisionReason::Snapshot),
        )
        .unwrap();
    let path = repo.export_recipe(&job, &ids[0]).unwrap();
    let second = repo.export_recipe(&job, &ids[0]).unwrap();
    assert_ne!(path, second);
    assert!(path.starts_with(root.path().join("output").canonicalize().unwrap()));
    let b = repo.get_recipe(&job, &ids[1]).unwrap();
    let imported = repo
        .import_recipe_file(&job, &ids[1], &path, b.generation)
        .unwrap();
    assert_eq!(imported.recipe.asset_id, ids[1]);
    assert_eq!(imported.recipe.global.basic.exposure_ev, 0.4);
    assert_eq!(imported.recipe.provenance.origin, RecipeOrigin::Imported);
    assert!(imported.recipe.local_layers[0].mask_reference.is_none());
    let asset = repo.asset(&job, &ids[1]).unwrap();
    let effective = engine()
        .effective_recipe(&imported.recipe, &asset.original_path, &Default::default())
        .unwrap();
    assert_eq!(effective.unresolved_masks, vec!["subject"]);
    assert!(!effective.adjustments.local_layers[0].enabled);
    assert_eq!(
        repo.get_recipe(&job, &ids[0]).unwrap().recipe_hash,
        a.recipe_hash
    );
    assert!(repo
        .import_recipe(&job, &ids[1], "{broken", imported.generation)
        .is_err());
    assert!(repo
        .import_recipe(
            &job,
            &ids[1],
            &" ".repeat(MAX_RECIPE_BYTES + 1),
            imported.generation
        )
        .is_err());
}
#[test]
fn history_retention_preserves_initial_and_recent_evidence() {
    let (_root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    let mut s = repo.get_recipe(&job, &ids[0]).unwrap();
    for i in 0..205 {
        s.recipe.global.basic.exposure_ev = (i % 40) as f32 / 10.;
        s = repo
            .save_recipe(
                &job,
                &ids[0],
                &s.recipe,
                s.generation,
                Some(RevisionReason::Snapshot),
            )
            .unwrap();
    }
    let mut rows = repo.recipe_history(&job, &ids[0], 0, 100).unwrap();
    rows.extend(repo.recipe_history(&job, &ids[0], 100, 100).unwrap());
    assert_eq!(rows.len(), MAX_REVISIONS as usize);
    assert_eq!(rows.last().unwrap().revision_number, 1);
    assert_eq!(rows[0].revision_number, s.current_revision);
    assert!(!rows[0].created_at.is_empty());
}
fn render_request(asset: &str, source: &Path, dest: &Path, preview: bool) -> RenderRequest {
    RenderRequest {
        asset_id: asset.into(),
        original: source.into(),
        adjustments: RenderAdjustments {
            exposure_ev: 4.,
            ..Default::default()
        },
        source_metadata: Default::default(),
        destination: dest.into(),
        output_format: OutputFormat::Jpeg,
        preview,
        jpeg_quality: 95,
    }
}
#[test]
fn renderer_executes_recipe_not_request_slider_projection_and_preview_export_match() {
    let (root, jobs, job, ids) = setup();
    let asset = jobs.repository.asset(&job, &ids[0]).unwrap();
    let r = jobs.repository.get_recipe(&job, &ids[0]).unwrap().recipe;
    let engine = engine();
    let mut paths = Vec::new();
    for i in 0..3 {
        let path = root.path().join(format!("neutral-{i}.jpg"));
        engine
            .render_recipe(
                &r,
                &render_request(&asset.id, &asset.original_path, &path, i == 0),
                &Default::default(),
            )
            .unwrap();
        paths.push(path);
    }
    assert_eq!(
        image::open(&paths[0]).unwrap().to_rgb16(),
        image::open(&paths[1]).unwrap().to_rgb16()
    );
    assert_eq!(
        image::open(&paths[1]).unwrap().to_rgb16(),
        image::open(&paths[2]).unwrap().to_rgb16()
    );
    for (name, recipe) in [
        ("exposure", {
            let mut a = r.clone();
            a.global.basic.exposure_ev = 1.;
            a
        }),
        ("hsl", {
            let mut a = r.clone();
            a.global.color_mixer.orange.saturation = -70.;
            a
        }),
        ("optics", {
            let mut a = r.clone();
            a.global.optics.manual_vignette = 40.;
            a
        }),
    ] {
        let path = root.path().join(format!("{name}.jpg"));
        engine
            .render_recipe(
                &recipe,
                &render_request(&asset.id, &asset.original_path, &path, false),
                &Default::default(),
            )
            .unwrap();
        assert_ne!(
            image::open(&paths[0]).unwrap().to_rgb16(),
            image::open(path).unwrap().to_rgb16(),
            "{name}"
        );
    }
}
#[test]
fn local_recipes_affect_only_their_target_and_mask_replacement_model_or_deletion_rekeys() {
    let (root, jobs, job, ids) = setup();
    let asset = jobs.repository.asset(&job, &ids[0]).unwrap();
    let cache_dir = root.path().join("masks");
    let cache = MaskCache::new(cache_dir.clone(), Box::new(HalfMask("test-v1")));
    let identity = rendering::source_identity(&asset.original_path).unwrap();
    let fixture = FloatImage {
        width: 64,
        height: 32,
        pixels: vec![[0.1; 3]; 64 * 32],
    };
    let diagnostic = cache
        .generate(&identity, "recipe-test", &fixture, &Default::default())
        .unwrap();
    let engine = engine().with_toolkit(LensProfileResolver::unavailable("test"), cache);
    let base = jobs.repository.get_recipe(&job, &ids[0]).unwrap().recipe;
    let mut r = base.clone();
    r.local_layers.push(layer("local", MaskType::Subject));
    let mut outputs = Vec::new();
    for (index, recipe) in [base.clone(), r.clone(), {
        let mut b = r.clone();
        b.local_layers[0].mask_type = MaskType::Background;
        b
    }]
    .into_iter()
    .enumerate()
    {
        let path = root.path().join(format!("local-{index}.jpg"));
        engine
            .render_recipe(
                &recipe,
                &render_request(&asset.id, &asset.original_path, &path, false),
                &Default::default(),
            )
            .unwrap();
        outputs.push(image::open(path).unwrap().to_rgb8());
    }
    assert!(outputs[1].get_pixel(8, 16)[0] > outputs[0].get_pixel(8, 16)[0] + 10);
    assert_eq!(outputs[1].get_pixel(56, 16), outputs[0].get_pixel(56, 16));
    assert_eq!(outputs[2].get_pixel(8, 16), outputs[0].get_pixel(8, 16));
    assert!(outputs[2].get_pixel(56, 16)[0] > outputs[0].get_pixel(56, 16)[0] + 10);
    let before = engine
        .effective_recipe(&r, &asset.original_path, &Default::default())
        .unwrap();
    let mask_path = cache_dir.join(format!("{}.png", diagnostic.reference.unwrap()));
    image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::from_pixel(4, 1, image::Luma([0]))
        .save(&mask_path)
        .unwrap();
    let replaced = engine
        .effective_recipe(&r, &asset.original_path, &Default::default())
        .unwrap();
    assert_ne!(before.dependency_hash, replaced.dependency_hash);
    assert_eq!(before.recipe_hash, replaced.recipe_hash);
    std::fs::remove_file(&mask_path).unwrap();
    let absent = engine
        .effective_recipe(&r, &asset.original_path, &Default::default())
        .unwrap();
    assert_ne!(replaced.dependency_hash, absent.dependency_hash);
    assert_eq!(absent.unresolved_masks, vec!["local"]);
    let model2 = CpuProcessingEngine::new(Box::new(NoRaw), RenderLimits::default()).with_toolkit(
        LensProfileResolver::unavailable("test"),
        MaskCache::new(cache_dir, Box::new(HalfMask("test-v2"))),
    );
    assert_ne!(
        absent.dependency_hash,
        model2
            .effective_recipe(&r, &asset.original_path, &Default::default())
            .unwrap()
            .dependency_hash
    );
    let other = jobs.repository.asset(&job, &ids[1]).unwrap();
    assert!(engine
        .effective_recipe(&r, &other.original_path, &Default::default())
        .unwrap()
        .adjustments
        .local_layers
        .iter()
        .all(|l| !l.enabled));
}
#[test]
fn service_cache_history_metadata_and_export_share_recipe_semantics() {
    let (root, jobs, job, ids) = setup();
    let repo = &jobs.repository;
    let dev = DevelopmentService::new(
        repo.clone(),
        Arc::new(engine()),
        root.path().join("render-cache"),
        None,
    )
    .unwrap();
    let render = |preview: bool, commit: bool| {
        let generation = repo.get_recipe(&job, &ids[0]).unwrap().generation;
        let req = RecipeRenderRequest {
            job_id: job.clone(),
            asset_id: ids[0].clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            expected_generation: generation,
            preview,
            output_format: OutputFormat::Jpeg,
            jpeg_quality: 90,
            commit,
        };
        let permit = dev.reserve(&req.request_id, preview).unwrap();
        dev.render_recipe(req, permit).unwrap()
    };
    let first = render(true, false);
    let second = render(true, false);
    assert_eq!(first.state.preview_path, second.state.preview_path);
    assert!(second
        .state
        .warnings
        .iter()
        .any(|w| w.contains("Cached reduced")));
    assert_eq!(repo.recipe_history(&job, &ids[0], 0, 100).unwrap().len(), 1);
    let mut state = repo.get_recipe(&job, &ids[0]).unwrap();
    state.recipe.metadata.sequence_id = Some("future-sequence".into());
    repo.save_recipe(
        &job,
        &ids[0],
        &state.recipe,
        state.generation,
        Some(RevisionReason::Snapshot),
    )
    .unwrap();
    let metadata = render(true, true);
    assert_eq!(first.state.preview_path, metadata.state.preview_path);
    state = repo.get_recipe(&job, &ids[0]).unwrap();
    state.recipe.global.basic.exposure_ev = 0.7;
    repo.save_recipe(&job, &ids[0], &state.recipe, state.generation, None)
        .unwrap();
    let edited = render(true, true);
    assert_ne!(first.state.preview_path, edited.state.preview_path);
    let exported = render(false, true);
    assert_eq!(
        image::open(edited.state.preview_path.unwrap())
            .unwrap()
            .to_rgb16(),
        image::open(exported.state.export_path.unwrap())
            .unwrap()
            .to_rgb16()
    );
}
#[test]
fn grid_of_3000_assets_does_not_materialize_recipe_histories() {
    let (root, jobs, job, ids) = setup();
    let db = rusqlite::Connection::open(root.path().join("data/jobs.sqlite3")).unwrap();
    db.execute("WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<2998) INSERT INTO assets(id,job_id,original_path,filename,file_type,file_size,fingerprint,metadata_json,preview_status,created_at) SELECT 'extra-'||n,?1,'extra-'||n||'.png','extra-'||n||'.png','png',100,'fp','{}','unavailable','now' FROM seq",[&job]).unwrap();
    jobs.repository.get_recipe(&job, &ids[0]).unwrap();
    let start = std::time::Instant::now();
    assert_eq!(jobs.repository.assets(&job, 0, 100).unwrap().total, 3000);
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM asset_recipes", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM recipe_revisions", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    println!("3,000 asset paginated grid query: {:?}", start.elapsed());
}

#[test]
fn lens_content_and_objective_metadata_are_effective_dependencies_not_recipe_metadata() {
    let (root, jobs, job, ids) = setup();
    let asset = jobs.repository.asset(&job, &ids[0]).unwrap();
    let mut recipe = jobs.repository.get_recipe(&job, &ids[0]).unwrap().recipe;
    recipe.global.optics.enabled = true;
    let mut identities = Vec::new();
    for (i, k) in [0.01, 0.02].into_iter().enumerate() {
        let folder = root.path().join(format!("lenses-{i}"));
        std::fs::create_dir(&folder).unwrap();
        let xml = format!(
            r#"<lensdatabase version="2"><camera><maker>Test</maker><model>Camera</model><cropfactor>1</cropfactor></camera><lens><maker>Test</maker><model>Lens</model><cropfactor>1</cropfactor><calibration><distortion model="poly3" focal="50" k1="{k}"/></calibration></lens></lensdatabase>"#
        );
        std::fs::write(folder.join("lens.xml"), xml).unwrap();
        let renderer = engine().with_toolkit(
            LensProfileResolver::load(&folder),
            MaskCache::new(root.path().join("masks"), Box::new(HalfMask("test"))),
        );
        let metadata = OpticsMetadata {
            camera_make: Some("Test".into()),
            camera_model: Some("Camera".into()),
            lens_model: Some("Lens".into()),
            focal_length: Some(50.),
            ..Default::default()
        };
        let effective = renderer
            .effective_recipe(&recipe, &asset.original_path, &metadata)
            .unwrap();
        identities.push(effective.dependency_hash.clone());
        let mut changed = metadata.clone();
        changed.focal_length = Some(51.);
        assert_ne!(
            effective.dependency_hash,
            renderer
                .effective_recipe(&recipe, &asset.original_path, &changed)
                .unwrap()
                .dependency_hash
        );
        let mut subject = recipe.clone();
        subject.local_layers.push(layer("s", MaskType::Subject));
        let before = renderer
            .effective_recipe(&subject, &asset.original_path, &metadata)
            .unwrap();
        subject.local_layers[0].adjustments.exposure_ev = 0.5;
        let after = renderer
            .effective_recipe(&subject, &asset.original_path, &metadata)
            .unwrap();
        assert_ne!(before.recipe_hash, after.recipe_hash);
        assert_ne!(
            rendering::recipe::recipe_preview_key(
                "source",
                &before.recipe_hash,
                &before.dependency_hash,
                "decoder"
            ),
            rendering::recipe::recipe_preview_key(
                "source",
                &after.recipe_hash,
                &after.dependency_hash,
                "decoder"
            )
        );
    }
    assert_ne!(identities[0], identities[1]);
}
#[test]
fn borrowed_mask_reference_is_rejected_even_when_target_has_its_own_ready_mask() {
    let (root, jobs, job, ids) = setup();
    let first = jobs.repository.asset(&job, &ids[0]).unwrap();
    let second = jobs.repository.asset(&job, &ids[1]).unwrap();
    let cache = MaskCache::new(root.path().join("masks"), Box::new(HalfMask("test-v1")));
    let fixture = FloatImage {
        width: 4,
        height: 1,
        pixels: vec![[0.1; 3]; 4],
    };
    let foreign = cache
        .generate(
            &rendering::source_identity(&first.original_path).unwrap(),
            "recipe-test",
            &fixture,
            &Default::default(),
        )
        .unwrap();
    let own = cache
        .generate(
            &rendering::source_identity(&second.original_path).unwrap(),
            "recipe-test",
            &fixture,
            &Default::default(),
        )
        .unwrap();
    let renderer = engine().with_toolkit(LensProfileResolver::unavailable("test"), cache);
    let mut recipe = jobs.repository.get_recipe(&job, &ids[1]).unwrap().recipe;
    recipe.local_layers.push(layer("s", MaskType::Subject));
    recipe.local_layers[0].mask_reference = Some(MaskReference {
        content_id: foreign.reference.unwrap(),
        source_fingerprint: None,
        model_id: None,
        model_version: None,
        geometry_version: None,
    });
    let stale = renderer
        .effective_recipe(&recipe, &second.original_path, &Default::default())
        .unwrap();
    assert_eq!(stale.unresolved_masks, vec!["s"]);
    assert_eq!(stale.mask.status, MaskStatus::Ready);
    assert!(!stale.adjustments.local_layers[0].enabled);
    recipe.local_layers[0].mask_reference = None;
    let rebound = renderer
        .effective_recipe(&recipe, &second.original_path, &Default::default())
        .unwrap();
    assert!(rebound.unresolved_masks.is_empty());
    assert!(rebound.adjustments.local_layers[0].enabled);
    assert_eq!(
        rebound.adjustments.local_layers[0].mask_reference,
        own.reference
    );
}
