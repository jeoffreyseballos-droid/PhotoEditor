use photo_contracts::{
    analysis::{PhotoType, RealEstateAnalysis, TypeAnalysis},
    batch_context::{BatchGroupKind, ConsistencyNoteCode, SequenceKind},
    culling::{DuplicateKind, SimilarityContext},
    CancellationToken, EditRecipe,
};
use photo_core::batch_context::{build_from_inputs, selection_identity, BatchAssetInput};

fn input(
    id: &str,
    timestamp: &str,
    hash: &str,
    luminance: f64,
    warm: f64,
    tint: f64,
    score: f64,
) -> BatchAssetInput {
    let mut analysis = photo_contracts::analysis::PhotoAnalysis::parse(include_str!(
        "../../../src/test/analysis-fixture.json"
    ))
    .unwrap();
    analysis.asset_id = id.into();
    analysis.analysis_id = format!("analysis-{id}");
    analysis.source_fingerprint = format!("{:0<64}", id.replace('-', ""));
    analysis.common.source.capture_timestamp = Some(timestamp.into());
    analysis.common.exposure.median_luminance = luminance;
    analysis.common.exposure.percentiles.p50 = luminance;
    analysis.common.color.warm_cool_balance = warm;
    analysis.common.color.green_magenta_balance = tint;

    let mut culling = photo_contracts::culling::CullingAssessment::parse(include_str!(
        "../../../src/test/culling-fixture.json"
    ))
    .unwrap();
    culling.asset_id = id.into();
    culling.assessment_id = format!("assessment-{id}");
    culling.source_analysis_id = Some(analysis.analysis_id.clone());
    culling.source_fingerprint = analysis.source_fingerprint.clone();
    culling.absolute_score = score;
    culling.final_score = score;
    culling.confidence = 0.85;
    let features = culling.features.as_mut().unwrap();
    features.asset_id = id.into();
    features.source_analysis_id = analysis.analysis_id.clone();
    features.source_fingerprint = analysis.source_fingerprint.clone();
    features.descriptor.capture_timestamp = Some(timestamp.into());
    features.descriptor.difference_hash = hash.into();
    let shift = luminance - features.descriptor.mean_luminance;
    for value in &mut features.descriptor.luminance_grid {
        *value = (*value + shift).clamp(0., 1.);
    }
    features.descriptor.mean_luminance = luminance;
    BatchAssetInput {
        asset_id: id.into(),
        source_fingerprint: format!("source-{id}"),
        analysis: Some(analysis),
        culling: Some(culling),
        unavailable_reason: None,
    }
}

fn explicit_group(inputs: &mut [BatchAssetInput], group: char, kind: DuplicateKind, bracket: bool) {
    let group_id = group.to_string().repeat(64);
    let ids = inputs
        .iter()
        .map(|input| input.asset_id.clone())
        .collect::<Vec<_>>();
    for input in inputs {
        input.culling.as_mut().unwrap().similarity = SimilarityContext {
            group_id: Some(group_id.clone()),
            group_size: ids.len() as u32,
            preferred: input.asset_id == ids[0],
            preferred_assets: vec![ids[0].clone()],
            relative_score: Some(if input.asset_id == ids[0] { 0. } else { 1. }),
            confidence: 0.9,
            bracket_like: bracket,
            kind,
            similarity_score: Some(0.98),
            exact: None,
        };
    }
}

fn context(
    inputs: &[BatchAssetInput],
    kind: PhotoType,
) -> photo_contracts::batch_context::BatchContext {
    build_from_inputs("job-1", kind, inputs, &CancellationToken::default()).unwrap()
}

