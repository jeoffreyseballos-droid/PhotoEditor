use photo_contracts::{
    analysis::PhotoType,
    trained_style::{PredictedCreativeAdjustments, STYLE_FEATURE_SCHEMA_V1},
    training::*,
    EditRecipe, RECIPE_SCHEMA_VERSION,
};
use std::path::PathBuf;

fn pair(dataset_id: &str) -> TrainingPair {
    TrainingPair {
        schema_version: TRAINING_PAIR_SCHEMA_VERSION,
        pair_id: "pair-1".into(),
        dataset_id: dataset_id.into(),
        source_job_id: "job-1".into(),
        source_asset_id: "asset-1".into(),
        source_path: PathBuf::from("source.CR3"),
        reference_path: PathBuf::from("source_EDIT.jpg"),
        photo_type: PhotoType::Portrait,
        source_fingerprint: "a".repeat(64),
        reference_fingerprint: "b".repeat(64),
        validation: PairValidation {
            status: PairValidationStatus::Ready,
            geometry: GeometryRelationship::ExactOrNear,
            structural_similarity: Some(0.92),
            source_width: Some(6000),
            source_height: Some(4000),
            reference_width: Some(3000),
            reference_height: Some(2000),
            diagnostics: vec![],
        },
        source_analysis_id: Some("analysis-1".into()),
        batch_context: None,
        scene_group_id: Some("scene-1".into()),
        target: Some(TargetRecipeResult {
            schema_version: TARGET_RECIPE_SCHEMA_VERSION,
            optimizer_version: TARGET_OPTIMIZER_VERSION.into(),
            cache_identity: "c".repeat(64),
            recipe: EditRecipe::neutral(
                "target-pair-1".into(),
                "asset-1".into(),
                "2026-09-05T00:00:00Z".into(),
            ),
            controls: PredictedCreativeAdjustments {
                exposure_ev: 0.5,
                ..Default::default()
            },
            confidence: TargetFitConfidence::High,
            loss: TargetLossBreakdown {
                total: 0.02,
                luminance: 0.01,
                color_balance: 0.01,
                saturation: 0.0,
                structure: 0.04,
            },
            iterations: 61,
            unsupported_differences: vec![],
            warnings: vec![],
        }),
        split: TrainingSplit::Validation,
        excluded: false,
        feedback: Some(ValidationFeedback::Accept),
        diagnostics: vec![],
    }
}

fn dataset() -> TrainingDataset {
    TrainingDataset {
        schema_version: TRAINING_DATASET_SCHEMA_VERSION,
        dataset_id: "dataset-1".into(),
        job_id: "job-1".into(),
        style_name: "My Portrait Style".into(),
        photo_type: PhotoType::Portrait,
        pairs: vec![pair("dataset-1")],
        created_at: "2026-09-05T00:00:00Z".into(),
        updated_at: "2026-09-05T00:00:00Z".into(),
        dataset_fingerprint: Some("d".repeat(64)),
        feature_schema: STYLE_FEATURE_SCHEMA_V1.into(),
        renderer_version: "cpu-recipe-renderer-v1".into(),
        target_recipe_schema: RECIPE_SCHEMA_VERSION,
        batch_context_id: None,
        warnings: vec![],
        before_files: vec![],
        after_files: vec![],
        alignment: None,
    }
}

#[test]
fn training_dataset_contract_round_trips_and_validates() {
    let dataset = dataset();
    dataset.validate_shape().unwrap();
    let json = serde_json::to_string(&dataset).unwrap();
    let decoded: TrainingDataset = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, dataset);
    assert_eq!(decoded.pairs[0].split, TrainingSplit::Validation);
    assert_eq!(
        decoded.pairs[0]
            .target
            .as_ref()
            .unwrap()
            .recipe
            .schema_version,
        RECIPE_SCHEMA_VERSION
    );
}

#[test]
fn training_contract_rejects_unknown_fields_and_cross_dataset_pairs() {
    let mut value = serde_json::to_value(dataset()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("future_field".into(), true.into());
    assert!(serde_json::from_value::<TrainingDataset>(value).is_err());

    let mut invalid = dataset();
    invalid.pairs[0].dataset_id = "different-dataset".into();
    assert!(invalid.validate_shape().is_err());
}

#[test]
fn training_config_and_target_recipe_versions_are_strict() {
    TrainingConfig::default().validate().unwrap();
    let config = TrainingConfig {
        validation_percent: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());

    let mut invalid = dataset();
    invalid.pairs[0].target.as_mut().unwrap().schema_version += 1;
    assert!(invalid.validate_shape().is_err());
}
