use photo_contracts::{analysis::PhotoType, batch_context::*};

fn fixture() -> BatchContext {
    let scene = "1".repeat(64);
    let lighting = "2".repeat(64);
    BatchContext {
        schema_version: BATCH_CONTEXT_SCHEMA_VERSION,
        batch_id: "a".repeat(64),
        job_id: "job-1".into(),
        photo_type: PhotoType::Portrait,
        selected_asset_ids: vec!["asset-1".into()],
        selection_identity: "b".repeat(64),
        created_at: "2026-09-05T12:00:00Z".into(),
        analysis_version: "photo-analysis-schema-1".into(),
        grouping_version: "test-grouping-v1".into(),
        scene_groups: vec![BatchGroup {
            group_id: scene.clone(),
            asset_ids: vec!["asset-1".into()],
            confidence: 0.8,
            reference_candidate_ids: vec!["asset-1".into()],
        }],
        lighting_groups: vec![BatchGroup {
            group_id: lighting.clone(),
            asset_ids: vec!["asset-1".into()],
            confidence: 0.8,
            reference_candidate_ids: vec!["asset-1".into()],
        }],
        sequence_groups: vec![],
        asset_contexts: vec![AssetBatchContext {
            asset_id: "asset-1".into(),
            availability: ContextAvailability::Available,
            scene_group_id: Some(scene.clone()),
            lighting_group_id: Some(lighting.clone()),
            sequence_group_id: None,
            reference_asset_id: Some("asset-1".into()),
            exposure_delta_from_group: Some(ExposureRelationship {
                delta_ev: 0.,
                confidence: 0.8,
            }),
            wb_delta_from_group: Some(WhiteBalanceRelationship {
                warm_cool_delta: 0.,
                green_magenta_delta: 0.,
                confidence: 0.8,
            }),
            group_confidence: 0.8,
            consistency_notes: vec![ConsistencyNote {
                code: ConsistencyNoteCode::ExposureReference,
                message: "Reference".into(),
            }],
        }],
        reference_candidates: vec![
            ReferenceCandidate {
                group_kind: BatchGroupKind::Scene,
                group_id: scene,
                asset_id: "asset-1".into(),
                rank: 1,
                technical_score: 90.,
                confidence: 0.8,
                reasons: vec!["Stable source".into()],
            },
            ReferenceCandidate {
                group_kind: BatchGroupKind::Lighting,
                group_id: lighting,
                asset_id: "asset-1".into(),
                rank: 1,
                technical_score: 90.,
                confidence: 0.8,
                reasons: vec!["Stable source".into()],
            },
        ],
        diagnostics: BatchDiagnostics {
            available_assets: 1,
            partial_assets: 0,
            unavailable_assets: 0,
            candidate_comparisons: 0,
            candidate_limit_per_asset: 64,
            timings: BatchStageTimings::default(),
            warnings: vec![],
        },
    }
}

#[test]
fn versioned_contract_roundtrips_canonically() {
    let context = fixture();
    let json = context.canonical_json().unwrap();
    assert_eq!(BatchContext::parse(&json).unwrap(), context);
    assert!(!json.contains("recipe"));
    assert!(!json.contains("preset"));
}

#[test]
fn unknown_future_and_invalid_relationships_are_rejected() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["schema_version"] = 2.into();
    assert_eq!(
        BatchContext::parse(&value.to_string()).unwrap_err(),
        BatchContextError::UnsupportedVersion(2)
    );
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["edit_recipe"] = serde_json::json!({});
    assert!(BatchContext::parse(&value.to_string()).is_err());
    let mut context = fixture();
    context.asset_contexts[0]
        .exposure_delta_from_group
        .as_mut()
        .unwrap()
        .delta_ev = f64::NAN;
    assert!(context.validate().is_err());
    let mut context = fixture();
    context.selected_asset_ids.push("asset-1".into());
    assert!(context.validate().is_err());
}
