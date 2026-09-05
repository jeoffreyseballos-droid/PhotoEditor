use photo_contracts::{analysis::PhotoType, culling::DuplicateKind};
use photo_core::{
    analysis::AnalysisService,
    batch_context::{BatchContextRequest, BatchContextService},
    culling::{
        features::{UnavailableEyes, YuNetDetector},
        CullingIssue, CullingRequest, CullingService,
    },
    jobs::JobService,
    models::NewJob,
    rendering::{
        decode::LibRawDecoder,
        masks::{MaskCache, ModnetProvider},
        CpuProcessingEngine, RenderLimits,
    },
};
use std::{collections::HashMap, error::Error, path::Path, sync::Arc};

fn print_context(
    label: &str,
    context: &photo_contracts::batch_context::BatchContext,
    names: &HashMap<String, String>,
) {
    let unique_references = context
        .reference_candidates
        .iter()
        .map(|candidate| candidate.asset_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .len();
    println!(
        "{label}\tselected={}\tscenes={}\tlighting={}\tsequences={}\treference_records={}\tunique_references={}\tavailable={}\tpartial={}\tunavailable={}\tcomparisons={}\tloading_ms={}\tgrouping_ms={}\tcontext_ms={}\tpersistence_ms={}\ttotal_ms={}",
        context.selected_asset_ids.len(),
        context.scene_groups.len(),
        context.lighting_groups.len(),
        context.sequence_groups.len(),
        context.reference_candidates.len(),
        unique_references,
        context.diagnostics.available_assets,
        context.diagnostics.partial_assets,
        context.diagnostics.unavailable_assets,
        context.diagnostics.candidate_comparisons,
        context.diagnostics.timings.loading_ms,
        context.diagnostics.timings.grouping_ms,
        context.diagnostics.timings.context_ms,
        context.diagnostics.timings.persistence_ms,
        context.diagnostics.timings.total_ms,
    );
    for group in context
        .scene_groups
        .iter()
        .filter(|group| group.asset_ids.len() > 1)
    {
        println!(
            "SCENE\tconfidence={:.2}\treferences={}\tmembers={}",
            group.confidence,
            group
                .reference_candidate_ids
                .iter()
                .map(|id| names.get(id).map(String::as_str).unwrap_or(id))
                .collect::<Vec<_>>()
                .join(","),
            group
                .asset_ids
                .iter()
                .map(|id| names.get(id).map(String::as_str).unwrap_or(id))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    for group in &context.sequence_groups {
        println!(
            "SEQUENCE\tkind={:?}\tconfidence={:.2}\tmembers={}",
            group.kind,
            group.confidence,
            group
                .asset_ids
                .iter()
                .map(|id| names.get(id).map(String::as_str).unwrap_or(id))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let project = std::env::current_dir()?.canonicalize()?;
    let portraits = project.join("test-photos/Portraits").canonicalize()?;
    if !portraits.starts_with(&project) {
        return Err("acceptance input must remain inside PhotoEditor".into());
    }
    let tools = project.join(".tools");
    std::fs::create_dir_all(&tools)?;
    let scratch = tempfile::Builder::new()
        .prefix("real-batch-context-")
        .tempdir_in(&tools)?;
    let input = scratch.path().join("input");
    let output = scratch.path().join("output");
    std::fs::create_dir_all(&input)?;
    std::fs::create_dir_all(&output)?;
    for entry in walkdir::WalkDir::new(&portraits)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let relative = entry.path().strip_prefix(&portraits)?;
        if relative == Path::new("Duplicates/IMG_4161.CR3") {
            continue;
        }
        let destination = input.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        std::fs::hard_link(entry.path(), destination)?;
    }

    let release = project.join("target/release");
    let toolkit = release.join("toolkit");
    let cache = scratch.path().join("cache");
    let jobs = JobService::with_exiftool(
        scratch.path().join("data"),
        cache.clone(),
        release.join("exiftool"),
    )?;
    let (job, scan) = jobs.create(NewJob {
        name: "Real batch-context acceptance".into(),
        input_path: input,
        output_path: output,
    })?;
    jobs.scan(&job.id, scan)?;
    let engine = Arc::new(CpuProcessingEngine::new(
        Box::new(LibRawDecoder {
            helper: release.join("raw/photo-raw-helper.exe"),
            scratch: cache.join("raw-scratch"),
        }),
        RenderLimits::default(),
    ));
    let analysis = Arc::new(AnalysisService::new(
        jobs.repository.clone(),
        engine.clone(),
        Some(MaskCache::new(
            cache.join("analysis-masks"),
            Box::new(ModnetProvider {
                resources: toolkit.clone(),
                scratch: cache.join("mask-scratch"),
            }),
        )),
    ));
    let culling = CullingService::new(
        jobs.repository.clone(),
        analysis.clone(),
        engine,
        Arc::new(YuNetDetector {
            toolkit,
            scratch: cache.join("face-scratch"),
        }),
        Arc::new(UnavailableEyes),
    );
    culling.run(culling.reserve(CullingRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        request_id: "real-batch-culling".into(),
        force: true,
    })?)?;
    let overview = culling.overview(&job.id, PhotoType::Portrait)?;
    assert_eq!(overview.items.len(), 52);
    let names = overview
        .items
        .iter()
        .map(|item| (item.asset.id.clone(), item.asset.filename.clone()))
        .collect::<HashMap<_, _>>();
    let selected = overview
        .items
        .iter()
        .filter(|item| {
            item.effective_rating
                .is_some_and(|rating| rating.get() == 5)
        })
        .filter(|item| !item.issues.contains(&CullingIssue::Blurry))
        .filter(|item| !item.issues.contains(&CullingIssue::ClosedEyes))
        .filter(|item| {
            let Some(similarity) = &item.similarity else {
                return true;
            };
            if similarity
                .exact
                .as_ref()
                .is_some_and(|exact| exact.canonical_asset_id != item.asset.id)
            {
                return false;
            }
            if similarity.group_id.is_some()
                && matches!(
                    similarity.kind,
                    DuplicateKind::NearDuplicate | DuplicateKind::Burst
                )
            {
                return item.preferred;
            }
            true
        })
        .map(|item| item.asset.id.clone())
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err("current culling filters produced an empty editing selection".into());
    }
    println!(
        "CURRENT_SELECTION\tcount={}\tmembers={}",
        selected.len(),
        selected
            .iter()
            .map(|id| names.get(id).map(String::as_str).unwrap_or(id))
            .collect::<Vec<_>>()
            .join(",")
    );
    culling.select_assets(&job.id, PhotoType::Portrait, &selected)?;
    let batch = BatchContextService::new(jobs.repository.clone(), analysis);
    let primary = batch.run(batch.reserve(BatchContextRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        request_id: "real-selected-context".into(),
        force: false,
    })?)?;
    let primary = primary.context.ok_or("missing selected context")?;
    print_context("SELECTED_CONTEXT", &primary, &names);

    let all = overview
        .items
        .iter()
        .map(|item| item.asset.id.clone())
        .collect::<Vec<_>>();
    culling.select_assets(&job.id, PhotoType::Portrait, &all)?;
    let expanded = batch.run(batch.reserve(BatchContextRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        request_id: "real-expanded-context".into(),
        force: false,
    })?)?;
    let expanded = expanded.context.ok_or("missing expanded context")?;
    print_context("EXPANDED_VALIDATION_CONTEXT", &expanded, &names);
    let known = expanded
        .sequence_groups
        .iter()
        .find(|group| {
            group
                .asset_ids
                .iter()
                .filter_map(|id| names.get(id))
                .filter(|name| {
                    ["IMG_4161", "IMG_4162", "IMG_4163", "IMG_4164", "IMG_4165"]
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
                })
                .count()
                >= 3
        })
        .ok_or("known IMG_4161-4165 sequence was not recognized")?;
    println!(
        "KNOWN_SEQUENCE\tkind={:?}\tmembers={}",
        known.kind,
        known
            .asset_ids
            .iter()
            .map(|id| names.get(id).map(String::as_str).unwrap_or(id))
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(())
}
