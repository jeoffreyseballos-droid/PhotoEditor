use photo_contracts::{
    analysis::PhotoType,
    culling::{DuplicateKind, ReasonCode, Signal},
};
use photo_core::{
    analysis::AnalysisService,
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
use std::{error::Error, path::PathBuf, sync::Arc};

fn signal_value(signal: &Signal<f64>) -> Option<f64> {
    match signal {
        Signal::Available { value, .. } => Some(*value),
        _ => None,
    }
}

fn number(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "-".into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let project = std::env::current_dir()?.canonicalize()?;
    let input = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| project.join("test-photos/Portraits"))
        .canonicalize()?;
    if !input.starts_with(&project) {
        return Err("acceptance input must remain inside the PhotoEditor workspace".into());
    }

    let tools = project.join(".tools");
    std::fs::create_dir_all(&tools)?;
    let scratch = tempfile::Builder::new()
        .prefix("real-culling-acceptance-")
        .tempdir_in(&tools)?;
    let output = scratch.path().join("output");
    std::fs::create_dir_all(&output)?;

    let release = project.join("target/release");
    let toolkit = release.join("toolkit");
    let cache = scratch.path().join("cache");
    let jobs = JobService::with_exiftool(
        scratch.path().join("data"),
        cache.clone(),
        release.join("exiftool"),
    )?;
    let (job, scan) = jobs.create(NewJob {
        name: "Real portrait acceptance".into(),
        input_path: input.clone(),
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
        analysis,
        engine,
        Arc::new(YuNetDetector {
            toolkit,
            scratch: cache.join("face-scratch"),
        }),
        Arc::new(UnavailableEyes),
    );

    let progress = culling.run(culling.reserve(CullingRequest {
        job_id: job.id.clone(),
        photo_type: PhotoType::Portrait,
        request_id: uuid::Uuid::new_v4().to_string(),
        force: true,
    })?)?;
    let overview = culling.overview(&job.id, PhotoType::Portrait)?;
    let mut items = overview.items;
    items.sort_by(|a, b| {
        a.asset
            .filename
            .cmp(&b.asset.filename)
            .then(a.asset.original_path.cmp(&b.asset.original_path))
    });

    println!(
        "SUMMARY\tfiles={}\tstatus={}\tcompleted={}\tfailed={}\tduration_ms={}\thash_bytes={}\thash_cached={}\texact_copies={}\texact_groups={}\tnear_groups={}\tburst_groups={}\tsimilar_groups={}\tunique={}\tunclassified={}",
        items.len(),
        progress.status,
        progress.completed,
        progress.failed,
        progress.duration_ms,
        progress.hash_bytes,
        progress.hash_cached,
        overview.duplicates.exact_copies,
        overview.duplicates.exact_groups,
        overview.duplicates.near_groups,
        overview.duplicates.burst_groups,
        overview.duplicates.similar_groups,
        overview.duplicates.unique_images,
        overview.duplicates.unclassified_images,
    );
    println!(
        "RATINGS\t5={}\t4={}\t3={}\t2={}\t1={}\tunrated={}",
        overview.counts[5],
        overview.counts[4],
        overview.counts[3],
        overview.counts[2],
        overview.counts[1],
        overview.counts[0],
    );

    let shown_without_duplicates = items
        .iter()
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
        .count();
    let blurry = items
        .iter()
        .filter(|item| item.issues.contains(&CullingIssue::Blurry))
        .count();
    println!(
        "FILTERS\tduplicates_hide_shows={}\thide_blurry_shows={}\tblurry={}",
        shown_without_duplicates,
        items.len() - blurry,
        blurry,
    );
    println!(
        "ITEM\trating\trelationship\tpreferred\tissues\tglobal\tsubject\tface_min\tfilename\tpath"
    );

    for item in items {
        let detail = culling.detail(&job.id, &item.asset.id, PhotoType::Portrait)?;
        let assessment = detail.assessment.as_ref();
        let features = assessment.and_then(|assessment| assessment.features.as_ref());
        let face_min = features.and_then(|features| {
            let Signal::Available { value: faces, .. } = &features.people.faces else {
                return None;
            };
            faces
                .iter()
                .filter(|face| face.relevant)
                .filter_map(|face| signal_value(&face.sharpness))
                .reduce(f64::min)
        });
        let global = features.map(|features| features.technical.global_sharpness);
        let subject =
            features.and_then(|features| signal_value(&features.technical.subject_sharpness));
        let severe = assessment.is_some_and(|assessment| {
            assessment
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::SevereSubjectSoftness)
        });
        let issues = if item.issues.is_empty() {
            "-".into()
        } else {
            format!("{:?}", item.issues)
        };
        let relationship = item
            .relationship_kind
            .map(|kind| format!("{kind:?}"))
            .unwrap_or_else(|| "-".into());
        let rating = item
            .effective_rating
            .map(|rating| rating.get().to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "ITEM\t{rating}\t{relationship}\t{}\t{}{}\t{}\t{}\t{}\t{}\t{}",
            item.preferred,
            issues,
            if severe { "+SEVERE" } else { "" },
            number(global),
            number(subject),
            number(face_min),
            item.asset.filename,
            item.asset.original_path.display(),
        );
    }
    Ok(())
}
