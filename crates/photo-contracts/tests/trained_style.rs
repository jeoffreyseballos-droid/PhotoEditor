use photo_contracts::trained_style::*;

fn manifest() -> TrainedStyle {
    serde_json::from_str(include_str!(
        "../../../styles/adaptive-natural-development/style.json"
    ))
    .unwrap()
}

fn model_fixture() -> StyleModel {
    serde_json::from_str(include_str!(
        "../../../styles/adaptive-natural-development/model.json"
    ))
    .unwrap()
}

#[test]
fn development_manifest_and_linear_model_are_strict_and_versioned() {
    let manifest = manifest();
    manifest.validate().unwrap();
    let model = model_fixture();
    let StyleModelKind::LinearV1(linear) = &model.model;
    let names = linear
        .feature_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    model.validate(&names).unwrap();
    assert_eq!(manifest.feature_schema, STYLE_FEATURE_SCHEMA_V1);
    assert_eq!(linear.outputs.len(), manifest.supported_controls.len());
}

#[test]
fn unsupported_package_and_wrong_feature_schema_fail_safely() {
    let mut manifest = manifest();
    manifest.package_schema_version = 2;
    assert_eq!(manifest.validate(), Err(StyleError::UnsupportedVersion(2)));

    let mut model = model_fixture();
    model.feature_schema = "future_features_v9".into();
    assert_eq!(
        model.validate(&[]),
        Err(StyleError::IncompatibleFeatureSchema(
            "future_features_v9".into()
        ))
    );
}

#[test]
fn nonfinite_or_dimensionally_invalid_model_parameters_are_rejected() {
    let mut model = model_fixture();
    let StyleModelKind::LinearV1(linear) = &mut model.model;
    let names = linear.feature_names.clone();
    linear.outputs[0].intercept = f32::NAN;
    assert!(model
        .validate(&names.iter().map(String::as_str).collect::<Vec<_>>())
        .is_err());

    let mut model = model_fixture();
    let StyleModelKind::LinearV1(linear) = &mut model.model;
    let names = linear.feature_names.clone();
    linear.outputs[0].weights.pop();
    assert!(model
        .validate(&names.iter().map(String::as_str).collect::<Vec<_>>())
        .is_err());
}

#[test]
fn predictions_reject_nonfinite_values_and_invalid_confidence() {
    let prediction = StylePrediction {
        style_id: "style".into(),
        style_version: "1".into(),
        model_version: "1".into(),
        package_identity: "a".repeat(64),
        feature_schema: STYLE_FEATURE_SCHEMA_V1.into(),
        adjustments: PredictedCreativeAdjustments {
            exposure_ev: f32::INFINITY,
            ..Default::default()
        },
        confidence: StyleConfidence::Low,
        confidence_score: 0.5,
        diagnostics: StylePredictionDiagnostics {
            resolver: "test".into(),
            missing_feature_count: 0,
            bounded_controls: vec![],
            warnings: vec![],
        },
    };
    assert!(prediction.validate().is_err());
}
