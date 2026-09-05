use photo_contracts::{analysis::*, culling::*, *};
use photo_core::{
    analysis::AnalysisService,
    culling::{features::*, score::score, *},
    jobs::JobService,
    models::NewJob,
    rendering::{
        decode::{Decoded, RawDecoder},
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
    time::{Duration, Instant},
};
fn f() -> CullingFeatures {
    CullingAssessment::parse(include_str!("../../../src/test/culling-fixture.json"))
        .unwrap()
        .features
        .unwrap()
}
fn faces(f: &mut CullingFeatures) -> &mut Vec<FaceFeatures> {
    match &mut f.people.faces {
        Signal::Available { value, .. } => value,
        _ => panic!("fixture faces"),
    }
}
fn scored(f: &CullingFeatures) -> score::Scored {
    score(f, &SimilarityContext::default()).unwrap()
}
#[test]
fn sharp_open_individual_and_group_vs_blink() {
    let mut a = f();
    assert_eq!(scored(&a).rating.get(), 5);
    faces(&mut a).truncate(1);
    assert_eq!(scored(&a).rating.get(), 5);
    faces(&mut a)[0].eyes = Signal::available(EyeState::Closed, 0.98);
    assert!(scored(&a).rating.get() <= 3);
    let mut group = f();
    faces(&mut group)[4].eyes = Signal::available(EyeState::Closed, 0.98);
    let s = scored(&group);
    assert!(s.score < scored(&f()).score);
    assert!(s
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::EyesClosed && r.subject_index == Some(4)));
    assert!(s
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::GroupIntegrity));
}
#[test]
fn group_soft_face_and_blink_are_not_hidden_by_global_sharpness() {
    let mut a = f();
    faces(&mut a)[3].sharpness = Signal::available(0.01, 0.8);
    let s = scored(&a);
    assert!(s.rating.get() < 5);
    assert!(s
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::FaceSoft && r.subject_index == Some(3)));
    faces(&mut a)[4].eyes = Signal::available(EyeState::Closed, 0.98);
    assert!(scored(&a).rating.get() <= 2);
}
#[test]
fn uncertain_closed_eyes_cannot_become_blink_and_unknown_not_five() {
    let mut a = f();
    faces(&mut a)[0].eyes = Signal::available(EyeState::Closed, 0.54);
    let s = scored(&a);
    assert!(!s.reasons.iter().any(|r| r.code == ReasonCode::EyesClosed));
    assert!(s.rating.get() >= 3);
    assert!(s.rating.get() < 5);
    faces(&mut a)[0].eyes = Signal::available(EyeState::Uncertain, 0.99);
    assert!(!scored(&a)
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::EyesClosed));
}
#[test]
fn missing_and_empty_faces_distinct_nonfatal() {
    let mut a = f();
    a.people.faces = Signal::unavailable("model absent");
    let s = scored(&a);
    assert!(s
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::FaceDetectorUnavailable));
    assert!(s.rating.get() >= 3);
    a.people.faces = Signal::available(vec![], 0.9);
    assert!(scored(&a)
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::NoFacesDetected));
}
#[test]
fn framing_and_clipping_conservative_and_penalties_bounded_for_groups() {
    let mut a = f();
    faces(&mut a)[0].edge_distance = 0.;
    assert!(scored(&a)
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::FaceNearEdge));
    let baseline = scored(&a).score;
    for face in faces(&mut a) {
        face.visible_fraction = 0.6;
        face.highlight_clip_fraction = 0.8;
    }
    assert!(scored(&a).score >= baseline - 16.);
    assert!(scored(&a)
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::FacePartlyClipped));
}
#[test]
fn photo_type_weights_and_intentional_dark_blur() {
    let mut a = f();
    faces(&mut a)[0].eyes = Signal::available(EyeState::Closed, 0.99);
    let portrait = scored(&a).score;
    for kind in [PhotoType::Landscape, PhotoType::RealEstate] {
        a.photo_type = kind;
        let s = scored(&a);
        assert!(s.score > portrait);
        assert!(!s.reasons.iter().any(|r| r.code == ReasonCode::EyesClosed));
        a.technical.global_sharpness = 0.001;
        a.exposure.median_luminance = 0.01;
        assert!(scored(&a).rating.get() >= 3);
    }
    a.exposure.shadow_clip_fraction = 1.;
    a.exposure.tonal_range = 0.;
    assert_eq!(scored(&a).rating.get(), 2);
}
fn image(soft: bool) -> FloatImage {
    FloatImage {
        width: 128,
        height: 128,
        pixels: (0..128 * 128)
            .map(|i| {
                let x = i % 128;
                if soft {
                    [0.1 + 0.7 * (x as f32 / 128.); 3]
                } else {
                    [if x % 16 < 8 { 0.1 } else { 0.8 }; 3]
                }
            })
            .collect(),
    }
}
#[test]
fn local_detail_direction_exposure_and_clipped_geometry() {
    let b = BoundingBox {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };
    let sharp = local_metrics(&image(false), &b);
    let blur = local_metrics(&image(true), &b);
    assert!(sharp.0 > blur.0 * 2.);
    assert!(sharp.4 > 0.9);
    let mut bright = image(false);
    bright.pixels.fill([1.; 3]);
    let m = local_metrics(&bright, &b);
    assert_eq!(m.1, 1.);
    assert_eq!(m.2, 1.);
    bright.pixels.fill([0.; 3]);
    assert_eq!(local_metrics(&bright, &b).3, 1.);
    let d = Detection {
        x: -0.1,
        y: 0.2,
        width: 0.2,
        height: 0.3,
        confidence: 0.99,
    };
    let (b, v, e) = normalized_box(&d).unwrap();
    assert_eq!(b.x, 0.);
    assert!((v - 0.5).abs() < 1e-9);
    assert_eq!(e, 0.);
}
#[test]
fn singleton_low_face_detail_does_not_receive_excellent_rating() {
    let mut a = f();
    faces(&mut a).truncate(1);
    faces(&mut a)[0].sharpness = Signal::available(0.001, 0.8);
    assert!(scored(&a).rating.get() <= 3);
}
#[test]
fn reliable_severe_face_softness_is_one_star_even_when_group_preferred() {
    let mut features = f();
    features.asset_id = "soft-frame".into();
    faces(&mut features).truncate(1);
    faces(&mut features)[0].sharpness = Signal::available(0.154, 0.8);
    faces(&mut features)[0].eyes = Signal::unavailable("eye model unavailable");
    let similarity = SimilarityContext {
        group_id: Some("1".repeat(64)),
        group_size: 2,
        preferred: true,
        preferred_assets: vec![features.asset_id.clone()],
        relative_score: Some(0.),
        confidence: 0.8,
        bracket_like: false,
        kind: DuplicateKind::NearDuplicate,
        similarity_score: Some(0.95),
        exact: None,
    };
    let result = score(&features, &similarity).unwrap();
    assert_eq!(result.rating.get(), 1);
    assert_eq!(result.score, 19.);
    assert!(result
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::SevereSubjectSoftness));
    assert!(!result
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::FaceSharp));
}
#[test]
fn reliably_focused_group_leader_can_reach_five_with_unknown_eyes() {
    let mut features = f();
    features.asset_id = "focused-frame".into();
    faces(&mut features).truncate(1);
    faces(&mut features)[0].sharpness = Signal::available(0.20, 0.8);
    faces(&mut features)[0].eyes = Signal::unavailable("eye model unavailable");
    let ungrouped = scored(&features);
    assert_eq!(ungrouped.rating.get(), 4);
    let similarity = SimilarityContext {
        group_id: Some("2".repeat(64)),
        group_size: 2,
        preferred: true,
        preferred_assets: vec![features.asset_id.clone()],
        relative_score: Some(0.),
        confidence: 0.8,
        bracket_like: false,
        kind: DuplicateKind::NearDuplicate,
        similarity_score: Some(0.95),
        exact: None,
    };
    let result = score(&features, &similarity).unwrap();
    assert_eq!(result.rating.get(), 5);
    assert!(result
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::EyesUncertain));
    assert!(!result
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::EyesOpen));
}
#[test]
fn realistic_portrait_features_cover_all_five_star_bands() {
    let portrait = |detail: f64, eyes: EyeState| {
        let mut features = f();
        faces(&mut features).truncate(1);
        faces(&mut features)[0].sharpness = Signal::available(detail, 0.8);
        faces(&mut features)[0].eyes = Signal::available(eyes, 0.98);
        features
    };

    let mut excellent = portrait(0.5, EyeState::Uncertain);
    excellent.technical.global_sharpness = 0.001;
    excellent.technical.subject_sharpness = Signal::available(0.001, 0.65);
    assert_eq!(scored(&excellent).rating.get(), 5);

    let good = portrait(0.3, EyeState::Uncertain);
    assert_eq!(scored(&good).rating.get(), 4);

    let usable = portrait(0.2, EyeState::Closed);
    assert_eq!(scored(&usable).rating.get(), 3);

    let mut significant = usable.clone();
    faces(&mut significant)[0].visible_fraction = 0.6;
    faces(&mut significant)[0].highlight_clip_fraction = 0.8;
    assert_eq!(scored(&significant).rating.get(), 2);

    let severe = portrait(0.19, EyeState::Uncertain);
    assert_eq!(scored(&severe).rating.get(), 1);
}
#[test]
fn photographer_issue_flags_require_confident_engine_reasons() {
    let mut severe = f();
    faces(&mut severe).truncate(1);
    faces(&mut severe)[0].sharpness = Signal::available(0.154, 0.8);
    faces(&mut severe)[0].eyes = Signal::unavailable("eye model unavailable");
    let severe = assess(severe, SimilarityContext::default()).unwrap();
    assert_eq!(culling_issues(&severe), vec![CullingIssue::Blurry]);

    let mut blink = f();
    faces(&mut blink).truncate(1);
    faces(&mut blink)[0].sharpness = Signal::available(0.3, 0.8);
    faces(&mut blink)[0].eyes = Signal::available(EyeState::Closed, 0.98);
    let blink = assess(blink, SimilarityContext::default()).unwrap();
    assert_eq!(culling_issues(&blink), vec![CullingIssue::ClosedEyes]);

    let mut uncertain = f();
    faces(&mut uncertain).truncate(1);
    faces(&mut uncertain)[0].sharpness = Signal::available(0.3, 0.8);
    faces(&mut uncertain)[0].eyes = Signal::available(EyeState::Closed, 0.54);
    let uncertain = assess(uncertain, SimilarityContext::default()).unwrap();
    assert!(culling_issues(&uncertain).is_empty());
}
#[test]
fn every_severely_blurry_group_member_stays_one_star_with_a_relative_winner() {
    let mut group = Vec::new();
    for (index, detail) in [0.195, 0.18, 0.17, 0.16, 0.15].into_iter().enumerate() {
        let mut features = f();
        features.asset_id = format!("soft-{index}");
        faces(&mut features).truncate(1);
        faces(&mut features)[0].sharpness = Signal::available(detail, 0.8);
        faces(&mut features)[0].eyes = Signal::unavailable("eye model unavailable");
        if index > 0 {
            faces(&mut features)[0].visible_fraction = 0.6;
        }
        group.push(features);
    }
    let contexts = similarity::group(&group, &CancellationToken::default()).unwrap();
    assert_eq!(
        contexts.iter().filter(|context| context.preferred).count(),
        1
    );
    assert!(contexts[0].preferred);
    for (features, context) in group.iter().zip(&contexts) {
        let result = score(features, context).unwrap();
        assert_eq!(result.rating.get(), 1);
        assert!(result
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::SevereSubjectSoftness));
    }
}
#[test]
fn similarity_requires_visual_and_time_and_camera_context() {
    let a = f().descriptor;
    assert!(similarity::similarity(&a, &a).is_some());
    let mut b = a.clone();
    b.capture_timestamp = Some("2026:09:04 12:00:01.2".into());
    b.luminance_grid[0] += 0.01;
    assert!(similarity::similarity(&a, &b).is_some());
    b.difference_hash = "0000000000000000".into();
    assert!(similarity::similarity(&a, &b).is_none());
    b = a.clone();
    b.capture_timestamp = Some("2026:09:04 12:05:00".into());
    assert!(similarity::similarity(&a, &b).is_none());
    b = a.clone();
    b.camera = Some("Other camera".into());
    assert!(similarity::similarity(&a, &b).is_none());
    b = a.clone();
    b.capture_timestamp = None;
    assert!(similarity::similarity(&a, &b).is_some());
}
#[test]
fn relative_best_frame_and_ties_never_force_alternatives_to_one() {
    let mut a = f();
    a.asset_id = "A".into();
    let mut b = a.clone();
    b.asset_id = "B".into();
    faces(&mut b)[4].eyes = Signal::available(EyeState::Closed, 0.98);
    let mut c = a.clone();
    c.asset_id = "C".into();
    faces(&mut c)[3].visible_fraction = 0.6;
    let list = vec![a, b, c];
    let groups = similarity::group(&list, &CancellationToken::default()).unwrap();
    assert_eq!(groups[0].preferred_assets, vec!["A"]);
    let scores: Vec<_> = list
        .iter()
        .zip(&groups)
        .map(|(f, g)| score(f, g).unwrap())
        .collect();
    assert!(scores[0].score > scores[1].score && scores[0].score > scores[2].score);
    assert!(scores[1..].iter().all(|s| s.rating.get() > 1));
    let ties = similarity::group(
        &[list[0].clone(), {
            let mut t = list[0].clone();
            t.asset_id = "tie".into();
            t
        }],
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(ties[0].preferred_assets.len(), 2);
}
struct NoRaw;
impl RawDecoder for NoRaw {
    fn id(&self) -> &str {
        "culling-test"
    }
    fn decode(
        &self,
        _: &Path,
        _: bool,
        _: RenderLimits,
        _: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        panic!("raster only")
    }
}
struct MockFaces {
    calls: Arc<AtomicUsize>,
    block: bool,
}
impl FaceDetector for MockFaces {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider: "FaceDetector".into(),
            model: "structured-fixture".into(),
            version: "v1".into(),
        }
    }
    fn detect(
        &self,
        _: &FloatImage,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Signal<Vec<Detection>>> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if self.block && call == 1 {
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(10));
            }
            cancel.check()?;
        }
        Ok(Signal::available(
            vec![Detection {
                x: 0.15,
                y: 0.15,
                width: 0.6,
                height: 0.6,
                confidence: 0.99,
            }],
            0.99,
        ))
    }
}
struct Setup {
    _root: tempfile::TempDir,
    jobs: JobService,
    service: Arc<CullingService>,
    analysis: Arc<AnalysisService>,
    engine: Arc<CpuProcessingEngine>,
    job: String,
    ids: Vec<String>,
    calls: Arc<AtomicUsize>,
}
fn setup(block: bool) -> Setup {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::create_dir(&output).unwrap();
    for n in 0..3 {
        image::RgbImage::from_fn(256, 256, |x, y| {
            // Near-identical, not byte copies: preserve visual-ranking test semantics.
            image::Rgb(
                [((x / 16 + y / 16) % 2 * 35 + 30 + x / 2 + if x == 0 && y == 0 { n } else { 0 })
                    as u8; 3],
            )
        })
        .save(input.join(format!("photo-{n}.png")))
        .unwrap();
    }
    let jobs = JobService::new(root.path().join("data"), root.path().join("thumbs")).unwrap();
    let (j, p) = jobs
        .create(NewJob {
            name: "Culling".into(),
            input_path: input,
            output_path: output,
        })
        .unwrap();
    jobs.scan(&j.id, p).unwrap();
    let ids = jobs
        .repository
        .assets(&j.id, 0, 100)
        .unwrap()
        .items
        .into_iter()
        .map(|a| a.id)
        .collect();
    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(NoRaw),
        RenderLimits::default(),
    ));
    let analysis = Arc::new(AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        None,
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let service = Arc::new(CullingService::new(
        jobs.repository.clone(),
        analysis.clone(),
        engine.clone(),
        Arc::new(MockFaces {
            calls: calls.clone(),
            block,
        }),
        Arc::new(UnavailableEyes),
    ));
    Setup {
        _root: root,
        jobs,
        service,
        analysis,
        engine,
        job: j.id,
        ids,
        calls,
    }
}
fn request(s: &Setup, force: bool) -> CullingRequest {
    CullingRequest {
        job_id: s.job.clone(),
        photo_type: PhotoType::Portrait,
        request_id: uuid::Uuid::new_v4().to_string(),
        force,
    }
}
fn run(s: &Setup, force: bool) -> CullingProgress {
    s.service
        .run(s.service.reserve(request(s, force)).unwrap())
        .unwrap()
}
fn job_assets(s: &Setup) -> Vec<photo_core::models::Asset> {
    s.jobs.repository.assets(&s.job, 0, 100).unwrap().items
}
fn rescan(s: &Setup) {
    let (_, permit) = s.jobs.resume(&s.job).unwrap();
    s.jobs.scan(&s.job, permit).unwrap();
}
fn make_exact(s: &Setup) {
    let a = job_assets(s);
    for dest in &a[1..] {
        std::fs::copy(&a[0].original_path, &dest.original_path).unwrap();
    }
}
#[test]
fn overview_chooses_one_display_representative_without_destroying_scoring_ties() {
    let s = setup(false);
    assert_eq!(run(&s, false).status, "complete");
    let overview = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    let mut groups = std::collections::BTreeMap::<String, Vec<&CullingItem>>::new();
    for item in &overview.items {
        if item.similarity.as_ref().is_some_and(|similarity| {
            matches!(
                similarity.kind,
                DuplicateKind::NearDuplicate | DuplicateKind::Burst
            )
        }) {
            groups
                .entry(item.group_id.clone().unwrap())
                .or_default()
                .push(item);
        }
    }
    assert!(!groups.is_empty());
    assert!(groups.values().any(|members| {
        members[0]
            .similarity
            .as_ref()
            .unwrap()
            .preferred_assets
            .len()
            > 1
    }));
    for members in groups.values() {
        assert_eq!(members.iter().filter(|item| item.preferred).count(), 1);
    }
}
#[test]
fn complete_file_identity_ignores_names_folders_and_reuses_safe_cache() {
    let s = setup(false);
    let a = job_assets(&s);
    let input = s._root.path().join("input");
    std::fs::create_dir(input.join("nested")).unwrap();
    std::fs::create_dir(input.join("different")).unwrap();
    std::fs::copy(&a[0].original_path, input.join("renamed-copy.png")).unwrap();
    std::fs::copy(
        &a[0].original_path,
        input.join("nested").join(&a[0].filename),
    )
    .unwrap();
    image::RgbImage::from_pixel(256, 256, image::Rgb([230u8, 20, 60]))
        .save(input.join("different").join(&a[0].filename))
        .unwrap();
    rescan(&s);
    let rows = job_assets(&s);
    let token = CancellationToken::default();
    let identities: Vec<_> = rows
        .iter()
        .map(|a| {
            let h = content::identify(&s.jobs.repository, a, false, &token).unwrap();
            assert!(!h.cached);
            assert_eq!(h.bytes_hashed, h.content.byte_length);
            (a.id.clone(), h.content)
        })
        .collect();
    let groups = similarity::exact_groups(&identities, &token).unwrap();
    assert_eq!(groups.len(), 3);
    let original = identities.iter().find(|(id, _)| id == &a[0].id).unwrap();
    for row in &rows {
        let h = content::identify(&s.jobs.repository, row, false, &token).unwrap();
        assert!(h.cached);
        assert_eq!(h.bytes_hashed, 0);
        assert_eq!(groups.contains_key(&row.id), h.content == original.1);
    }
    let nested = rows
        .iter()
        .find(|a| a.original_path.parent().unwrap().ends_with("nested"))
        .unwrap();
    assert_eq!(nested.filename, a[0].filename);
    assert_ne!(nested.fingerprint, a[0].fingerprint);
    let different = rows
        .iter()
        .find(|a| a.original_path.parent().unwrap().ends_with("different"))
        .unwrap();
    assert_eq!(different.filename, a[0].filename);
    assert!(!groups.contains_key(&different.id));
    // The original setup's single-pixel variation must not be called exact.
    assert!(!groups.contains_key(&a[1].id));
    let repo = JobRepository::open(s._root.path().join("data/jobs.sqlite3")).unwrap();
    assert!(
        content::identify(&repo, &a[0], false, &token)
            .unwrap()
            .cached
    );
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert_eq!(
        content::identify(&repo, &a[0], true, &cancelled)
            .err()
            .unwrap()
            .code,
        ProcessingErrorCode::Cancelled
    );
}
#[test]
fn exact_rating_override_selection_recipe_and_restart_are_independent() {
    let s = setup(false);
    make_exact(&s);
    let first = run(&s, false);
    assert_eq!(first.status, "complete", "{first:?}");
    assert!(first.hash_bytes > 0);
    assert_eq!(first.hash_cached, 0);
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert!(o.issue_availability.blurry);
    assert!(!o.issue_availability.closed_eyes);
    assert_eq!(o.duplicates.exact_groups, 1);
    assert_eq!(o.duplicates.exact_copies, 2);
    assert_eq!(o.duplicates.unique_images, 0);
    let canonical = s.ids.iter().min().unwrap();
    let copy = s.ids.iter().find(|id| *id != canonical).unwrap();
    for i in &o.items {
        assert_eq!(i.relationship_kind, Some(DuplicateKind::Exact));
        assert_eq!(
            i.similarity
                .as_ref()
                .unwrap()
                .exact
                .as_ref()
                .unwrap()
                .canonical_asset_id,
            *canonical
        );
        if &i.asset.id == canonical {
            assert!(i.ai_rating.unwrap().get() >= 3);
            assert!(i.preferred);
        } else {
            assert_eq!(i.ai_rating, Stars::new(1).ok());
            assert!(!i.preferred);
        }
    }
    s.service
        .set_rating(&s.job, copy, PhotoType::Portrait, Stars::new(5).ok())
        .unwrap();
    s.service.select_asset(&s.job, copy, true).unwrap();
    let before = s
        .service
        .detail(&s.job, copy, PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    for stage in 0..3 {
        let current = s.jobs.repository.get_recipe(&s.job, copy).unwrap();
        let mut recipe = current.recipe.clone();
        match stage {
            0 => recipe.global.basic.exposure_ev = 1.2,
            1 => recipe.global.color_mixer.red.hue = 20.,
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
        }
        s.jobs
            .repository
            .save_recipe(&s.job, copy, &recipe, current.generation, None)
            .unwrap();
        let p = run(&s, false);
        assert_eq!(p.status, "complete", "{p:?}");
        assert_eq!(p.hash_bytes, 0);
        assert_eq!(p.hash_cached, 3);
        assert_eq!(p.cached, 3);
        assert_eq!(
            s.service
                .detail(&s.job, copy, PhotoType::Portrait)
                .unwrap()
                .assessment
                .unwrap(),
            before
        );
    }
    let p = run(&s, true);
    assert_eq!(p.status, "complete");
    assert!(p.hash_bytes > 0);
    assert_eq!(p.hash_cached, 0);
    let repo = JobRepository::open(s._root.path().join("data/jobs.sqlite3")).unwrap();
    let state = repo
        .culling_state(&s.job, copy, PhotoType::Portrait)
        .unwrap();
    assert_eq!(
        state.assessment.as_ref().unwrap().ai_rating,
        Stars::new(1).ok()
    );
    assert_eq!(state.user_rating, Stars::new(5).ok());
    assert_eq!(state.effective_rating, Stars::new(5).ok());
    assert!(state.selected_for_editing);
    assert_eq!(state.assessment.unwrap().similarity, before.similarity);
    s.service
        .set_rating(&s.job, copy, PhotoType::Portrait, None)
        .unwrap();
    let state = s.service.detail(&s.job, copy, PhotoType::Portrait).unwrap();
    assert_eq!(state.effective_rating, Stars::new(1).ok());
    assert!(state.selected_for_editing);
    s.service
        .select_filtered(
            &s.job,
            PhotoType::Portrait,
            &[Stars::new(1).unwrap()],
            RelationshipFilter::Exact,
            false,
            false,
        )
        .unwrap();
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .selected_count,
        2
    );
    s.service
        .select_filtered(
            &s.job,
            PhotoType::Portrait,
            &[
                Stars::new(1).unwrap(),
                Stars::new(2).unwrap(),
                Stars::new(3).unwrap(),
                Stars::new(4).unwrap(),
                Stars::new(5).unwrap(),
            ],
            RelationshipFilter::All,
            false,
            true,
        )
        .unwrap();
    let overview = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(overview.selected_count, 1);
    assert!(
        overview
            .items
            .iter()
            .find(|item| item.asset.id == canonical.as_str())
            .unwrap()
            .selected_for_editing
    );
    s.service.select_asset(&s.job, copy, true).unwrap();
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .selected_count,
        2
    );
}
#[test]
fn exact_exclusion_does_not_collapse_near_or_burst_groups() {
    let s = setup(false);
    assert_eq!(run(&s, false).status, "complete");
    let diagnostic = s
        .service
        .detail(&s.job, &s.ids[0], PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    let group_focus = diagnostic
        .reasons
        .iter()
        .find(|reason| reason.code == ReasonCode::GroupFocusReference)
        .and_then(|reason| reason.measurement.as_ref())
        .unwrap();
    assert_eq!(group_focus.unit, "normalized_detail");
    assert!(group_focus.reference.is_some());
    for asset_id in &s.ids {
        s.service
            .set_rating(&s.job, asset_id, PhotoType::Portrait, Stars::new(4).ok())
            .unwrap();
    }
    s.service
        .select_filtered(
            &s.job,
            PhotoType::Portrait,
            &[Stars::new(4).unwrap()],
            RelationshipFilter::All,
            false,
            true,
        )
        .unwrap();
    let selected = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(selected.duplicates.near_groups, 1);
    assert_eq!(selected.selected_count, 3);

    s.service
        .select_filtered(
            &s.job,
            PhotoType::Portrait,
            &[Stars::new(4).unwrap()],
            RelationshipFilter::All,
            false,
            false,
        )
        .unwrap();
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .selected_count,
        3
    );
}
#[test]
fn explicit_asset_snapshot_selects_exact_ids_and_rejects_foreign_ids() {
    let s = setup(false);
    assert_eq!(run(&s, false).status, "complete");
    s.service
        .select_assets(
            &s.job,
            PhotoType::Portrait,
            &[s.ids[0].clone(), s.ids[2].clone()],
        )
        .unwrap();
    let selected = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(selected.selected_count, 2);
    assert_eq!(
        selected
            .items
            .iter()
            .filter(|item| item.selected_for_editing)
            .map(|item| item.asset.id.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        [&s.ids[0], &s.ids[2]]
            .into_iter()
            .map(String::as_str)
            .collect()
    );

    assert!(s
        .service
        .select_assets(&s.job, PhotoType::Portrait, &["not-in-this-job".into()],)
        .is_err());
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .selected_count,
        2
    );
}
#[test]
fn rescan_adds_exact_and_near_members_without_rehashing_unchanged_sources() {
    let s = setup(false);
    assert_eq!(run(&s, false).status, "complete");
    let a = job_assets(&s);
    let old = s
        .service
        .detail(&s.job, &a[0].id, PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    std::fs::copy(
        &a[0].original_path,
        s._root.path().join("input/later-copy.png"),
    )
    .unwrap();
    rescan(&s);
    assert!(
        s.service
            .detail(&s.job, &a[0].id, PhotoType::Portrait)
            .unwrap()
            .stale
    );
    let p = run(&s, false);
    assert_eq!(p.status, "complete", "{p:?}");
    assert_eq!(p.cached, 3);
    assert_eq!(p.hash_cached, 3);
    assert_eq!(
        p.hash_bytes,
        std::fs::metadata(&a[0].original_path).unwrap().len()
    );
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(o.items.len(), 4);
    assert_eq!(o.duplicates.exact_groups, 1);
    assert_eq!(o.duplicates.exact_copies, 1);
    assert_eq!(o.duplicates.near_groups, 1);
    assert!(o.items.iter().all(|i| !i.stale));
    let group = o.items[0].similarity.as_ref().unwrap();
    assert_eq!(group.group_size, 4);
    assert_ne!(group.group_id, old.similarity.group_id);
    assert!(o
        .items
        .iter()
        .filter(|i| i.ai_rating == Stars::new(1).ok())
        .all(|i| i.relationship_kind == Some(DuplicateKind::Exact)));
    let mut new = image::open(&a[0].original_path).unwrap().to_rgb8();
    new.put_pixel(2, 2, image::Rgb([50, 51, 52]));
    new.save(s._root.path().join("input/later-near.png"))
        .unwrap();
    rescan(&s);
    let p = run(&s, false);
    assert_eq!(p.status, "complete");
    assert_eq!(p.hash_cached, 4);
    assert_eq!(p.cached, 4);
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(o.duplicates.exact_copies, 1);
    let new = o
        .items
        .iter()
        .find(|i| i.asset.filename == "later-near.png")
        .unwrap();
    assert_eq!(new.relationship_kind, Some(DuplicateKind::NearDuplicate));
    assert_eq!(new.similarity.as_ref().unwrap().group_size, 5);
    assert!(new.ai_rating.unwrap().get() > 1);
}
#[test]
fn size_and_mtime_preserving_edit_invalidates_content_analysis_and_overlapping_groups() {
    use std::{
        fs::{File, FileTimes, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
    };
    let s = setup(false);
    let a = job_assets(&s);
    std::fs::copy(&a[0].original_path, &a[1].original_path).unwrap();
    assert_eq!(run(&s, false).status, "complete");
    let before = s
        .service
        .detail(&s.job, &a[0].id, PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    let m = std::fs::metadata(&a[0].original_path).unwrap();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&a[0].original_path)
        .unwrap();
    // Appended ancillary bytes do not decode; changing the PNG header preserves size/mtime
    // and proves old visual evidence is not reused for an unprocessable changed source.
    let mut b = [0u8];
    file.read_exact(&mut b).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(&[b[0] ^ 1]).unwrap();
    file.sync_all().unwrap();
    drop(file);
    File::options()
        .write(true)
        .open(&a[0].original_path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(m.modified().unwrap()))
        .unwrap();
    assert_eq!(
        std::fs::metadata(&a[0].original_path).unwrap().len(),
        m.len()
    );
    assert_eq!(
        std::fs::metadata(&a[0].original_path)
            .unwrap()
            .modified()
            .unwrap(),
        m.modified().unwrap()
    );
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert!(o.items.iter().all(|i| i.stale));
    let p = run(&s, false);
    assert_eq!(p.status, "complete", "{p:?}");
    assert_eq!(p.hash_cached, 2);
    assert_eq!(p.failed, 1);
    let after = s
        .service
        .detail(&s.job, &a[0].id, PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    assert_ne!(after.duplicate_content, before.duplicate_content);
    assert!(after.features.is_none());
    assert!(after.source_analysis_id.is_none());
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .duplicates
            .exact_groups,
        0
    );
}
#[test]
fn exact_detection_still_works_for_undecodable_bytes_without_quality_claims() {
    let s = setup(false);
    for a in job_assets(&s) {
        std::fs::write(a.original_path, b"same corrupt photograph bytes").unwrap();
    }
    let p = run(&s, false);
    assert_eq!(p.status, "complete", "{p:?}");
    assert_eq!(p.failed, 3);
    assert_eq!(p.hash_failures, 0);
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(o.duplicates.exact_copies, 2);
    assert_eq!(o.counts[0], 1);
    assert_eq!(o.counts[1], 2);
    for i in &o.items {
        let a = s
            .service
            .detail(&s.job, &i.asset.id, PhotoType::Portrait)
            .unwrap()
            .assessment
            .unwrap();
        assert!(a.features.is_none());
        assert!(a
            .reasons
            .iter()
            .any(|r| r.code == ReasonCode::SourceUnavailable));
        if !i.preferred {
            assert!(a
                .reasons
                .iter()
                .any(|r| r.code == ReasonCode::ExactDuplicate));
        }
    }
}
#[test]
fn visual_categories_use_content_time_camera_and_conservative_moment_semantics() {
    let mut a = f();
    a.asset_id = "a".into();
    a.descriptor.capture_timestamp = None;
    a.descriptor.camera = None;
    let mut tiny = a.clone();
    tiny.asset_id = "b".into();
    tiny.descriptor.luminance_grid[0] += 0.001;
    let near =
        similarity::group(&[a.clone(), tiny.clone()], &CancellationToken::default()).unwrap();
    assert!(near
        .iter()
        .all(|g| g.kind == DuplicateKind::NearDuplicate && g.exact.is_none()));
    a.descriptor.capture_timestamp = Some("2026:09:04 12:00:00".into());
    a.descriptor.camera = Some("Camera A".into());
    tiny.descriptor.camera = a.descriptor.camera.clone();
    tiny.descriptor.capture_timestamp = Some("2026:09:04 12:00:02.250".into());
    assert_eq!(
        similarity::classify(&a.descriptor, &tiny.descriptor)
            .unwrap()
            .kind,
        DuplicateKind::Burst
    );
    tiny.descriptor.capture_timestamp = Some("2026:09:04 12:15:00".into());
    let similar =
        similarity::group(&[a.clone(), tiny.clone()], &CancellationToken::default()).unwrap();
    assert!(similar.iter().all(|g| g.kind == DuplicateKind::Similar));
    for (f, g) in [a.clone(), tiny.clone()].iter().zip(&similar) {
        assert_eq!(score(f, g).unwrap().score, scored(f).score);
        assert_eq!(score(f, g).unwrap().rating, scored(f).rating);
    }
    tiny.descriptor.difference_hash = "0000000000000000".into();
    let unique = similarity::group(&[a, tiny], &CancellationToken::default()).unwrap();
    assert!(unique.iter().all(|g| g.kind == DuplicateKind::Unique));
    let mut flat = f().descriptor;
    flat.luminance_grid.fill(0.3);
    assert!(similarity::classify(&flat, &flat).is_none());
}
#[test]
fn exact_canonical_is_deterministic_and_not_a_second_visual_preferred_frame() {
    let mut a = f();
    a.asset_id = "a".into();
    let mut b = a.clone();
    b.asset_id = "b".into();
    let mut c = a.clone();
    c.asset_id = "c".into();
    faces(&mut c)[0].visible_fraction = 0.6;
    let content = DuplicateContent {
        sha256: "a".repeat(64),
        byte_length: 100,
    };
    let entries = vec![("b".into(), content.clone()), ("a".into(), content)];
    let token = CancellationToken::default();
    let exact = similarity::exact_groups(&entries, &token).unwrap();
    let mut reversed = entries;
    reversed.reverse();
    assert_eq!(exact, similarity::exact_groups(&reversed, &token).unwrap());
    assert_eq!(exact["b"].canonical_asset_id, "a");
    let features = vec![b, c, a];
    let contexts = similarity::group_with_exact(&features, &exact, &token).unwrap();
    assert_eq!(contexts[0].kind, DuplicateKind::Burst);
    assert_eq!(contexts[0].group_size, 3);
    assert!(!contexts[0].preferred);
    assert_eq!(contexts[0].preferred_assets, vec!["a"]);
    assert!(!score(&features[0], &contexts[0])
        .unwrap()
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::SimilarAlternative));
    assert_eq!(score(&features[0], &contexts[0]).unwrap().rating.get(), 1);
    assert!(score(&features[1], &contexts[1]).unwrap().rating.get() > 1);
    assert_eq!(score(&features[2], &contexts[2]).unwrap().rating.get(), 5);
    let mut shuffled = features.clone();
    shuffled.reverse();
    let other = similarity::group_with_exact(&shuffled, &exact, &token).unwrap();
    for (f, g) in features.iter().zip(contexts) {
        let index = shuffled
            .iter()
            .position(|v| v.asset_id == f.asset_id)
            .unwrap();
        assert_eq!(g, other[index]);
    }
}
#[test]
fn duplicate_grouping_scales_to_500_1000_3000_and_global_exact_has_no_anchor_limit() {
    let token = CancellationToken::default();
    let base = f();
    for n in [500, 1000, 3000] {
        let features: Vec<_> = (0..n)
            .map(|i| {
                let mut f = base.clone();
                f.asset_id = format!("asset-{i:04}");
                f
            })
            .collect();
        let content: Vec<_> = features
            .iter()
            .enumerate()
            .map(|(i, f)| {
                (
                    f.asset_id.clone(),
                    DuplicateContent {
                        sha256: format!("{:064x}", i % 200),
                        byte_length: 100,
                    },
                )
            })
            .collect();
        let start = Instant::now();
        let exact = similarity::exact_groups(&content, &token).unwrap();
        let hash_group_ms = start.elapsed().as_millis();
        let start = Instant::now();
        let groups = similarity::group_with_exact(&features, &exact, &token).unwrap();
        let visual_ms = start.elapsed().as_millis();
        assert_eq!(groups.len(), n);
        assert!(groups.iter().all(|g| g.exact.is_some()));
        assert_eq!(
            exact[&format!("asset-{:04}", n - 1)].canonical_asset_id,
            format!("asset-{:04}", (n - 1) % 200)
        );
        assert!(groups.iter().all(|g| g.validate().is_ok()));
        eprintln!("Structured duplicate groups {n}: exact {hash_group_ms} ms, visual {visual_ms} ms (200 distinct-content representatives)");
        let start = Instant::now();
        let plain = similarity::group(&features, &token).unwrap();
        assert_eq!(plain.len(), n);
        eprintln!(
            "Structured visual groups {n} distinct sources: {} ms",
            start.elapsed().as_millis()
        );
    }
    let entries: Vec<_> = (0..3000)
        .rev()
        .map(|i| {
            (
                format!("{i:04}"),
                DuplicateContent {
                    sha256: "f".repeat(64),
                    byte_length: 1,
                },
            )
        })
        .collect();
    let exact = similarity::exact_groups(&entries, &token).unwrap();
    assert_eq!(exact.len(), 3000);
    assert_eq!(exact["2999"].group_size, 3000);
    assert_eq!(exact["2999"].canonical_asset_id, "0000");
}
#[test]
fn full_file_hashing_cost_is_separate_and_resume_reads_zero_content_bytes() {
    let s = setup(false);
    let asset = job_assets(&s).remove(0);
    let bytes = vec![0x5au8; 32 * 1024 * 1024];
    std::fs::write(&asset.original_path, &bytes).unwrap();
    let token = CancellationToken::default();
    let first = content::identify(&s.jobs.repository, &asset, false, &token).unwrap();
    assert_eq!(first.bytes_hashed, 32 * 1024 * 1024);
    assert!(!first.cached);
    let cached = content::identify(&s.jobs.repository, &asset, false, &token).unwrap();
    assert_eq!(cached.bytes_hashed, 0);
    assert!(cached.cached);
    assert_eq!(first.content, cached.content);
    eprintln!("Synthetic 32 MiB complete-file SHA-256: {} ms; cached identity: {} ms, {} content bytes read",first.duration_ms,cached.duration_ms,cached.bytes_hashed);
    let forced = content::identify(&s.jobs.repository, &asset, true, &token).unwrap();
    assert!(!forced.cached);
    assert_eq!(forced.content, first.content);
}
#[test]
fn persistence_override_rerun_clear_selection_and_recipe_independence() {
    let s = setup(false);
    let repo = &s.jobs.repository;
    assert_eq!(run(&s, false).status, "complete");
    let a = &s.ids[0];
    let first = s
        .service
        .detail(&s.job, a, PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    assert_eq!(s.calls.load(Ordering::SeqCst), 3);
    let before = std::fs::read(repo.asset(&s.job, a).unwrap().original_path).unwrap();
    for stage in 0..3 {
        let current = repo.get_recipe(&s.job, a).unwrap();
        let mut recipe = current.recipe.clone();
        match stage {
            0 => recipe.global.basic.exposure_ev = 1.2,
            1 => recipe.global.color_mixer.red.hue = 20.,
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
        }
        repo.save_recipe(&s.job, a, &recipe, current.generation, None)
            .unwrap();
        assert_eq!(run(&s, false).cached, 3);
        assert_eq!(
            s.service
                .detail(&s.job, a, PhotoType::Portrait)
                .unwrap()
                .assessment
                .unwrap(),
            first
        );
    }
    s.service
        .set_rating(&s.job, a, PhotoType::Portrait, Stars::new(5).ok())
        .unwrap();
    s.service
        .select_ratings(&s.job, PhotoType::Portrait, &[Stars::new(5).unwrap()])
        .unwrap();
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .selected_count,
        3
    );
    assert_eq!(run(&s, true).status, "complete");
    let after = s.service.detail(&s.job, a, PhotoType::Portrait).unwrap();
    assert_eq!(after.user_rating, Stars::new(5).ok());
    assert_eq!(after.effective_rating, Stars::new(5).ok());
    assert!(after.selected_for_editing);
    assert_ne!(
        after.assessment.as_ref().unwrap().assessment_id,
        first.assessment_id
    );
    let reopened = JobRepository::open(s._root.path().join("data/jobs.sqlite3")).unwrap();
    let state = reopened
        .culling_state(&s.job, a, PhotoType::Portrait)
        .unwrap();
    assert_eq!(state.assessment, after.assessment);
    assert_eq!(state.user_rating, after.user_rating);
    assert!(state.selected_for_editing);
    s.service
        .set_rating(&s.job, a, PhotoType::Portrait, None)
        .unwrap();
    let clear = s.service.detail(&s.job, a, PhotoType::Portrait).unwrap();
    assert_eq!(
        clear.effective_rating,
        clear.assessment.as_ref().unwrap().ai_rating
    );
    assert!(clear.selected_for_editing);
    assert_eq!(
        before,
        std::fs::read(repo.asset(&s.job, a).unwrap().original_path).unwrap()
    );
}
#[test]
fn source_and_analysis_change_invalidate_ai_but_not_user_or_selection() {
    let s = setup(false);
    run(&s, false);
    let a = &s.ids[0];
    s.service
        .set_rating(&s.job, a, PhotoType::Portrait, Stars::new(5).ok())
        .unwrap();
    s.service.select_asset(&s.job, a, true).unwrap();
    let path = s.jobs.repository.asset(&s.job, a).unwrap().original_path;
    image::RgbImage::from_pixel(300, 200, image::Rgb([100u8; 3]))
        .save(path)
        .unwrap();
    let state = s.service.detail(&s.job, a, PhotoType::Portrait).unwrap();
    assert!(state.stale);
    assert_eq!(state.effective_rating, Stars::new(5).ok());
    assert!(state.selected_for_editing);
    run(&s, false);
    assert!(
        !s.service
            .detail(&s.job, a, PhotoType::Portrait)
            .unwrap()
            .stale
    );
    s.analysis.invalidate_analysis(&s.job, a).unwrap();
    assert!(
        s.service
            .detail(&s.job, a, PhotoType::Portrait)
            .unwrap()
            .stale
    );
}
#[test]
fn cache_models_type_and_feature_versions_independent_of_user_rating() {
    let a = f();
    let k = feature_key(
        &a.source_fingerprint,
        &a.source_analysis_id,
        a.photo_type,
        &a.models,
    );
    assert_ne!(
        k,
        feature_key("different", &a.source_analysis_id, a.photo_type, &a.models)
    );
    assert_ne!(
        k,
        feature_key(
            &a.source_fingerprint,
            "new-analysis",
            a.photo_type,
            &a.models
        )
    );
    assert_ne!(
        k,
        feature_key(
            &a.source_fingerprint,
            &a.source_analysis_id,
            PhotoType::Landscape,
            &a.models
        )
    );
    let mut models = a.models.clone();
    models[0].version = "new".into();
    assert_ne!(
        k,
        feature_key(
            &a.source_fingerprint,
            &a.source_analysis_id,
            a.photo_type,
            &models
        )
    );
}
#[test]
fn cancellation_preserves_completed_work_and_resume_reuses_features() {
    let s = setup(true);
    let req = request(&s, false);
    let id = req.request_id.clone();
    let permit = s.service.reserve(req).unwrap();
    assert!(s.service.reserve(request(&s, false)).is_err());
    let service = s.service.clone();
    let worker = std::thread::spawn(move || service.run(permit).unwrap());
    let started = Instant::now();
    while s.calls.load(Ordering::SeqCst) < 2 {
        assert!(started.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    s.service.cancel(&id).unwrap();
    let p = worker.join().unwrap();
    assert_eq!(p.status, "cancelled");
    assert_eq!(p.completed, 1);
    assert_eq!(
        s.service
            .overview(&s.job, PhotoType::Portrait)
            .unwrap()
            .items
            .iter()
            .filter(|i| i.ai_rating.is_some())
            .count(),
        1
    );
    let done = run(&s, false);
    assert_eq!(done.status, "complete");
    assert_eq!(done.cached, 1);
    assert_eq!(done.hash_cached, 3);
    assert_eq!(done.hash_bytes, 0);
}
#[test]
fn cancelled_exact_batch_keeps_manual_duplicate_selection_until_and_after_regroup() {
    let s = setup(true);
    make_exact(&s);
    let copy = s.ids.iter().max().unwrap();
    s.service.select_asset(&s.job, copy, true).unwrap();
    s.service
        .set_rating(&s.job, copy, PhotoType::Portrait, Stars::new(5).ok())
        .unwrap();
    let request = request(&s, false);
    let id = request.request_id.clone();
    let permit = s.service.reserve(request).unwrap();
    let service = s.service.clone();
    let worker = std::thread::spawn(move || service.run(permit).unwrap());
    let start = Instant::now();
    while s.calls.load(Ordering::SeqCst) < 2 {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    s.service.cancel(&id).unwrap();
    assert_eq!(worker.join().unwrap().status, "cancelled");
    let state = s
        .jobs
        .repository
        .culling_state(&s.job, copy, PhotoType::Portrait)
        .unwrap();
    assert!(state.selected_for_editing);
    assert_eq!(state.effective_rating, Stars::new(5).ok());
    let p = run(&s, false);
    assert_eq!(p.status, "complete", "{p:?}");
    assert_eq!(p.hash_bytes, 0);
    let state = s.service.detail(&s.job, copy, PhotoType::Portrait).unwrap();
    assert!(state.selected_for_editing);
    assert_eq!(state.effective_rating, Stars::new(5).ok());
    assert_eq!(state.assessment.unwrap().ai_rating, Stars::new(1).ok());
}
#[test]
fn unreadable_identity_is_unclassified_and_does_not_abort_other_photos() {
    let s = setup(false);
    let a = job_assets(&s).remove(0);
    std::fs::remove_file(&a.original_path).unwrap();
    let p = run(&s, false);
    assert_eq!(p.status, "complete", "{p:?}");
    assert_eq!(p.hash_failures, 1);
    assert_eq!(p.failed, 1);
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(o.duplicates.unclassified_images, 1);
    let missing = o.items.iter().find(|i| i.asset.id == a.id).unwrap();
    assert_eq!(missing.relationship_kind, None);
    assert_eq!(missing.ai_rating, None);
    let a = s
        .service
        .detail(&s.job, &a.id, PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    assert!(a
        .reasons
        .iter()
        .any(|r| r.code == ReasonCode::DuplicateIdentityUnavailable));
    assert!(a.duplicate_content.is_none());
}
#[test]
fn dropped_reservation_and_restart_report_interrupted() {
    let s = setup(false);
    drop(s.service.reserve(request(&s, false)).unwrap());
    assert_eq!(
        s.service.progress(&s.job).unwrap().unwrap().status,
        "cancelled"
    );
    let permit = s.service.reserve(request(&s, false)).unwrap();
    s.jobs.repository.recover_interrupted().unwrap();
    assert_eq!(
        s.service.progress(&s.job).unwrap().unwrap().status,
        "interrupted"
    );
    drop(permit);
}
#[test]
fn missing_and_real_runtime_models_fail_gracefully_and_do_not_create_eye_claims() {
    let root = tempfile::tempdir().unwrap();
    let missing = YuNetDetector {
        toolkit: root.path().join("absent"),
        scratch: root.path().join("scratch"),
    };
    assert!(matches!(
        missing
            .detect(&image(false), &CancellationToken::default())
            .unwrap(),
        Signal::Unavailable { .. }
    ));
    let runtime = YuNetDetector {
        toolkit: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.resources/toolkit"),
        scratch: root.path().join("scratch"),
    };
    if runtime.identity().version.ends_with("missing") {
        eprintln!("Prepared YuNet not available; runtime smoke skipped");
        return;
    }
    let mut blank = image(false);
    blank.pixels.fill([0.3; 3]);
    let started = Instant::now();
    let result = runtime
        .detect(&blank, &CancellationToken::default())
        .unwrap();
    assert!(
        matches!(&result,Signal::Available{value,..}if value.is_empty()),
        "{result:?}"
    );
    eprintln!(
        "YuNet blank 640 input: {} ms",
        started.elapsed().as_millis()
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("scratch"))
            .unwrap()
            .count(),
        0
    );
}
#[test]
fn extraction_exposes_per_person_outliers_and_unavailable_eyes() {
    let mut analysis =
        PhotoAnalysis::parse(include_str!("../../../src/test/analysis-fixture.json")).unwrap();
    analysis.photo_type = PhotoType::Portrait;
    let detector = MockFaces {
        calls: Arc::new(AtomicUsize::new(0)),
        block: false,
    };
    let f = extract(
        &image(false),
        &analysis,
        &detector,
        &UnavailableEyes,
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(f.people.faces.value().unwrap().len(), 1);
    assert!(matches!(
        f.people.faces.value().unwrap()[0].eyes,
        Signal::Unavailable { .. }
    ));
    assert!(f.people.softest_subject.is_some());
}
#[test]
fn synthetic_timing_single_small_portrait_batch_and_grouping() {
    let s = setup(false);
    let a = &s.ids[0];
    let p = s
        .analysis
        .reserve(photo_core::analysis::AnalysisRequest {
            job_id: s.job.clone(),
            asset_id: a.clone(),
            photo_type: PhotoType::Portrait,
            request_id: "timing".into(),
        })
        .unwrap();
    let start = Instant::now();
    let analysis = s.analysis.analyze_asset(p).unwrap().analysis.unwrap();
    let path = s.jobs.repository.asset(&s.job, a).unwrap().original_path;
    let input = s
        .engine
        .analysis_input(&path, &CancellationToken::default())
        .unwrap();
    let detector = MockFaces {
        calls: Arc::new(AtomicUsize::new(0)),
        block: false,
    };
    let features = extract(
        &input.image,
        &analysis,
        &detector,
        &UnavailableEyes,
        &CancellationToken::default(),
    )
    .unwrap();
    assess(features, SimilarityContext::default()).unwrap();
    eprintln!(
        "Synthetic single portrait 256px (mock detector): {} ms",
        start.elapsed().as_millis()
    );
    let start = Instant::now();
    assert_eq!(run(&s, false).status, "complete");
    eprintln!(
        "Synthetic portrait batch3 (mock detector, prior unbound analysis refreshed): {} ms",
        start.elapsed().as_millis()
    );
    let list: Vec<_> = (0..1000)
        .map(|i| {
            let mut a = f();
            a.asset_id = format!("{i:04}");
            a
        })
        .collect();
    let start = Instant::now();
    let contexts = similarity::group(&list, &CancellationToken::default()).unwrap();
    assert_eq!(contexts.len(), 1000);
    eprintln!(
        "Structured similarity1000: {} ms",
        start.elapsed().as_millis()
    );
}

struct FailingFaces;
impl FaceDetector for FailingFaces {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider: "FaceDetector".into(),
            model: "failed-test".into(),
            version: "v1".into(),
        }
    }
    fn detect(
        &self,
        _: &FloatImage,
        _: &CancellationToken,
    ) -> ProcessingResult<Signal<Vec<Detection>>> {
        Err(photo_core::rendering::internal("Model cannot load"))
    }
}
#[test]
fn optional_detector_failure_preserves_measurements_and_rating() {
    let a = PhotoAnalysis::parse(include_str!("../../../src/test/analysis-fixture.json")).unwrap();
    let f = extract(
        &image(false),
        &a,
        &FailingFaces,
        &UnavailableEyes,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(matches!(f.people.faces, Signal::Failed { .. }));
    assert!(scored(&f).rating.get() >= 3);
    let token = CancellationToken::default();
    token.cancel();
    assert!(extract(&image(false), &a, &FailingFaces, &UnavailableEyes, &token).is_err());
}

#[test]
fn persisted_groups_invalidate_together_and_regroup_after_source_change() {
    let s = setup(false);
    run(&s, false);
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert!(o.items.iter().all(|i| i.group_id.is_some()));
    assert!(o.items.iter().all(|i| i.group_id == o.items[0].group_id));
    let path = s
        .jobs
        .repository
        .asset(&s.job, &s.ids[0])
        .unwrap()
        .original_path;
    image::RgbImage::from_pixel(120, 80, image::Rgb([110u8; 3]))
        .save(path)
        .unwrap();
    let stale = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert!(stale.items.iter().all(|i| i.stale));
    assert!(stale.items.iter().all(|i| i.ai_rating.is_none()));
    run(&s, false);
    let fresh = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert!(fresh.items.iter().all(|i| !i.stale));
    assert_eq!(
        fresh.items.iter().filter(|i| i.group_id.is_some()).count(),
        2
    );
}

#[test]
fn history_preserves_ai_two_user_five_then_ai_one_and_clear() {
    let s = setup(false);
    let repo = &s.jobs.repository;
    let mut features = f();
    features.asset_id = s.ids[0].clone();
    let mut first = assess(features.clone(), SimilarityContext::default()).unwrap();
    first.ai_rating = Stars::new(2).ok();
    repo.persist_culling(&s.job, &[first.clone()], &CancellationToken::default())
        .unwrap();
    repo.culling_override(&s.job, &s.ids[0], PhotoType::Portrait, Stars::new(5).ok())
        .unwrap();
    let mut second = assess(features, SimilarityContext::default()).unwrap();
    second.ai_rating = Stars::new(1).ok();
    repo.persist_culling(&s.job, &[second.clone()], &CancellationToken::default())
        .unwrap();
    let state = repo
        .culling_state(&s.job, &s.ids[0], PhotoType::Portrait)
        .unwrap();
    assert_eq!(state.assessment.unwrap().ai_rating, Stars::new(1).ok());
    assert_eq!(state.user_rating, Stars::new(5).ok());
    assert_eq!(state.effective_rating, Stars::new(5).ok());
    let db = rusqlite::Connection::open(s._root.path().join("data/jobs.sqlite3")).unwrap();
    let old: String = db
        .query_row(
            "SELECT payload FROM culling_assessments WHERE assessment_id=?1",
            [&first.assessment_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(CullingAssessment::parse(&old).unwrap(), first);
    let referenced: String = db
        .query_row(
            "SELECT assessment_id FROM culling_rating_events ORDER BY event_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(referenced, first.assessment_id);
    repo.culling_override(&s.job, &s.ids[0], PhotoType::Portrait, None)
        .unwrap();
    assert_eq!(
        repo.culling_state(&s.job, &s.ids[0], PhotoType::Portrait)
            .unwrap()
            .effective_rating,
        Stars::new(1).ok()
    );
}

#[test]
fn group_persistence_rolls_back_and_immutable_id_cannot_be_rewritten() {
    let s = setup(false);
    let mut a = f();
    a.asset_id = s.ids[0].clone();
    let first = assess(a, SimilarityContext::default()).unwrap();
    let mut invalid = first.clone();
    invalid.asset_id = s.ids[1].clone();
    assert!(s
        .jobs
        .repository
        .persist_culling(
            &s.job,
            &[first.clone(), invalid],
            &CancellationToken::default()
        )
        .is_err());
    assert!(s
        .jobs
        .repository
        .culling_state(&s.job, &s.ids[0], PhotoType::Portrait)
        .unwrap()
        .assessment
        .is_none());
    let token = CancellationToken::default();
    token.cancel();
    assert!(s
        .jobs
        .repository
        .persist_culling(&s.job, std::slice::from_ref(&first), &token)
        .is_err());
    s.jobs
        .repository
        .persist_culling(
            &s.job,
            std::slice::from_ref(&first),
            &CancellationToken::default(),
        )
        .unwrap();
    let mut rewritten = first.clone();
    rewritten.ai_rating = Stars::new(1).ok();
    assert!(s
        .jobs
        .repository
        .persist_culling(&s.job, &[rewritten], &CancellationToken::default())
        .is_err());
    assert_eq!(
        s.jobs
            .repository
            .culling_state(&s.job, &s.ids[0], PhotoType::Portrait)
            .unwrap()
            .assessment
            .unwrap(),
        first
    );
}

#[test]
fn corrupt_source_is_unrated_nonfatal_and_manual_selection_still_works() {
    let s = setup(false);
    let id = &s.ids[0];
    let path = s.jobs.repository.asset(&s.job, id).unwrap().original_path;
    std::fs::write(path, b"not a photograph").unwrap();
    let p = run(&s, false);
    assert_eq!(p.status, "complete", "{:?}", p);
    assert_eq!(p.failed, 1);
    let state = s.service.detail(&s.job, id, PhotoType::Portrait).unwrap();
    assert!(state.assessment.as_ref().unwrap().ai_rating.is_none());
    assert_eq!(
        state.assessment.unwrap().reasons[0].code,
        ReasonCode::SourceUnavailable
    );
    s.service
        .set_rating(&s.job, id, PhotoType::Portrait, Stars::new(4).ok())
        .unwrap();
    s.service.select_asset(&s.job, id, true).unwrap();
    let o = s.service.overview(&s.job, PhotoType::Portrait).unwrap();
    assert_eq!(o.selected_count, 1);
    assert_eq!(
        o.items
            .iter()
            .find(|i| i.asset.id == *id)
            .unwrap()
            .effective_rating,
        Stars::new(4).ok()
    );
}

#[test]
fn bracket_like_frames_and_cancelled_grouping() {
    let mut a = f();
    a.asset_id = "a".into();
    let mut b = a.clone();
    b.asset_id = "b".into();
    for v in &mut b.descriptor.luminance_grid {
        *v += 0.15;
    }
    b.descriptor.mean_luminance += 0.15;
    for v in &mut b.descriptor.color_grid {
        *v += 0.15;
    }
    let g = similarity::group(&[a, b], &CancellationToken::default()).unwrap();
    assert!(g[0].bracket_like);
    let token = CancellationToken::default();
    token.cancel();
    assert!(similarity::group(&[f()], &token).is_err());
}

#[test]
fn large_group_reason_budget_and_invalid_context() {
    let mut a = f();
    let face = faces(&mut a)[0].clone();
    a.people.faces = Signal::available(
        (0..64)
            .map(|i| FaceFeatures {
                index: i,
                visible_fraction: 0.5,
                highlight_clip_fraction: 0.7,
                ..face.clone()
            })
            .collect(),
        0.9,
    );
    assess(a, SimilarityContext::default()).unwrap();
    let invalid = SimilarityContext {
        confidence: f64::NAN,
        ..Default::default()
    };
    assert!(score(&f(), &invalid).is_err());
}

#[test]
fn synthetic_directional_motion_like_average_reduces_local_detail() {
    let clean = FloatImage {
        width: 128,
        height: 128,
        pixels: (0..128 * 128)
            .map(|i| {
                [if (i % 128 / 8 + i / 128 / 8) % 2 == 0 {
                    0.1
                } else {
                    0.8
                }; 3]
            })
            .collect(),
    };
    let mut motion = clean.clone();
    for y in 0..128 {
        for x in 0..128 {
            let mean = (-8i32..=8)
                .map(|dx| clean.pixels[y * 128 + (x as i32 + dx).clamp(0, 127) as usize][0])
                .sum::<f32>()
                / 17.;
            motion.pixels[y * 128 + x] = [mean; 3];
        }
    }
    let b = BoundingBox {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };
    let c = local_metrics(&clean, &b);
    let m = local_metrics(&motion, &b);
    assert!(m.0 < c.0);
    assert!(m.4 > c.4);
    assert!((m.1 - c.1).abs() < 0.01);
}

struct ChangedModel(Arc<AtomicUsize>);
impl FaceDetector for ChangedModel {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider: "FaceDetector".into(),
            model: "structured-fixture".into(),
            version: "v2".into(),
        }
    }
    fn detect(
        &self,
        image: &FloatImage,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Signal<Vec<Detection>>> {
        MockFaces {
            calls: self.0.clone(),
            block: false,
        }
        .detect(image, cancel)
    }
}
#[test]
fn installed_model_version_change_reextracts_features_but_reuses_phase_four() {
    let s = setup(false);
    run(&s, false);
    let first = s
        .service
        .detail(&s.job, &s.ids[0], PhotoType::Portrait)
        .unwrap()
        .assessment
        .unwrap();
    s.service
        .set_rating(&s.job, &s.ids[0], PhotoType::Portrait, Stars::new(5).ok())
        .unwrap();
    let service = CullingService::new(
        s.jobs.repository.clone(),
        s.analysis.clone(),
        s.engine.clone(),
        Arc::new(ChangedModel(s.calls.clone())),
        Arc::new(UnavailableEyes),
    );
    assert!(
        service
            .detail(&s.job, &s.ids[0], PhotoType::Portrait)
            .unwrap()
            .stale
    );
    let result = service
        .run(service.reserve(request(&s, false)).unwrap())
        .unwrap();
    assert_eq!(result.status, "complete");
    assert_eq!(result.cached, 0);
    assert_eq!(s.calls.load(Ordering::SeqCst), 6);
    let state = service
        .detail(&s.job, &s.ids[0], PhotoType::Portrait)
        .unwrap();
    let next = state.assessment.unwrap();
    assert_eq!(first.source_analysis_id, next.source_analysis_id);
    assert_ne!(first.cache_key, next.cache_key);
    assert_eq!(next.model_versions[0].version, "v2");
    assert_eq!(state.user_rating, Stars::new(5).ok());
}
