use photo_contracts::{
    analysis::PhotoAnalysis,
    batch_context::{
        AssetBatchContext, ContextAvailability, ExposureRelationship, WhiteBalanceRelationship,
    },
};
use photo_core::trained_styles::{
    benchmark_predictions, features::build_features, package::load_style_package,
    resolver::LinearStyleResolver,
};
use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let package = load_style_package(&project.join("styles/adaptive-natural-development"))?;
    let mut analysis: PhotoAnalysis =
        serde_json::from_str(include_str!("../../../src/test/analysis-fixture.json"))?;
    analysis.asset_id = "benchmark-asset".into();
    analysis.analysis_id = "benchmark-analysis".into();
    analysis.source_fingerprint = "a".repeat(64);
    let context = AssetBatchContext {
        asset_id: analysis.asset_id.clone(),
        availability: ContextAvailability::Available,
        scene_group_id: Some("scene".into()),
        lighting_group_id: Some("lighting".into()),
        sequence_group_id: None,
        reference_asset_id: None,
        exposure_delta_from_group: Some(ExposureRelationship {
            delta_ev: -0.2,
            confidence: 0.8,
        }),
        wb_delta_from_group: Some(WhiteBalanceRelationship {
            warm_cool_delta: 0.05,
            green_magenta_delta: 0.0,
            confidence: 0.8,
        }),
        group_confidence: 0.85,
        consistency_notes: vec![],
    };
    let features = build_features(&analysis, &context, &"b".repeat(64))?;
    for count in [100_usize, 500, 1_000, 3_000] {
        let micros = benchmark_predictions(&LinearStyleResolver, &package, &features, count)?;
        println!(
            "{count} predictions\t{micros} µs\t{:.3} µs/prediction",
            micros as f64 / count as f64
        );
    }
    Ok(())
}
