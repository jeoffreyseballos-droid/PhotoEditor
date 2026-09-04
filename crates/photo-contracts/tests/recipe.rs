use photo_contracts::*;
fn neutral() -> EditRecipe {
    EditRecipe::neutral(
        "recipe-1".into(),
        "asset-1".into(),
        "2026-09-04T00:00:00Z".into(),
    )
}
fn layer(id: &str, kind: MaskType) -> RecipeLayer {
    RecipeLayer {
        id: id.into(),
        mask_type: kind,
        enabled: true,
        strength: 1.,
        invert: false,
        mask_reference: None,
        confidence: None,
        adjustments: Default::default(),
    }
}
#[test]
fn neutral_roundtrip_canonical_and_missing_fields() {
    let r = neutral();
    assert_eq!(r.adjustments().unwrap(), RenderAdjustments::default());
    let json = r.canonical_json().unwrap();
    assert_eq!(parse_recipe(&json).unwrap(), r);
    assert_eq!(parse_recipe(&json).unwrap().canonical_json().unwrap(), json);
    let sparse = r#"{"schema_version":1,"recipe_id":"recipe-1","asset_id":"asset-1","created_at":"2026-09-04T00:00:00Z","updated_at":"2026-09-04T00:00:00Z"}"#;
    assert_eq!(parse_recipe(sparse).unwrap(), r);
    assert!(json.len() < 5000);
    let mut v = serde_json::to_value(&r).unwrap();
    v["global"]["optics"] = serde_json::json!({});
    v["global"]["detail"] = serde_json::json!({});
    assert_eq!(
        parse_recipe(&v.to_string())
            .unwrap()
            .content_hash()
            .unwrap(),
        r.content_hash().unwrap()
    );
}
#[test]
fn legacy_bridge_upgrades_without_reinterpreting_adjustment_versions() {
    let v = serde_json::json!({"schema_version":0,"recipe_id":"r","asset_id":"a","created_at":"2020-01-01T00:00:00Z","updated_at":"2020-01-01T00:00:00Z","adjustments":{"schema_version":1,"exposure_ev":1.2,"sharpening":20}});
    let r = parse_recipe(&v.to_string()).unwrap();
    assert_eq!(r.schema_version, RECIPE_SCHEMA_VERSION);
    assert_eq!(r.provenance.origin, RecipeOrigin::Migrated);
    assert_eq!(r.global.basic.exposure_ev, 1.2);
    assert_eq!(r.global.detail.legacy_sharpening, 20.);
    assert_eq!(r.global.detail.sharpening.amount, 0.);
}
#[test]
fn required_fields_unknown_fields_and_future_versions_fail_clearly() {
    let v = serde_json::to_value(neutral()).unwrap();
    for key in [
        "schema_version",
        "recipe_id",
        "asset_id",
        "created_at",
        "updated_at",
    ] {
        let mut missing = v.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(parse_recipe(&missing.to_string()).is_err(), "{key}");
    }
    for version in [2, 999] {
        let mut future = v.clone();
        future["schema_version"] = serde_json::json!(version);
        assert_eq!(
            parse_recipe(&future.to_string()).unwrap_err().code,
            RecipeErrorCode::UnsupportedVersion
        );
    }
    let mut unknown = v.clone();
    unknown["overlay"] = serde_json::json!(true);
    assert!(parse_recipe(&unknown.to_string()).is_err());
    let mut bad = neutral();
    bad.schema_version = 0;
    assert!(bad.validated().is_err());
    bad = neutral();
    bad.created_at = "yesterday".into();
    assert!(bad.validated().is_err());
}
#[test]
fn finite_and_bounds_validation_precedes_serialization() {
    let mut r = neutral();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 5.01, -5.01] {
        r.global.basic.exposure_ev = value;
        assert!(r.validated().is_err());
        assert!(r.canonical_json().is_err());
        assert!(r.content_hash().is_err());
    }
    r = neutral();
    r.metadata.confidence = Some(f32::NAN);
    assert!(r.validated().is_err());
    r = neutral();
    r.global.detail.noise.color = f32::INFINITY;
    assert!(r.validated().is_err());
    let base = serde_json::to_value(neutral()).unwrap();
    for path in [
        "/global/basic/temperature",
        "/global/basic/tint",
        "/global/basic/contrast",
        "/global/basic/highlights",
        "/global/basic/shadows",
        "/global/basic/whites",
        "/global/basic/blacks",
        "/global/basic/saturation",
        "/global/basic/vibrance",
        "/global/color_mixer/red/hue",
        "/global/presence/texture",
        "/global/detail/sharpening/radius",
        "/global/detail/noise/luminance_detail",
        "/global/optics/distortion_strength",
        "/global/optics/manual_vignette",
        "/global/effects/vignette/feather",
        "/global/geometry/rotation_degrees",
    ] {
        let mut v = base.clone();
        *v.pointer_mut(path).unwrap() = serde_json::json!(999999);
        assert!(parse_recipe(&v.to_string()).is_err(), "{path}");
    }
}
#[test]
fn malformed_curve_crop_and_layers_are_rejected() {
    let mut r = neutral();
    r.global.curve.red.clear();
    assert!(r.validated().is_err());
    r = neutral();
    r.global.geometry.crop.x = 0.5;
    assert!(r.validated().is_err());
    r = neutral();
    r.global.geometry.crop.width = 0.;
    assert!(r.validated().is_err());
    r = neutral();
    r.local_layers = vec![
        layer("s", MaskType::Subject),
        layer("s", MaskType::Background),
    ];
    assert!(r.validated().is_err());
    r.local_layers.pop();
    r.local_layers[0].strength = 1.1;
    assert!(r.validated().is_err());
    r.local_layers[0].strength = f32::NAN;
    assert!(r.validated().is_err());
    r.local_layers[0].strength = 1.;
    let mut v = serde_json::to_value(&r).unwrap();
    v["local_layers"][0]["mask_type"] = serde_json::json!("sky");
    assert!(parse_recipe(&v.to_string()).is_err());
    v["local_layers"][0]["mask_type"] = serde_json::json!("custom");
    assert!(parse_recipe(&v.to_string()).is_ok()); // reserved legacy kind: unresolved at execution
    v["local_layers"][0]["adjustments"]["crop"] = serde_json::json!({});
    assert!(parse_recipe(&v.to_string()).is_err());
}
#[test]
fn mask_references_are_bounded_content_identities_not_paths_or_pixels() {
    let mut r = neutral();
    r.local_layers.push(layer("s", MaskType::Subject));
    r.local_layers[0].mask_reference = Some(MaskReference {
        content_id: "C:/cache/mask.png".into(),
        source_fingerprint: None,
        model_id: None,
        model_version: None,
        geometry_version: None,
    });
    assert!(r.validated().is_err());
    r.local_layers[0]
        .mask_reference
        .as_mut()
        .unwrap()
        .content_id = "a".repeat(64);
    assert!(r.validated().is_ok());
    let mut v = serde_json::to_value(&r).unwrap();
    v["local_layers"][0]["mask_reference"]["pixels"] = serde_json::json!([0, 1]);
    assert!(parse_recipe(&v.to_string()).is_err());
}
#[test]
fn normalization_makes_equivalent_neutral_forms_identical() {
    let mut a = neutral();
    let mut b = neutral();
    a.global.geometry.rotation_degrees = 540.;
    b.global.geometry.rotation_degrees = -180.;
    a.global.basic.tint = -0.;
    a.global
        .curve
        .master
        .insert(1, CurvePoint { x: 0.5, y: 0.5 });
    assert_eq!(a.canonical_json().unwrap(), b.canonical_json().unwrap());
    assert_eq!(a.content_hash().unwrap(), b.content_hash().unwrap());
}
#[test]
fn hash_excludes_identity_clocks_provenance_confidence_and_disabled_layers() {
    let a = neutral();
    let mut b = a.clone();
    b.recipe_id = "another".into();
    b.asset_id = "other-asset".into();
    b.updated_at = "2026-09-05T00:00:00Z".into();
    b.provenance.origin = RecipeOrigin::Imported;
    b.provenance.model_id = Some("future".into());
    b.metadata.scene_cluster_id = Some("scene".into());
    b.metadata.needs_review = Some(true);
    b.local_layers.push(layer("disabled", MaskType::Subject));
    b.local_layers[0].enabled = false;
    assert_eq!(a.content_hash().unwrap(), b.content_hash().unwrap());
}
#[test]
fn hash_tracks_all_edit_groups_and_order_without_layer_identity_noise() {
    let r = neutral();
    let hash = r.content_hash().unwrap();
    let base = serde_json::to_value(&r).unwrap();
    for (path, value) in [
        ("/global/basic/exposure_ev", serde_json::json!(0.5)),
        (
            "/global/color_mixer/green/saturation",
            serde_json::json!(-10),
        ),
        ("/global/curve/blue/0/y", serde_json::json!(0.01)),
        ("/global/presence/clarity", serde_json::json!(10)),
        ("/global/detail/noise/color", serde_json::json!(20)),
        ("/global/optics/enabled", serde_json::json!(true)),
        ("/global/effects/vignette/amount", serde_json::json!(-10)),
        ("/global/geometry/rotation_degrees", serde_json::json!(1)),
    ] {
        let mut v = base.clone();
        *v.pointer_mut(path).unwrap() = value;
        assert_ne!(
            parse_recipe(&v.to_string())
                .unwrap()
                .content_hash()
                .unwrap(),
            hash,
            "{path}"
        );
    }
    let mut a = r;
    a.local_layers = vec![
        layer("s", MaskType::Subject),
        layer("b", MaskType::Background),
    ];
    a.local_layers[0].adjustments.exposure_ev = 0.3;
    a.local_layers[1].adjustments.contrast = 20.;
    let mut b = a.clone();
    b.local_layers.swap(0, 1);
    assert_ne!(a.content_hash().unwrap(), b.content_hash().unwrap());
    b = a.clone();
    b.local_layers[0].id = "renamed".into();
    b.local_layers[0].confidence = Some(0.7);
    assert_eq!(a.content_hash().unwrap(), b.content_hash().unwrap());
}
#[test]
fn bridge_preserves_every_existing_renderer_field() {
    let mut a = RenderAdjustments {
        exposure_ev: 1.2,
        sharpening: 12.,
        noise_reduction: 13.,
        rotation_degrees: 23.,
        ..Default::default()
    };
    a.curve.red.insert(1, CurvePoint { x: 0.5, y: 0.6 });
    a.hsl[3].saturation = -21.;
    a.presence.dehaze = 3.;
    a.detail.sharpening.masking = 45.;
    a.detail.noise.color = 22.;
    a.optics.enabled = true;
    a.optics.manual_distortion = 5.;
    a.effects.vignette.amount = -14.;
    a.local_layers.push(LocalAdjustmentLayer {
        id: "s".into(),
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
    a.batch_context = Some(BatchContext {
        sequence_id: Some("seq".into()),
        ..Default::default()
    });
    assert_eq!(
        neutral()
            .with_adjustments(&a)
            .unwrap()
            .adjustments()
            .unwrap(),
        a
    );
}
#[test]
fn semantic_diff_covers_controls_local_identity_and_order() {
    let mut a = neutral();
    a.local_layers = vec![
        layer("s", MaskType::Subject),
        layer("b", MaskType::Background),
    ];
    let mut b = a.clone();
    b.global.basic.exposure_ev = 0.8;
    b.global.color_mixer.green.saturation = -14.;
    b.global
        .curve
        .master
        .insert(1, CurvePoint { x: 0.5, y: 0.6 });
    b.global.optics.enabled = true;
    b.local_layers[0].adjustments.exposure_ev = 0.3;
    b.local_layers[1].adjustments.highlights = -10.;
    b.local_layers[0].strength = 0.7;
    b.local_layers[1].enabled = false;
    b.local_layers.swap(0, 1);
    let controls: Vec<_> = diff_recipes(&a, &b)
        .unwrap()
        .into_iter()
        .map(|d| d.control)
        .collect();
    for name in [
        "Exposure (EV)",
        "green / saturation",
        "curve / master",
        "optics / enabled",
        "Subject [s] / adjustments / Exposure",
        "Background [b] / adjustments / highlights",
        "Subject [s] / strength",
        "Background [b] / enabled",
        "Local layer order",
    ] {
        assert!(
            controls.iter().any(|s| s.contains(name)),
            "missing {name}: {controls:?}"
        );
    }
    b = a.clone();
    b.provenance.origin = RecipeOrigin::Imported;
    assert!(diff_recipes(&a, &b).unwrap().is_empty());
}
#[test]
fn templates_instantiate_independent_asset_bound_recipes_without_masks() {
    let mut r = neutral();
    r.local_layers.push(layer("s", MaskType::Subject));
    r.local_layers[0].mask_reference = Some(MaskReference {
        content_id: "a".repeat(64),
        source_fingerprint: None,
        model_id: None,
        model_version: None,
        geometry_version: None,
    });
    let template = RecipeTemplate::from_recipe(&r).unwrap();
    let mut a = template
        .instantiate("a-recipe".into(), "a".into(), r.created_at.clone())
        .unwrap();
    let b = template
        .instantiate("b-recipe".into(), "b".into(), r.created_at)
        .unwrap();
    a.global.basic.exposure_ev = 1.;
    assert_eq!(b.global.basic.exposure_ev, 0.);
    assert!(a.local_layers[0].mask_reference.is_none());
    assert_ne!(a.asset_id, b.asset_id);
}
