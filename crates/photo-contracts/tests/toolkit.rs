use photo_contracts::*;
#[test]
fn legacy_jobs_receive_neutral_toolkit_defaults() {
    let old = r#"{"exposure_ev":1.2,"temperature":6800,"sharpening":12,"crop":{"x":0,"y":0,"width":1,"height":1}}"#;
    let a: RenderAdjustments = serde_json::from_str(old).unwrap();
    let a = a.validated().unwrap();
    assert_eq!(a.schema_version, 2);
    assert_eq!(a.exposure_ev, 1.2);
    assert_eq!(a.sharpening, 12.);
    assert_eq!(a.curve, ToneCurve::default());
    assert!(a.local_layers.is_empty());
    assert_eq!(a.detail, Detail::default());
    assert!(!a.optics.enabled);
}
#[test]
fn curves_round_trip_and_preserve_order() {
    let mut a = RenderAdjustments::default();
    a.curve.master.insert(1, CurvePoint { x: 0.4, y: 0.5 });
    a.curve.blue[0].y = 0.02;
    let s = serde_json::to_string(&a.validated().unwrap()).unwrap();
    let b: RenderAdjustments = serde_json::from_str(&s).unwrap();
    assert_eq!(a, b);
    assert_eq!(s, serde_json::to_string(&b).unwrap());
}
#[test]
fn invalid_curves_and_future_versions_are_rejected() {
    for points in [
        vec![],
        vec![CurvePoint { x: 0., y: 0. }],
        vec![CurvePoint { x: 0., y: 0. }, CurvePoint { x: 0., y: 1. }],
        vec![CurvePoint { x: 0., y: 1. }, CurvePoint { x: 1., y: 0. }],
        vec![
            CurvePoint { x: 0., y: 0. },
            CurvePoint { x: 1., y: f32::NAN },
        ],
    ] {
        let mut a = RenderAdjustments::default();
        a.curve.master = points;
        assert!(a.validated().is_err());
    }
    assert!(RenderAdjustments {
        schema_version: 3,
        ..Default::default()
    }
    .validated()
    .is_err());
}
#[test]
fn detail_presence_optics_and_layers_are_bounded() {
    let base = serde_json::to_value(RenderAdjustments::default()).unwrap();
    for pointer in [
        "/presence/texture",
        "/presence/clarity",
        "/presence/dehaze",
        "/detail/sharpening/amount",
        "/detail/sharpening/radius",
        "/detail/sharpening/detail",
        "/detail/sharpening/masking",
        "/detail/noise/luminance",
        "/detail/noise/luminance_detail",
        "/detail/noise/color",
        "/detail/noise/color_detail",
        "/optics/distortion_strength",
        "/effects/vignette/amount",
    ] {
        let mut json = base.clone();
        *json.pointer_mut(pointer).unwrap() = serde_json::json!(999);
        let a: RenderAdjustments = serde_json::from_value(json).unwrap();
        assert!(a.validated().is_err(), "{pointer}");
    }
    let layer = LocalAdjustmentLayer {
        id: "subject".into(),
        mask_type: MaskType::Subject,
        enabled: true,
        strength: 1.,
        invert: false,
        confidence: None,
        mask_reference: None,
        adjustments: Default::default(),
    };
    let mut a = RenderAdjustments {
        local_layers: vec![layer.clone()],
        ..Default::default()
    };
    assert!(a.validated().is_ok());
    a.local_layers.push(layer);
    assert!(a.validated().is_err());
    a.local_layers.pop();
    a.local_layers[0].strength = 1.01;
    assert!(a.validated().is_err());
    a.local_layers[0].strength = 1.;
    a.local_layers[0].mask_reference = Some("../../original.raw".into());
    assert!(a.validated().is_err());
}
#[test]
fn local_adjustments_cannot_contain_geometry_or_optics() {
    for key in ["crop", "rotation_degrees", "optics", "local_layers"] {
        let mut value = serde_json::to_value(LocalAdjustments::default()).unwrap();
        value[key] = serde_json::json!(null);
        assert!(serde_json::from_value::<LocalAdjustments>(value).is_err());
    }
}