#[test]
fn burst_and_similar_scene_group_together_but_clear_scenes_remain_separate() {
    let mut burst = vec![
        input(
            "a",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.20,
            0.,
            0.,
            90.,
        ),
        input(
            "b",
            "2026:09:05 10:00:01",
            "55aa55aa55aa55ab",
            0.21,
            0.,
            0.,
            88.,
        ),
    ];
    explicit_group(&mut burst, '1', DuplicateKind::Burst, false);
    burst.push(input(
        "different",
        "2026:09:05 10:20:00",
        "aa55aa55aa55aa55",
        0.20,
        0.,
        0.,
        90.,
    ));
    let result = context(&burst, PhotoType::Portrait);
    assert_eq!(result.scene_groups.len(), 2);
    assert!(result
        .scene_groups
        .iter()
        .any(|group| group.asset_ids == ["a", "b"]));
    assert_eq!(result.sequence_groups.len(), 1);
    assert_eq!(result.sequence_groups[0].kind, SequenceKind::Burst);
}

#[test]
fn adjacent_scenes_can_share_lighting_without_becoming_one_scene() {
    let inputs = vec![
        input(
            "room-a",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.20,
            0.10,
            0.02,
            90.,
        ),
        input(
            "room-b",
            "2026:09:05 10:01:00",
            "aa55aa55aa55aa55",
            0.21,
            0.11,
            0.01,
            90.,
        ),
    ];
    let result = context(&inputs, PhotoType::RealEstate);
    assert_eq!(result.scene_groups.len(), 2);
    assert_eq!(result.lighting_groups.len(), 1);
    assert_eq!(result.lighting_groups[0].asset_ids.len(), 2);
}

#[test]
fn real_estate_exposure_sequence_is_an_explicit_bracket() {
    let mut inputs = vec![
        input(
            "bracket-a",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.08,
            0.,
            0.,
            85.,
        ),
        input(
            "bracket-b",
            "2026:09:05 10:00:01",
            "55aa55aa55aa55ab",
            0.20,
            0.,
            0.,
            90.,
        ),
        input(
            "bracket-c",
            "2026:09:05 10:00:02",
            "55aa55aa55aa55a9",
            0.48,
            0.,
            0.,
            85.,
        ),
    ];
    for input in &mut inputs {
        input.analysis.as_mut().unwrap().photo_type = PhotoType::RealEstate;
        input.analysis.as_mut().unwrap().type_specific =
            TypeAnalysis::RealEstate(RealEstateAnalysis {
                interior_exterior: photo_contracts::analysis::Observation::inferred(
                    "interior".into(),
                    0.7,
                ),
                bright_region_fraction: 0.2,
                shadow_depth: 0.3,
                mixed_lighting: photo_contracts::analysis::Observation::inferred(0.2, 0.7),
                estimated_roll: photo_contracts::analysis::Observation::unavailable("none"),
            });
        let culling = input.culling.as_mut().unwrap();
        culling.photo_type = PhotoType::RealEstate;
        culling.features.as_mut().unwrap().photo_type = PhotoType::RealEstate;
    }
    explicit_group(&mut inputs, '2', DuplicateKind::Burst, true);
    let result = context(&inputs, PhotoType::RealEstate);
    assert_eq!(
        result.sequence_groups[0].kind,
        SequenceKind::ExposureBracket
    );
    assert!(result.asset_contexts.iter().all(|asset| {
        asset
            .consistency_notes
            .iter()
            .any(|note| note.code == ConsistencyNoteCode::BracketMember)
    }));
}

#[test]
fn darker_and_warmer_frames_record_source_relationships_without_edits() {
    let mut inputs = vec![
        input(
            "reference",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.24,
            0.,
            0.,
            92.,
        ),
        input(
            "variant",
            "2026:09:05 10:00:01",
            "55aa55aa55aa55ab",
            0.10,
            0.20,
            0.,
            88.,
        ),
    ];
    explicit_group(&mut inputs, '3', DuplicateKind::NearDuplicate, false);
    let result = context(&inputs, PhotoType::Portrait);
    let variant = result
        .asset_contexts
        .iter()
        .find(|asset| asset.asset_id == "variant")
        .unwrap();
    assert!(variant.exposure_delta_from_group.as_ref().unwrap().delta_ev < -0.3);
    assert!(
        variant
            .wb_delta_from_group
            .as_ref()
            .unwrap()
            .warm_cool_delta
            > 0.08
    );
    let json = result.canonical_json().unwrap();
    assert!(!json.contains("exposure_ev"));
    assert!(!json.contains("temperature"));
}

