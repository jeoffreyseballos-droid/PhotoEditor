//! Read-only production-service smoke pass over the local portrait corpus.
//! All generated state is placed in an ignored PhotoEditor/.tools directory.
use photo_contracts::{analysis::PhotoType, development::OutputFormat};
use photo_core::{
    analysis::{AnalysisRequest, AnalysisService},
    batch_context::{BatchContextRequest, BatchContextService},
    development::{DevelopmentService, RecipeRenderRequest},
    jobs::JobService,
    models::NewJob,
    rendering::{decode::LibRawDecoder, CpuProcessingEngine, RenderLimits},
    trained_styles::{StyleApplyRequest, TrainedStyleService},
};
use std::{collections::HashMap, error::Error, sync::Arc};

fn main() -> Result<(), Box<dyn Error>> {
    let project = std::env::current_dir()?.canonicalize()?;
    let portraits = project.join("test-photos/Portraits").canonicalize()?;
    if !portraits.starts_with(&project) {
        return Err("acceptance input must remain inside PhotoEditor".into());
    }
    let tools = project.join(".tools");
    std::fs::create_dir_all(&tools)?;
    let scratch = tempfile::Builder::new()
        .prefix("real-trained-style-")
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
        let destination = input.join(relative);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        std::fs::hard_link(entry.path(), destination)?;
    }

    let release = project.join("target/release");
    let cache = scratch.path().join("cache");
    let jobs = JobService::with_exiftool(
        scratch.path().join("data"),
        cache.clone(),
        release.join("exiftool"),
    )?;
    let (job, scan) = jobs.create(NewJob {
        name: "Real trained-style acceptance".into(),
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
        None,
    ));
    let assets = jobs.assets(&job.id, 0, 100)?.items;
    let wanted = [
        "IMG_3804.CR3",
        "IMG_3909.CR3",
        "IMG_4093.JPG",
        "IMG_4161.JPG",
    ];
    let selected = wanted
        .iter()
        .filter_map(|filename| assets.iter().find(|asset| asset.filename == *filename))
        .collect::<Vec<_>>();
    if selected.len() != wanted.len() {
        return Err("the expected portrait acceptance filenames were not found".into());
    }
    let selected_ids = selected
        .iter()
        .map(|asset| asset.id.clone())
        .collect::<Vec<_>>();
    let states = assets
        .iter()
        .map(|asset| (asset.id.clone(), (asset.id.clone(), false)))
        .collect::<HashMap<_, _>>();
    let updates = states
        .into_values()
        .map(|(id, _)| {
            let selected = selected_ids.contains(&id);
            (id, selected)
        })
        .collect::<Vec<_>>();
    jobs.repository.culling_select(&job.id, &updates)?;

    for (index, asset_id) in selected_ids.iter().enumerate() {
        let permit = analysis.reserve(AnalysisRequest {
            job_id: job.id.clone(),
            asset_id: asset_id.clone(),
            photo_type: PhotoType::Portrait,
            request_id: format!("real-style-analysis-{index}"),
        })?;
        analysis.analyze_asset(permit)?;
    }
    let batch = Arc::new(BatchContextService::new(
        jobs.repository.clone(),
        analysis.clone(),
    ));
    let context = batch.run(batch.reserve(BatchContextRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        request_id: "real-style-context".into(),
        force: false,
    })?)?;
    let context = context.context.ok_or("missing current batch context")?;
    let service = TrainedStyleService::new(
        jobs.repository.clone(),
        analysis,
        batch,
        &project.join("styles"),
    )?;
    let permit = service.reserve(StyleApplyRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        style_id: "adaptive-natural-development".into(),
        selected_asset_ids: selected_ids.clone(),
        request_id: "real-style-apply".into(),
    })?;
    let result = service.apply(permit)?;
    let names = selected
        .iter()
        .map(|asset| (asset.id.as_str(), asset.filename.as_str()))
        .collect::<HashMap<_, _>>();
    println!(
        "STYLE_ACCEPTANCE\tselected={}\tpredictions={}\trecipes={}\tfailures={}",
        result.selected_asset_ids.len(),
        result.predictions_succeeded,
        result.recipes_updated,
        result.predictions_failed
    );

    let renderer = DevelopmentService::new(
        jobs.repository.clone(),
        engine,
        cache.join("development"),
        None,
    )?;
    for inference in &result.inferences {
        let summary = inference
            .feature_summary
            .as_ref()
            .ok_or("missing feature summary")?;
        let prediction = inference.prediction.as_ref().ok_or("missing prediction")?;
        let current = jobs.repository.get_recipe(&job.id, &inference.asset_id)?;
        let render_permit =
            renderer.reserve(&format!("real-style-preview-{}", inference.asset_id), true)?;
        let preview = renderer.render_recipe(
            RecipeRenderRequest {
                job_id: job.id.clone(),
                asset_id: inference.asset_id.clone(),
                request_id: format!("real-style-render-{}", inference.asset_id),
                expected_generation: current.generation,
                preview: true,
                output_format: OutputFormat::Jpeg,
                jpeg_quality: 95,
                commit: true,
            },
            render_permit,
        )?;
        println!(
            "PHOTO\t{}\tmedian={:.3}\tgroup_exposure={}\texposure={:+.3}\ttemperature_delta={:+.1}\tconfidence={:?}\tedited_preview={}",
            names.get(inference.asset_id.as_str()).copied().unwrap_or(inference.asset_id.as_str()),
            summary.median_luminance,
            summary.batch_exposure_delta_ev.map(|value| format!("{value:+.3}")).unwrap_or_else(|| "unavailable".into()),
            prediction.adjustments.exposure_ev,
            prediction.adjustments.temperature_delta,
            prediction.confidence,
            preview.preview_data.is_some(),
        );
    }
    println!(
        "CONTEXT\tbatch_id={}\tgroups={}\tavailable={}",
        context.batch_id,
        context.scene_groups.len(),
        context.diagnostics.available_assets
    );
    Ok(())
}
