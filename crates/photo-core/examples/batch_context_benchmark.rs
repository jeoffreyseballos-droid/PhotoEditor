use photo_contracts::{analysis::PhotoType, culling::DuplicateKind, CancellationToken};
use photo_core::{
    batch_context::{build_from_inputs, BatchAssetInput},
    models::NewJob,
    repository::JobRepository,
};
use std::{error::Error, time::Instant};

fn inputs(count: usize) -> Vec<BatchAssetInput> {
    let analysis = photo_contracts::analysis::PhotoAnalysis::parse(include_str!(
        "../../../src/test/analysis-fixture.json"
    ))
    .unwrap();
    let culling = photo_contracts::culling::CullingAssessment::parse(include_str!(
        "../../../src/test/culling-fixture.json"
    ))
    .unwrap();
    (0..count)
        .map(|index| {
            let id = format!("benchmark-{index:04}");
            let family = index / 8;
            let mut analysis = analysis.clone();
            analysis.asset_id = id.clone();
            analysis.analysis_id = format!("analysis-{id}");
            analysis.source_fingerprint = format!("{index:064x}");
            analysis.common.source.capture_timestamp = Some(format!(
                "2026:09:05 10:{:02}:{:02}",
                (index / 60) % 60,
                index % 60
            ));
            analysis.common.exposure.median_luminance = 0.16 + (family % 3) as f64 * 0.02;
            analysis.common.exposure.percentiles.p50 = analysis.common.exposure.median_luminance;
            analysis.common.color.warm_cool_balance = (family % 4) as f64 * 0.04;

            let mut culling = culling.clone();
            culling.asset_id = id.clone();
            culling.assessment_id = format!("assessment-{id}");
            culling.source_analysis_id = Some(analysis.analysis_id.clone());
            culling.source_fingerprint = analysis.source_fingerprint.clone();
            let features = culling.features.as_mut().unwrap();
            features.asset_id = id.clone();
            features.source_analysis_id = analysis.analysis_id.clone();
            features.source_fingerprint = analysis.source_fingerprint.clone();
            features.descriptor.capture_timestamp =
                analysis.common.source.capture_timestamp.clone();
            features.descriptor.difference_hash = if family % 2 == 0 {
                format!("{:016x}", 0x55aa55aa55aa55aau64 ^ (index % 8) as u64)
            } else {
                format!("{:016x}", 0xaa55aa55aa55aa55u64 ^ (index % 8) as u64)
            };
            culling.similarity.kind = DuplicateKind::Unique;
            BatchAssetInput {
                asset_id: id,
                source_fingerprint: format!("ingestion-{index:04}"),
                analysis: Some(analysis),
                culling: Some(culling),
                unavailable_reason: None,
            }
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let project = std::env::current_dir()?.canonicalize()?;
    let tools = project.join(".tools");
    std::fs::create_dir_all(&tools)?;
    let temporary = tempfile::Builder::new()
        .prefix("batch-context-benchmark-")
        .tempdir_in(&tools)?;
    let repository = JobRepository::open(temporary.path().join("jobs.sqlite3"))?;
    let job = repository.create_job(&NewJob {
        name: "Batch context benchmark".into(),
        input_path: temporary.path().join("input"),
        output_path: temporary.path().join("output"),
    })?;

    println!("assets\tloading_ms\tcandidate_ms\tgrouping_ms\tcontext_ms\tpersistence_ms\ttotal_ms\tcomparisons");
    for count in [100, 500, 1_000, 3_000] {
        let total = Instant::now();
        let loading = Instant::now();
        let inputs = inputs(count);
        let loading_ms = loading.elapsed().as_millis();
        let mut context = build_from_inputs(
            &job.id,
            PhotoType::Portrait,
            &inputs,
            &CancellationToken::default(),
        )?;
        let persistence = Instant::now();
        repository.persist_batch_context(&context)?;
        let persistence_ms = persistence.elapsed().as_millis() as u64;
        context.diagnostics.timings.persistence_ms = persistence_ms;
        let timings = &context.diagnostics.timings;
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            count,
            loading_ms,
            timings.candidate_generation_ms,
            timings.grouping_ms,
            timings.context_ms,
            persistence_ms,
            total.elapsed().as_millis(),
            context.diagnostics.candidate_comparisons,
        );
    }
    Ok(())
}
