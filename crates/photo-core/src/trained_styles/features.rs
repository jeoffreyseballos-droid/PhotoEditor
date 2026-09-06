use photo_contracts::{
    analysis::{Observation, PhotoAnalysis, PhotoType},
    batch_context::AssetBatchContext,
    trained_style::{StyleError, StyleFeatureVector, STYLE_FEATURE_SCHEMA_V1},
};

pub const STYLE_FEATURE_NAMES: [&str; 26] = [
    "median_luminance",
    "p05_luminance",
    "p95_luminance",
    "shadow_fraction",
    "highlight_fraction",
    "shadow_clip_fraction",
    "highlight_clip_fraction",
    "dynamic_range_ev_normalized",
    "warm_cool_balance",
    "green_magenta_balance",
    "mean_saturation",
    "edge_strength",
    "blur_likelihood",
    "noise_severity",
    "subject_luminance",
    "background_luminance",
    "subject_background_ev_normalized",
    "backlighting_tendency",
    "mixed_lighting_tendency",
    "batch_exposure_delta_ev",
    "batch_warm_cool_delta",
    "batch_green_magenta_delta",
    "batch_group_confidence",
    "photo_type_portrait",
    "photo_type_real_estate",
    "photo_type_landscape",
];

fn observation(value: &Observation<f64>) -> Option<f64> {
    value.value().copied()
}

/// Stable, UI-independent Phase 7 feature schema. Missing values are represented by
/// an explicit availability bit and a neutral numeric zero.
pub fn build_features(
    analysis: &PhotoAnalysis,
    context: &AssetBatchContext,
    batch_context_id: &str,
) -> Result<StyleFeatureVector, StyleError> {
    analysis
        .validate()
        .map_err(|error| StyleError::InvalidModel(error.to_string()))?;
    if analysis.asset_id != context.asset_id {
        return Err(StyleError::InvalidModel(
            "Analysis and batch context refer to different assets".into(),
        ));
    }
    let subject = analysis.subjects.measurements.value();
    let values = [
        Some(analysis.common.exposure.median_luminance),
        Some(analysis.common.exposure.percentiles.p05),
        Some(analysis.common.exposure.percentiles.p95),
        Some(analysis.common.exposure.shadow_fraction),
        Some(analysis.common.exposure.highlight_fraction),
        Some(analysis.common.exposure.shadow_clip_fraction),
        Some(analysis.common.exposure.highlight_clip_fraction),
        Some((analysis.common.dynamic_range.percentile_ev_span / 12.0).clamp(0.0, 1.0)),
        Some(analysis.common.color.warm_cool_balance),
        Some(analysis.common.color.green_magenta_balance),
        Some(analysis.common.color.mean_saturation),
        Some(analysis.common.detail.edge_strength.clamp(0.0, 1.0)),
        observation(&analysis.common.detail.blur_likelihood),
        analysis
            .common
            .detail
            .noise
            .value()
            .map(|noise| noise.severity),
        subject.map(|measurements| measurements.subject.mean_luminance),
        subject.map(|measurements| measurements.background.mean_luminance),
        subject.map(|measurements| {
            (measurements.subject_background_ev_difference / 4.0).clamp(-1.0, 1.0)
        }),
        observation(&analysis.lighting.backlighting_tendency),
        observation(&analysis.lighting.mixed_lighting_tendency),
        context
            .exposure_delta_from_group
            .as_ref()
            .map(|relationship| relationship.delta_ev.clamp(-5.0, 5.0)),
        context
            .wb_delta_from_group
            .as_ref()
            .map(|relationship| relationship.warm_cool_delta),
        context
            .wb_delta_from_group
            .as_ref()
            .map(|relationship| relationship.green_magenta_delta),
        Some(context.group_confidence),
        Some((analysis.photo_type == PhotoType::Portrait) as u8 as f64),
        Some((analysis.photo_type == PhotoType::RealEstate) as u8 as f64),
        Some((analysis.photo_type == PhotoType::Landscape) as u8 as f64),
    ];
    let available = values.iter().map(Option::is_some).collect::<Vec<_>>();
    let missing_features = STYLE_FEATURE_NAMES
        .iter()
        .zip(&available)
        .filter(|(_, available)| !**available)
        .map(|(name, _)| (*name).to_owned())
        .collect::<Vec<_>>();
    let vector = StyleFeatureVector {
        schema_version: STYLE_FEATURE_SCHEMA_V1.into(),
        asset_id: analysis.asset_id.clone(),
        analysis_id: analysis.analysis_id.clone(),
        batch_context_id: batch_context_id.into(),
        feature_names: STYLE_FEATURE_NAMES
            .iter()
            .map(|name| (*name).into())
            .collect(),
        values: values
            .into_iter()
            .map(|value| value.unwrap_or(0.0) as f32)
            .collect(),
        available,
        missing_features,
    };
    vector.validate(&STYLE_FEATURE_NAMES)?;
    Ok(vector)
}