#[test]
fn weak_references_are_not_forced_and_equivalent_strong_references_are_stable() {
    let mut weak = vec![
        input(
            "weak-a",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.20,
            0.,
            0.,
            60.,
        ),
        input(
            "weak-b",
            "2026:09:05 10:00:01",
            "55aa55aa55aa55ab",
            0.20,
            0.,
            0.,
            55.,
        ),
    ];
    explicit_group(&mut weak, '4', DuplicateKind::NearDuplicate, false);
    assert!(context(&weak, PhotoType::Portrait)
        .reference_candidates
        .is_empty());

    let mut strong = vec![
        input(
            "z-reference",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.20,
            0.,
            0.,
            90.,
        ),
        input(
            "a-reference",
            "2026:09:05 10:00:01",
            "55aa55aa55aa55ab",
            0.20,
            0.,
            0.,
            90.,
        ),
    ];
    explicit_group(&mut strong, '5', DuplicateKind::NearDuplicate, false);
    let result = context(&strong, PhotoType::Portrait);
    let scene = result
        .reference_candidates
        .iter()
        .filter(|candidate| candidate.group_kind == BatchGroupKind::Scene)
        .collect::<Vec<_>>();
    assert_eq!(scene.len(), 2);
    assert_eq!(scene[0].asset_id, "a-reference");
    assert_eq!(scene[0].rank, 1);
    assert_eq!(scene[1].asset_id, "z-reference");
    assert_eq!(scene[1].rank, 2);
}

#[test]
fn selection_and_source_evidence_invalidate_identity_but_recipe_changes_do_not() {
    let first = input(
        "a",
        "2026:09:05 10:00:00",
        "55aa55aa55aa55aa",
        0.2,
        0.,
        0.,
        90.,
    );
    let identity =
        selection_identity("job-1", PhotoType::Portrait, std::slice::from_ref(&first)).unwrap();
    let mut recipe =
        EditRecipe::neutral("recipe-a".into(), "a".into(), "2026-09-05T10:00:00Z".into());
    recipe.global.basic.exposure_ev = 1.2;
    assert_eq!(
        identity,
        selection_identity("job-1", PhotoType::Portrait, std::slice::from_ref(&first)).unwrap()
    );
    let mut changed = first.clone();
    changed.source_fingerprint.push_str("-changed");
    assert_ne!(
        identity,
        selection_identity("job-1", PhotoType::Portrait, &[changed]).unwrap()
    );
    assert_ne!(
        identity,
        selection_identity(
            "job-1",
            PhotoType::Portrait,
            &[
                first,
                input(
                    "b",
                    "2026:09:05 10:00:01",
                    "55aa55aa55aa55ab",
                    0.2,
                    0.,
                    0.,
                    90.
                )
            ]
        )
        .unwrap()
    );
    assert_eq!(recipe.global.basic.exposure_ev, 1.2);
    let reordered = vec![
        input(
            "b",
            "2026:09:05 10:00:01",
            "55aa55aa55aa55ab",
            0.2,
            0.,
            0.,
            90.,
        ),
        input(
            "a",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.2,
            0.,
            0.,
            90.,
        ),
    ];
    let mut sorted = reordered.clone();
    sorted.reverse();
    assert_eq!(
        selection_identity("job-1", PhotoType::Portrait, &reordered).unwrap(),
        selection_identity("job-1", PhotoType::Portrait, &sorted).unwrap()
    );
}

