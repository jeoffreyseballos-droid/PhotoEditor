use photo_contracts::culling::*;
fn fixture() -> CullingAssessment {
    CullingAssessment::parse(include_str!("../../../src/test/culling-fixture.json")).unwrap()
}
#[test]
fn structured_fixture_roundtrip_and_star_domain() {
    let a = fixture();
    assert_eq!(
        a,
        CullingAssessment::parse(&a.canonical_json().unwrap()).unwrap()
    );
    for n in 1..=5 {
        assert_eq!(Stars::new(n).unwrap().get(), n);
    }
    for n in [0, 6, 255] {
        assert!(Stars::new(n).is_err());
        assert!(serde_json::from_str::<Stars>(&n.to_string()).is_err());
    }
}
#[test]
fn missing_future_unknown_oversized_contracts_rejected() {
    let mut v = serde_json::to_value(fixture()).unwrap();
    v["schema_version"] = 3.into();
    assert_eq!(
        CullingAssessment::parse(&v.to_string()).unwrap_err(),
        CullingError::UnsupportedVersion(3)
    );
    v.as_object_mut().unwrap().remove("schema_version");
    assert!(CullingAssessment::parse(&v.to_string()).is_err());
    let mut v = serde_json::to_value(fixture()).unwrap();
    v["recipe"] = true.into();
    assert!(CullingAssessment::parse(&v.to_string()).is_err());
    assert!(CullingAssessment::parse(&" ".repeat(MAX_CULLING_BYTES + 1)).is_err());
}
#[test]
fn invalid_finite_confidence_geometry_binding_and_subject_rejected() {
    let mut a = fixture();
    a.confidence = f64::NAN;
    assert!(a.validate().is_err());
    a = fixture();
    a.reasons[0].confidence = 1.1;
    assert!(a.validate().is_err());
    a = fixture();
    a.reasons[0].subject_index = Some(99);
    assert!(a.validate().is_err());
    a = fixture();
    a.features.as_mut().unwrap().asset_id = "different".into();
    assert!(a.validate().is_err());
    a = fixture();
    a.similarity.relative_score = Some(f64::NAN);
    assert!(a.validate().is_err());
    a = fixture();
    if let Signal::Available { value, .. } = &mut a.features.as_mut().unwrap().people.faces {
        value[0].bbox.width = 1.5;
    }
    assert!(a.validate().is_err());
}
#[test]
fn missing_evidence_never_becomes_zero_stars_or_closed_eyes() {
    let mut a = fixture();
    a.features = None;
    assert!(a.validate().is_err());
    let s = Signal::<EyeState>::unavailable("no model");
    assert!(s.value().is_none());
    assert_eq!(
        CullingState::effective(Stars::new(2).ok(), Stars::new(5).ok()),
        Stars::new(5).ok()
    );
    assert_eq!(
        CullingState::effective(Stars::new(1).ok(), Stars::new(5).ok()),
        Stars::new(5).ok()
    );
    assert_eq!(
        CullingState::effective(Stars::new(1).ok(), None),
        Stars::new(1).ok()
    );
}
#[test]
fn legacy_snapshots_upgrade_without_claiming_duplicate_identity() {
    let mut v = serde_json::to_value(fixture()).unwrap();
    v["schema_version"] = 1.into();
    for k in ["duplicate_content", "duplicate_stamp", "membership_key"] {
        v.as_object_mut().unwrap().remove(k);
    }
    for k in ["kind", "similarity_score", "exact"] {
        v["similarity"].as_object_mut().unwrap().remove(k);
    }
    let legacy = v.to_string();
    let a = CullingAssessment::parse(&legacy).unwrap();
    assert_eq!(a.schema_version, 2);
    assert!(a.duplicate_content.is_none());
    assert!(a.membership_key.is_none());
    v["similarity"]["group_id"] = "a".repeat(64).into();
    v["similarity"]["group_size"] = 2.into();
    v["similarity"]["preferred"] = true.into();
    v["similarity"]["preferred_assets"] = serde_json::json!(["photo-1"]);
    v["similarity"]["relative_score"] = 0.into();
    let a = CullingAssessment::parse(&v.to_string()).unwrap();
    assert_eq!(a.similarity.kind, DuplicateKind::Similar);
    assert!(a.similarity.exact.is_none());
    for malformed in [
        serde_json::Value::Null,
        serde_json::json!("invalid"),
        serde_json::json!([]),
    ] {
        v["similarity"] = malformed;
        assert!(CullingAssessment::parse(&v.to_string()).is_err());
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&legacy).unwrap()["schema_version"],
        1
    );
}
#[test]
fn duplicate_context_validation_and_exact_only_unprocessable_rating() {
    let mut a = fixture();
    a.similarity.exact = Some(ExactDuplicateRelationship {
        group_id: "1".repeat(64),
        group_size: 2,
        canonical_asset_id: "other".into(),
        content: a.duplicate_content.clone().unwrap(),
    });
    a.ai_rating = Stars::new(1).ok();
    a.final_score = 5.;
    a.reasons = vec![CullingReason {
        code: ReasonCode::ExactDuplicate,
        severity: Severity::Major,
        confidence: 1.,
        subject_index: None,
        measurement: None,
    }];
    a.features = None;
    a.source_analysis_id = None;
    a.validate().unwrap();
    let exact = a.clone();
    a.duplicate_content.as_mut().unwrap().sha256 = "0".repeat(64);
    assert!(a.validate().is_err());
    a = exact.clone();
    a.similarity.exact.as_mut().unwrap().group_size = 1;
    assert!(a.validate().is_err());
    a = exact.clone();
    a.similarity.exact.as_mut().unwrap().canonical_asset_id = a.asset_id.clone();
    assert!(a.validate().is_err());
    a = exact.clone();
    a.ai_rating = Stars::new(5).ok();
    assert!(a.validate().is_err());
    a = exact;
    a.similarity.kind = DuplicateKind::NearDuplicate;
    assert!(a.validate().is_err());
    let mut v = serde_json::to_value(fixture()).unwrap();
    v["similarity"].as_object_mut().unwrap().remove("kind");
    assert!(CullingAssessment::parse(&v.to_string()).is_err());
}
