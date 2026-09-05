use photo_contracts::analysis::*;
fn fixture() -> PhotoAnalysis {
    PhotoAnalysis::parse(include_str!("../../../src/test/analysis-fixture.json")).unwrap()
}
#[test]
fn authoritative_fixture_roundtrips_without_numeric_drift() {
    let a = fixture();
    assert_eq!(
        a,
        PhotoAnalysis::parse(&a.canonical_json().unwrap()).unwrap()
    );
}
#[test]
fn future_missing_unknown_and_oversized_contracts_are_rejected() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["schema_version"] = 2.into();
    assert_eq!(
        PhotoAnalysis::parse(&value.to_string()).unwrap_err(),
        AnalysisError::UnsupportedVersion(2)
    );
    value.as_object_mut().unwrap().remove("schema_version");
    assert!(PhotoAnalysis::parse(&value.to_string()).is_err());
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["recipe_exposure"] = 0.8.into();
    assert!(PhotoAnalysis::parse(&value.to_string()).is_err());
    assert!(PhotoAnalysis::parse(&" ".repeat(MAX_ANALYSIS_BYTES + 1)).is_err());
}
#[test]
fn finite_confidence_and_geometry_are_enforced() {
    let mut a = fixture();
    a.subjects.subject_present = Observation::Available {
        value: true,
        confidence: Some(f64::NAN),
    };
    assert!(a.validate().is_err());
    a = fixture();
    a.common.exposure.mean_luminance = f64::INFINITY;
    assert!(a.validate().is_err());
    a = fixture();
    if let Observation::Available { value, .. } = &mut a.subjects.measurements {
        value.geometry.bbox.width = 1.5;
    }
    assert!(a.validate().is_err());
    a = fixture();
    a.common.scene.low_light_tendency = Observation::inferred(1.2, 0.5);
    assert!(a.validate().is_err());
}
#[test]
fn type_identity_percentiles_and_unavailable_semantics_are_validated() {
    let mut a = fixture();
    a.photo_type = PhotoType::Landscape;
    assert!(a.validate().is_err());
    a = fixture();
    a.common.exposure.percentiles.p01 = 1.;
    assert!(a.validate().is_err());
    a = fixture();
    a.source_fingerprint = "bad".into();
    assert!(a.validate().is_err());
    let json =
        serde_json::to_value(Observation::<f64>::unavailable("No horizon evidence")).unwrap();
    assert!(json.get("value").is_none());
    assert_eq!(json["status"], "unavailable");
}