#[test]
fn one_unavailable_analysis_is_nonfatal_and_cancellation_is_honored() {
    let available = input(
        "available",
        "2026:09:05 10:00:00",
        "55aa55aa55aa55aa",
        0.2,
        0.,
        0.,
        90.,
    );
    let unavailable = BatchAssetInput {
        asset_id: "unavailable".into(),
        source_fingerprint: "missing-source".into(),
        analysis: None,
        culling: None,
        unavailable_reason: Some("Current PhotoAnalysis is unavailable".into()),
    };
    let result = context(&[available, unavailable], PhotoType::Portrait);
    assert_eq!(result.diagnostics.available_assets, 1);
    assert_eq!(result.diagnostics.unavailable_assets, 1);
    let missing = result
        .asset_contexts
        .iter()
        .find(|asset| asset.asset_id == "unavailable")
        .unwrap();
    assert_eq!(
        missing.availability,
        photo_contracts::batch_context::ContextAvailability::Unavailable
    );
    assert!(missing.scene_group_id.is_none());

    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert_eq!(
        build_from_inputs("job-1", PhotoType::Portrait, &many(10), &cancelled)
            .unwrap_err()
            .code,
        photo_contracts::ProcessingErrorCode::Cancelled
    );
}

#[test]
fn photo_type_changes_conservative_temporal_boundaries() {
    let inputs = vec![
        input(
            "early",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.2,
            0.,
            0.,
            90.,
        ),
        input(
            "late",
            "2026:09:05 10:20:00",
            "55aa55aa55aa55ab",
            0.2,
            0.,
            0.,
            90.,
        ),
    ];
    assert_eq!(context(&inputs, PhotoType::Portrait).scene_groups.len(), 2);
    assert_eq!(context(&inputs, PhotoType::Landscape).scene_groups.len(), 1);
}

#[test]
fn exact_time_camera_and_orientation_can_link_divergent_raw_jpeg_descriptors() {
    let inputs = vec![
        input(
            "raw",
            "2026:09:05 10:00:00",
            "55aa55aa55aa55aa",
            0.2,
            0.,
            0.,
            90.,
        ),
        input(
            "jpeg",
            "2026:09:05 10:00:00",
            "aa55aa55aa55aa55",
            0.2,
            0.,
            0.,
            90.,
        ),
    ];
    let result = context(&inputs, PhotoType::Portrait);
    assert_eq!(result.scene_groups.len(), 1);
    assert_eq!(result.scene_groups[0].asset_ids, ["jpeg", "raw"]);
    assert!(result.scene_groups[0].confidence > 0.6);
}

fn many(count: usize) -> Vec<BatchAssetInput> {
    (0..count)
        .map(|index| {
            input(
                &format!("asset-{index:04}"),
                &format!("2026:09:05 10:{:02}:{:02}", (index / 60) % 60, index % 60),
                if index % 2 == 0 {
                    "55aa55aa55aa55aa"
                } else {
                    "aa55aa55aa55aa55"
                },
                0.20,
                0.,
                0.,
                90.,
            )
        })
        .collect()
}

#[test]
fn thousand_asset_grouping_has_a_fixed_candidate_bound() {
    let inputs = many(1_000);
    let result = context(&inputs, PhotoType::Portrait);
    assert_eq!(result.asset_contexts.len(), 1_000);
    assert!(result.diagnostics.candidate_comparisons <= 1_000 * 91);
}

#[test]
fn three_thousand_asset_grouping_has_a_fixed_candidate_bound() {
    let inputs = many(3_000);
    let result = context(&inputs, PhotoType::Portrait);
    assert_eq!(result.asset_contexts.len(), 3_000);
    assert!(result.diagnostics.candidate_comparisons <= 3_000 * 91);
    assert!(
        result.canonical_json().unwrap().len()
            < photo_contracts::batch_context::MAX_BATCH_CONTEXT_BYTES
    );
}
