//! Cached, bounded source relationships for the explicit editing selection.
mod storage;

use crate::{
    analysis::{AnalysisService, AnalysisStatus},
    culling::{self, similarity},
    rendering::internal,
    repository::JobRepository,
};
use photo_contracts::{
    analysis::{
        Observation, PhotoAnalysis, PhotoType, TypeAnalysis, PHOTO_ANALYSIS_SCHEMA_VERSION,
    },
    batch_context::*,
    culling::{CullingAssessment, DuplicateKind, ReasonCode},
    CancellationToken, ProcessingError, ProcessingErrorCode, ProcessingResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

pub const GROUPING_VERSION: &str = "batch-context-bounded-anchor-v1";
pub const CANDIDATE_LIMIT: usize = 64;

fn grouping_version() -> String {
    format!(
        "{GROUPING_VERSION};{};{}",
        culling::features::FEATURE_VERSION,
        culling::similarity::SIMILARITY_VERSION
    )
}

fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

#[derive(Clone, Debug)]
pub struct BatchAssetInput {
    pub asset_id: String,
    /// Current ingestion fingerprint; source changes alter the selection identity.
    pub source_fingerprint: String,
    pub analysis: Option<PhotoAnalysis>,
    pub culling: Option<CullingAssessment>,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchContextRequest {
    pub job_id: String,
    pub photo_type: PhotoType,
    pub request_id: String,
    pub force: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchContextProgress {
    pub job_id: String,
    pub request_id: String,
    pub photo_type: PhotoType,
    pub status: String,
    pub stage: String,
    pub completed: u32,
    pub total: u32,
    pub cached: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchContextState {
    pub selected_count: u32,
    pub selection_identity: Option<String>,
    pub context: Option<BatchContext>,
    pub cached: bool,
    pub stale: bool,
    pub progress: Option<BatchContextProgress>,
}

struct Active {
    request: BatchContextRequest,
    token: CancellationToken,
}
type ActiveSlot = Arc<Mutex<Option<Active>>>;

pub struct BatchContextPermit {
    request: BatchContextRequest,
    token: CancellationToken,
    active: ActiveSlot,
    repository: JobRepository,
}

impl Drop for BatchContextPermit {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            if active
                .as_ref()
                .is_some_and(|entry| entry.request.request_id == self.request.request_id)
            {
                if let Ok(Some(mut progress)) = self
                    .repository
                    .batch_context_progress(&self.request.job_id, self.request.photo_type)
                {
                    if matches!(progress.status.as_str(), "queued" | "running") {
                        progress.status = "cancelled".into();
                        progress.stage = "Stopped; cached context remains available".into();
                        let _ = self.repository.save_batch_context_progress(&progress);
                    }
                }
                *active = None;
            }
        }
    }
}

pub struct BatchContextService {
    repository: JobRepository,
    analysis: Arc<AnalysisService>,
    active: ActiveSlot,
}

impl BatchContextService {
    pub fn new(repository: JobRepository, analysis: Arc<AnalysisService>) -> Self {
        Self {
            repository,
            analysis,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub fn reserve(&self, request: BatchContextRequest) -> ProcessingResult<BatchContextPermit> {
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err(internal("Invalid batch-context request ID"));
        }
        let selected = self
            .repository
            .selected_editing_asset_ids(&request.job_id)?;
        if selected.is_empty() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Select at least one photograph before building batch context",
            ));
        }
        let mut active = self.active.lock().map_err(internal)?;
        if active.is_some() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "One batch-context task is already active",
            ));
        }
        let token = CancellationToken::default();
        let progress = BatchContextProgress {
            job_id: request.job_id.clone(),
            request_id: request.request_id.clone(),
            photo_type: request.photo_type,
            status: "queued".into(),
            stage: "Queued".into(),
            completed: 0,
            total: selected.len() as u32,
            cached: false,
            duration_ms: 0,
            error: None,
        };
        self.repository.save_batch_context_progress(&progress)?;
        *active = Some(Active {
            request: request.clone(),
            token: token.clone(),
        });
        Ok(BatchContextPermit {
            request,
            token,
            active: self.active.clone(),
            repository: self.repository.clone(),
        })
    }

    pub fn cancel(&self, request_id: &str) -> ProcessingResult<()> {
        let active = self.active.lock().map_err(internal)?;
        if let Some(entry) = active.as_ref() {
            if entry.request.request_id == request_id {
                entry.token.cancel();
            }
        }
        Ok(())
    }

    pub fn progress(
        &self,
        job: &str,
        kind: PhotoType,
    ) -> ProcessingResult<Option<BatchContextProgress>> {
        self.repository.batch_context_progress(job, kind)
    }

    pub fn state(&self, job: &str, kind: PhotoType) -> ProcessingResult<BatchContextState> {
        let selected = self.repository.selected_editing_asset_ids(job)?;
        let progress = self.progress(job, kind)?;
        if selected.is_empty() {
            return Ok(BatchContextState {
                selected_count: 0,
                selection_identity: None,
                context: None,
                cached: false,
                stale: self.repository.has_other_batch_context(job, kind, None)?,
                progress,
            });
        }
        let inputs = self.load_inputs(job, kind, &selected, None, &CancellationToken::default())?;
        let identity = selection_identity(job, kind, &inputs)?;
        let context = self.repository.batch_context(job, kind, &identity)?;
        let stale = context.is_none()
            && self
                .repository
                .has_other_batch_context(job, kind, Some(&identity))?;
        Ok(BatchContextState {
            selected_count: selected.len() as u32,
            selection_identity: Some(identity),
            cached: context.is_some(),
            context,
            stale,
            progress,
        })
    }

    pub fn run(&self, permit: BatchContextPermit) -> ProcessingResult<BatchContextState> {
        if !Arc::ptr_eq(&self.active, &permit.active) {
            return Err(internal(
                "Batch-context permit belongs to a different service",
            ));
        }
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run_inner(&permit)))
                .unwrap_or_else(|_| Err(internal("Batch-context worker stopped unexpectedly")));
        if let Err(error) = &result {
            if let Ok(Some(mut progress)) =
                self.progress(&permit.request.job_id, permit.request.photo_type)
            {
                progress.status = if error.code == ProcessingErrorCode::Cancelled {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                progress.stage = if error.code == ProcessingErrorCode::Cancelled {
                    "Stopped; cached context remains available"
                } else {
                    "Batch context failed; source photos and edits are unchanged"
                }
                .into();
                progress.error = Some(error.message.clone());
                let _ = self.repository.save_batch_context_progress(&progress);
            }
        }
        result
    }

    fn run_inner(&self, permit: &BatchContextPermit) -> ProcessingResult<BatchContextState> {
        let started = Instant::now();
        let request = &permit.request;
        let selected = self
            .repository
            .selected_editing_asset_ids(&request.job_id)?;
        let mut progress = BatchContextProgress {
            job_id: request.job_id.clone(),
            request_id: request.request_id.clone(),
            photo_type: request.photo_type,
            status: "running".into(),
            stage: "Loading existing source analysis".into(),
            completed: 0,
            total: selected.len() as u32,
            cached: false,
            duration_ms: 0,
            error: None,
        };
        self.repository.save_batch_context_progress(&progress)?;
        let loading = Instant::now();
        let inputs = self.load_inputs(
            &request.job_id,
            request.photo_type,
            &selected,
            Some(&mut progress),
            &permit.token,
        )?;
        let loading_ms = loading.elapsed().as_millis() as u64;
        let identity = selection_identity(&request.job_id, request.photo_type, &inputs)?;
        if !request.force {
            if let Some(context) =
                self.repository
                    .batch_context(&request.job_id, request.photo_type, &identity)?
            {
                progress.status = "complete".into();
                progress.stage = "Complete; current batch context reused".into();
                progress.completed = progress.total;
                progress.cached = true;
                progress.duration_ms = started.elapsed().as_millis() as u64;
                self.repository.save_batch_context_progress(&progress)?;
                return Ok(BatchContextState {
                    selected_count: selected.len() as u32,
                    selection_identity: Some(identity),
                    context: Some(context),
                    cached: true,
                    stale: false,
                    progress: Some(progress),
                });
            }
        }
        permit.token.check()?;
        progress.stage = "Generating bounded candidates".into();
        progress.completed = progress.total;
        self.repository.save_batch_context_progress(&progress)?;
        let mut context =
            build_from_inputs(&request.job_id, request.photo_type, &inputs, &permit.token)?;
        context.diagnostics.timings.loading_ms = loading_ms;
        context.diagnostics.timings.total_ms = started.elapsed().as_millis() as u64;
        progress.stage = "Persisting batch context".into();
        self.repository.save_batch_context_progress(&progress)?;
        let persistence = Instant::now();
        self.repository.persist_batch_context(&context)?;
        context.diagnostics.timings.persistence_ms = persistence.elapsed().as_millis() as u64;
        context.diagnostics.timings.total_ms = started.elapsed().as_millis() as u64;
        self.repository.persist_batch_context(&context)?;
        progress.status = "complete".into();
        progress.stage = "Complete".into();
        progress.completed = progress.total;
        progress.duration_ms = started.elapsed().as_millis() as u64;
        self.repository.save_batch_context_progress(&progress)?;
        Ok(BatchContextState {
            selected_count: selected.len() as u32,
            selection_identity: Some(identity),
            context: Some(context),
            cached: false,
            stale: false,
            progress: Some(progress),
        })
    }

    fn load_inputs(
        &self,
        job: &str,
        kind: PhotoType,
        selected: &[String],
        mut progress: Option<&mut BatchContextProgress>,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Vec<BatchAssetInput>> {
        let mut inputs = Vec::with_capacity(selected.len());
        for (index, asset_id) in selected.iter().enumerate() {
            cancel.check()?;
            let asset = self.repository.asset(job, asset_id).map_err(internal)?;
            let (analysis, mut unavailable_reason) =
                match self.analysis.get_analysis(job, asset_id, kind) {
                    Ok(state) => (
                        state.analysis,
                        state.error.or_else(|| {
                            (!matches!(
                                state.status,
                                AnalysisStatus::Complete | AnalysisStatus::Warning
                            ))
                            .then(|| format!("Current analysis status is {:?}", state.status))
                        }),
                    ),
                    Err(error) => (None, Some(error.message)),
                };
            let culling = match self.repository.culling_state(job, asset_id, kind) {
                Ok(state) => state.assessment.filter(|assessment| {
                    analysis.as_ref().is_some_and(|analysis| {
                        assessment.source_analysis_id.as_deref() == Some(&analysis.analysis_id)
                            && assessment.source_fingerprint == analysis.source_fingerprint
                    })
                }),
                Err(error) => {
                    unavailable_reason.get_or_insert(error.message);
                    None
                }
            };
            inputs.push(BatchAssetInput {
                asset_id: asset.id,
                source_fingerprint: asset.fingerprint,
                analysis,
                culling,
                unavailable_reason,
            });
            if let Some(current) = progress.as_deref_mut() {
                current.completed = (index + 1) as u32;
                if index + 1 == selected.len() || index % 16 == 15 {
                    self.repository.save_batch_context_progress(current)?;
                }
            }
        }
        Ok(inputs)
    }
}

pub fn selection_identity(
    job: &str,
    kind: PhotoType,
    inputs: &[BatchAssetInput],
) -> ProcessingResult<String> {
    if inputs.is_empty() || inputs.len() > MAX_BATCH_CONTEXT_ASSETS {
        return Err(internal(
            "Batch selection must contain 1 through 5,000 assets",
        ));
    }
    let mut ordered = inputs.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));
    if ordered
        .windows(2)
        .any(|pair| pair[0].asset_id == pair[1].asset_id)
    {
        return Err(internal("Batch selection contains duplicate asset IDs"));
    }
    let evidence = ordered
        .into_iter()
        .map(|input| {
            format!(
                "{}:{}:{}:{}",
                input.asset_id,
                input.source_fingerprint,
                input
                    .analysis
                    .as_ref()
                    .map(|analysis| analysis.analysis_id.as_str())
                    .unwrap_or("analysis-unavailable"),
                input
                    .culling
                    .as_ref()
                    .map(|assessment| assessment.assessment_id.as_str())
                    .unwrap_or("culling-unavailable")
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    Ok(digest(&[
        "batch-context-selection-v1",
        job,
        kind.as_str(),
        &PHOTO_ANALYSIS_SCHEMA_VERSION.to_string(),
        &grouping_version(),
        &evidence,
    ]))
}

#[derive(Clone)]
struct Prepared<'a> {
    input: &'a BatchAssetInput,
    timestamp: Option<i64>,
    exposure_ev: f64,
    warm: f64,
    tint: f64,
    dynamic_range: f64,
    saturation: f64,
    subject_light: Option<f64>,
    mixed_light: Option<f64>,
}

fn timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp_millis())
        .ok()
        .or_else(|| {
            ["%Y:%m:%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"]
                .iter()
                .find_map(|format| {
                    chrono::NaiveDateTime::parse_from_str(value, format)
                        .ok()
                        .map(|time| time.and_utc().timestamp_millis())
                })
        })
}

fn observed(signal: &Observation<f64>) -> Option<f64> {
    signal.value().copied()
}

fn prepare(input: &BatchAssetInput) -> Option<Prepared<'_>> {
    let analysis = input.analysis.as_ref()?;
    let median = analysis.common.exposure.median_luminance;
    let mixed_light = match &analysis.type_specific {
        TypeAnalysis::RealEstate(value) => observed(&value.mixed_lighting),
        _ => observed(&analysis.lighting.mixed_lighting_tendency),
    };
    Some(Prepared {
        input,
        timestamp: analysis
            .common
            .source
            .capture_timestamp
            .as_deref()
            .and_then(timestamp),
        exposure_ev: (median + 0.005).log2(),
        warm: analysis.common.color.warm_cool_balance,
        tint: analysis.common.color.green_magenta_balance,
        dynamic_range: analysis.common.dynamic_range.percentile_range,
        saturation: analysis.common.color.mean_saturation,
        subject_light: observed(&analysis.lighting.subject_light_level),
        mixed_light,
    })
}

fn scene_limits(kind: PhotoType) -> (i64, f64) {
    match kind {
        PhotoType::Portrait => (3 * 60 * 1000, 0.94),
        PhotoType::RealEstate => (15 * 60 * 1000, 0.95),
        PhotoType::Landscape => (30 * 60 * 1000, 0.96),
    }
}

fn explicit_visual_group(input: &BatchAssetInput) -> Option<(&str, DuplicateKind, f64)> {
    let similarity = &input.culling.as_ref()?.similarity;
    let id = similarity.group_id.as_deref()?;
    Some((id, similarity.kind, similarity.confidence))
}

fn metadata_scene_confidence(left: &Prepared<'_>, right: &Prepared<'_>) -> Option<f64> {
    let left_analysis = left.input.analysis.as_ref()?;
    let right_analysis = right.input.analysis.as_ref()?;
    let gap = left
        .timestamp
        .zip(right.timestamp)
        .map(|(left, right)| (left - right).abs())?;
    if gap > 2_000
        || (left_analysis.common.composition.aspect_ratio
            / right_analysis.common.composition.aspect_ratio
            - 1.)
            .abs()
            > 0.03
        || left_analysis.common.composition.orientation
            != right_analysis.common.composition.orientation
    {
        return None;
    }
    let left_source = &left_analysis.common.source;
    let right_source = &right_analysis.common.source;
    let same_camera = left_source
        .camera_model
        .as_ref()
        .zip(right_source.camera_model.as_ref())
        .is_some_and(|(left, right)| left == right)
        || left_source
            .camera_make
            .as_ref()
            .zip(right_source.camera_make.as_ref())
            .is_some_and(|(left, right)| left == right)
        || left
            .input
            .culling
            .as_ref()
            .and_then(|assessment| assessment.features.as_ref())
            .and_then(|features| features.descriptor.camera.as_ref())
            .zip(
                right
                    .input
                    .culling
                    .as_ref()
                    .and_then(|assessment| assessment.features.as_ref())
                    .and_then(|features| features.descriptor.camera.as_ref()),
            )
            .is_some_and(|(left, right)| left == right);
    let compatible_lens = left_source
        .lens
        .as_ref()
        .zip(right_source.lens.as_ref())
        .is_none_or(|(left, right)| left == right);
    (same_camera && compatible_lens).then_some(0.68)
}

fn scene_groups(
    prepared: &[Prepared<'_>],
    kind: PhotoType,
    cancel: &CancellationToken,
) -> ProcessingResult<(Vec<Vec<usize>>, Vec<f64>, u64)> {
    let mut order = (0..prepared.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        prepared[left]
            .timestamp
            .cmp(&prepared[right].timestamp)
            .then_with(|| {
                prepared[left]
                    .input
                    .asset_id
                    .cmp(&prepared[right].input.asset_id)
            })
    });
    let (time_limit, score_limit) = scene_limits(kind);
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut confidence: Vec<f64> = Vec::new();
    let mut explicit = HashMap::<String, usize>::new();
    let mut comparisons = 0u64;
    for index in order {
        cancel.check()?;
        let current = &prepared[index];
        let direct = explicit_visual_group(current.input).and_then(|(id, relation, evidence)| {
            matches!(
                relation,
                DuplicateKind::NearDuplicate | DuplicateKind::Burst
            )
            .then(|| explicit.get(id).copied().map(|group| (group, evidence)))
            .flatten()
        });
        let candidate = direct.or_else(|| {
            let descriptor = current
                .input
                .culling
                .as_ref()
                .and_then(|assessment| assessment.features.as_ref())
                .map(|features| &features.descriptor);
            groups
                .iter()
                .enumerate()
                .rev()
                .take(CANDIDATE_LIMIT)
                .filter_map(|(group_index, members)| {
                    comparisons += 1;
                    let anchor = &prepared[members[0]];
                    let anchor_descriptor = anchor
                        .input
                        .culling
                        .as_ref()
                        .and_then(|assessment| assessment.features.as_ref())
                        .map(|features| &features.descriptor);
                    let gap = anchor
                        .timestamp
                        .zip(current.timestamp)
                        .map(|(left, right)| (left - right).abs());
                    let visual = anchor_descriptor
                        .zip(descriptor)
                        .and_then(|(anchor, current)| similarity::classify(anchor, current));
                    if let Some(visual) = visual {
                        let time_ok = gap.is_some_and(|gap| gap <= time_limit)
                            || (gap.is_none()
                                && visual.kind == DuplicateKind::NearDuplicate
                                && visual.score >= 0.985);
                        if time_ok && visual.score >= score_limit {
                            return Some((group_index, visual.confidence * visual.score));
                        }
                    }
                    metadata_scene_confidence(anchor, current)
                        .map(|confidence| (group_index, confidence))
                })
                .max_by(|left, right| left.1.total_cmp(&right.1))
        });
        let group_index = if let Some((group_index, link_confidence)) = candidate {
            groups[group_index].push(index);
            confidence[group_index] = confidence[group_index].min(link_confidence.clamp(0., 1.));
            group_index
        } else {
            groups.push(vec![index]);
            confidence.push(1.);
            groups.len() - 1
        };
        if let Some((id, relation, _)) = explicit_visual_group(current.input) {
            if matches!(
                relation,
                DuplicateKind::NearDuplicate | DuplicateKind::Burst
            ) {
                explicit.entry(id.into()).or_insert(group_index);
            }
        }
    }
    for (group, confidence) in groups.iter().zip(&mut confidence) {
        if group.len() == 1 {
            *confidence = 0.55;
        }
    }
    Ok((groups, confidence, comparisons))
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct LightKey(i32, i32, i32);

fn light_key(asset: &Prepared<'_>) -> LightKey {
    LightKey(
        (asset.warm / 0.25).round() as i32,
        (asset.tint / 0.25).round() as i32,
        (asset.dynamic_range / 0.30).round() as i32,
    )
}

fn lighting_distance(left: &Prepared<'_>, right: &Prepared<'_>, kind: PhotoType) -> f64 {
    let color = (left.warm - right.warm).abs() / 0.30 + (left.tint - right.tint).abs() / 0.30;
    let structure = (left.dynamic_range - right.dynamic_range).abs() / 0.40
        + (left.saturation - right.saturation).abs() / 0.45;
    let exposure = (left.exposure_ev - right.exposure_ev).abs() / 3.0;
    let subject = left
        .subject_light
        .zip(right.subject_light)
        .map(|(left, right)| (((left + 0.005).log2() - (right + 0.005).log2()).abs() / 2.0).min(2.))
        .unwrap_or(0.25);
    let mixed = left
        .mixed_light
        .zip(right.mixed_light)
        .map(|(left, right)| (left - right).abs() / 0.50)
        .unwrap_or(0.15);
    match kind {
        PhotoType::Portrait => {
            color * 0.30 + structure * 0.15 + exposure * 0.15 + subject * 0.30 + mixed * 0.10
        }
        PhotoType::RealEstate => {
            color * 0.25 + structure * 0.20 + exposure * 0.20 + subject * 0.10 + mixed * 0.25
        }
        PhotoType::Landscape => {
            color * 0.40 + structure * 0.25 + exposure * 0.20 + subject * 0.05 + mixed * 0.10
        }
    }
}

fn lighting_groups(
    prepared: &[Prepared<'_>],
    kind: PhotoType,
    cancel: &CancellationToken,
) -> ProcessingResult<(Vec<Vec<usize>>, Vec<f64>, u64)> {
    let mut order = (0..prepared.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        prepared[left]
            .input
            .asset_id
            .cmp(&prepared[right].input.asset_id)
    });
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut confidence: Vec<f64> = Vec::new();
    let mut buckets = HashMap::<LightKey, usize>::new();
    let mut comparisons = 0u64;
    for index in order {
        cancel.check()?;
        let key = light_key(&prepared[index]);
        let mut candidates = HashSet::new();
        for warm in -1..=1 {
            for tint in -1..=1 {
                for dynamic in -1..=1 {
                    if let Some(group) =
                        buckets.get(&LightKey(key.0 + warm, key.1 + tint, key.2 + dynamic))
                    {
                        candidates.insert(*group);
                    }
                }
            }
        }
        let candidate = candidates
            .into_iter()
            .filter_map(|group| {
                comparisons += 1;
                let distance =
                    lighting_distance(&prepared[groups[group][0]], &prepared[index], kind);
                (distance <= 0.82).then_some((group, distance))
            })
            .min_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.0.cmp(&right.0))
            });
        let group_index = if let Some((group, distance)) = candidate {
            groups[group].push(index);
            confidence[group] = confidence[group].min((1. - distance * 0.35).clamp(0.55, 0.95));
            group
        } else {
            groups.push(vec![index]);
            confidence.push(1.);
            groups.len() - 1
        };
        buckets.entry(key).or_insert(group_index);
    }
    for (group, confidence) in groups.iter().zip(&mut confidence) {
        if group.len() == 1 {
            *confidence = 0.60;
        }
    }
    Ok((groups, confidence, comparisons))
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.
    } else {
        values[middle]
    }
}

fn group_id(label: &str, kind: PhotoType, members: &[usize], prepared: &[Prepared<'_>]) -> String {
    let mut evidence = members
        .iter()
        .map(|&index| {
            format!(
                "{}:{}:{}",
                prepared[index].input.asset_id,
                prepared[index].input.source_fingerprint,
                prepared[index]
                    .input
                    .analysis
                    .as_ref()
                    .map(|analysis| analysis.analysis_id.as_str())
                    .unwrap_or("unavailable")
            )
        })
        .collect::<Vec<_>>();
    evidence.sort();
    digest(&[GROUPING_VERSION, label, kind.as_str(), &evidence.join("|")])
}

fn technical_candidate(input: &BatchAssetInput) -> Option<(f64, f64, Vec<String>)> {
    let analysis = input.analysis.as_ref()?;
    let exposure = &analysis.common.exposure;
    let severe = input.culling.as_ref().is_some_and(|assessment| {
        assessment.reasons.iter().any(|reason| {
            matches!(
                reason.code,
                ReasonCode::SourceUnavailable
                    | ReasonCode::SevereClipping
                    | ReasonCode::SevereSubjectSoftness
            )
        })
    });
    let (score, confidence) = input
        .culling
        .as_ref()
        .map(|assessment| (assessment.absolute_score, assessment.confidence))
        .unwrap_or_else(|| {
            let clipping = exposure.highlight_clip_fraction + exposure.shadow_clip_fraction;
            ((75. - clipping * 80.).clamp(0., 100.), 0.45)
        });
    if severe
        || score < 65.
        || confidence < 0.45
        || exposure.highlight_clip_fraction > 0.35
        || exposure.shadow_clip_fraction > 0.50
    {
        return None;
    }
    let mut reasons = vec!["Usable source exposure and dynamic range".into()];
    if input.culling.is_some() {
        reasons.push("No severe Phase 5 technical defect".into());
    } else {
        reasons.push("Phase 4 evidence only; lower anchor confidence".into());
    }
    Some((score, confidence, reasons))
}

fn references_for_groups(
    kind: BatchGroupKind,
    groups: &[Vec<usize>],
    group_ids: &[String],
    prepared: &[Prepared<'_>],
) -> (Vec<Vec<String>>, Vec<ReferenceCandidate>) {
    let mut group_references = Vec::with_capacity(groups.len());
    let mut all = Vec::new();
    for (group_index, members) in groups.iter().enumerate() {
        let mut candidates = members
            .iter()
            .filter_map(|&index| {
                technical_candidate(prepared[index].input).map(|candidate| (index, candidate))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_index, left), (right_index, right)| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| right.1.total_cmp(&left.1))
                .then_with(|| {
                    prepared[*left_index]
                        .input
                        .asset_id
                        .cmp(&prepared[*right_index].input.asset_id)
                })
        });
        let best = candidates.first().map(|(_, candidate)| candidate.0);
        let equivalent = candidates
            .into_iter()
            .filter(|(_, candidate)| best.is_some_and(|best| best - candidate.0 <= 2.5))
            .take(3)
            .collect::<Vec<_>>();
        let mut ids = equivalent
            .iter()
            .map(|(index, _)| prepared[*index].input.asset_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        for (rank, (index, (score, confidence, reasons))) in equivalent.into_iter().enumerate() {
            all.push(ReferenceCandidate {
                group_kind: kind,
                group_id: group_ids[group_index].clone(),
                asset_id: prepared[index].input.asset_id.clone(),
                rank: rank as u32 + 1,
                technical_score: score,
                confidence,
                reasons,
            });
        }
        group_references.push(ids);
    }
    (group_references, all)
}

type SequenceCandidate = (Vec<usize>, SequenceKind, f64, Option<String>);

fn sequence_groups(
    prepared: &[Prepared<'_>],
    scenes: &[Vec<usize>],
    kind: PhotoType,
    cancel: &CancellationToken,
) -> ProcessingResult<Vec<SequenceCandidate>> {
    let mut explicit = BTreeMap::<String, Vec<usize>>::new();
    for (index, asset) in prepared.iter().enumerate() {
        cancel.check()?;
        if let Some((group, relation, _)) = explicit_visual_group(asset.input) {
            if matches!(
                relation,
                DuplicateKind::Burst | DuplicateKind::NearDuplicate
            ) {
                explicit.entry(group.into()).or_default().push(index);
            }
        }
    }
    let mut sequences = Vec::new();
    let mut assigned = HashSet::new();
    for (group, mut members) in explicit {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|&left, &right| {
            prepared[left]
                .timestamp
                .cmp(&prepared[right].timestamp)
                .then_with(|| {
                    prepared[left]
                        .input
                        .asset_id
                        .cmp(&prepared[right].input.asset_id)
                })
        });
        let bracket = members.iter().any(|&index| {
            prepared[index]
                .input
                .culling
                .as_ref()
                .is_some_and(|assessment| assessment.similarity.bracket_like)
        }) || (kind == PhotoType::RealEstate
            && median(members.iter().map(|&index| prepared[index].exposure_ev)).is_finite()
            && members
                .iter()
                .map(|&index| prepared[index].exposure_ev)
                .fold(f64::NEG_INFINITY, f64::max)
                - members
                    .iter()
                    .map(|&index| prepared[index].exposure_ev)
                    .fold(f64::INFINITY, f64::min)
                >= 0.55);
        let relation = members
            .iter()
            .filter_map(|&index| explicit_visual_group(prepared[index].input))
            .map(|(_, relation, _)| relation)
            .collect::<Vec<_>>();
        let sequence_kind = if bracket {
            SequenceKind::ExposureBracket
        } else if relation.contains(&DuplicateKind::Burst) {
            SequenceKind::Burst
        } else {
            SequenceKind::RepeatedFrames
        };
        assigned.extend(members.iter().copied());
        sequences.push((members, sequence_kind, 0.85, Some(group)));
    }
    for scene in scenes {
        let mut members = scene
            .iter()
            .copied()
            .filter(|index| !assigned.contains(index))
            .collect::<Vec<_>>();
        if members.len() < 2 {
            continue;
        }
        members.sort_by_key(|&index| prepared[index].timestamp);
        let span = prepared[*members.last().unwrap()]
            .timestamp
            .zip(prepared[members[0]].timestamp)
            .map(|(last, first)| (last - first).abs());
        if span.is_some_and(|span| span <= 8_000) {
            let bracket = kind == PhotoType::RealEstate
                && members
                    .iter()
                    .map(|&index| prepared[index].exposure_ev)
                    .fold(f64::NEG_INFINITY, f64::max)
                    - members
                        .iter()
                        .map(|&index| prepared[index].exposure_ev)
                        .fold(f64::INFINITY, f64::min)
                    >= 0.55;
            sequences.push((
                members,
                if bracket {
                    SequenceKind::ExposureBracket
                } else {
                    SequenceKind::RepeatedFrames
                },
                0.65,
                None,
            ));
        }
    }
    Ok(sequences)
}

pub fn build_from_inputs(
    job: &str,
    kind: PhotoType,
    inputs: &[BatchAssetInput],
    cancel: &CancellationToken,
) -> ProcessingResult<BatchContext> {
    cancel.check()?;
    let started = Instant::now();
    let identity = selection_identity(job, kind, inputs)?;
    let candidate_started = Instant::now();
    let prepared = inputs.iter().filter_map(prepare).collect::<Vec<_>>();
    let candidate_generation_ms = candidate_started.elapsed().as_millis() as u64;
    let grouping_started = Instant::now();
    let (scenes, scene_confidence, scene_comparisons) = scene_groups(&prepared, kind, cancel)?;
    let (lighting, lighting_confidence, lighting_comparisons) =
        lighting_groups(&prepared, kind, cancel)?;
    let sequences = sequence_groups(&prepared, &scenes, kind, cancel)?;
    let scene_ids = scenes
        .iter()
        .map(|members| group_id("scene", kind, members, &prepared))
        .collect::<Vec<_>>();
    let lighting_ids = lighting
        .iter()
        .map(|members| group_id("lighting", kind, members, &prepared))
        .collect::<Vec<_>>();
    let sequence_ids = sequences
        .iter()
        .map(|(members, sequence_kind, _, _)| {
            group_id(
                match sequence_kind {
                    SequenceKind::Burst => "sequence-burst",
                    SequenceKind::ExposureBracket => "sequence-bracket",
                    SequenceKind::RepeatedFrames => "sequence-repeated",
                },
                kind,
                members,
                &prepared,
            )
        })
        .collect::<Vec<_>>();
    let grouping_ms = grouping_started.elapsed().as_millis() as u64;
    let context_started = Instant::now();
    let (scene_references, mut references) =
        references_for_groups(BatchGroupKind::Scene, &scenes, &scene_ids, &prepared);
    let (lighting_references, lighting_candidates) = references_for_groups(
        BatchGroupKind::Lighting,
        &lighting,
        &lighting_ids,
        &prepared,
    );
    references.extend(lighting_candidates);
    let scene_contracts = scenes
        .iter()
        .enumerate()
        .map(|(index, members)| BatchGroup {
            group_id: scene_ids[index].clone(),
            asset_ids: {
                let mut ids = members
                    .iter()
                    .map(|&member| prepared[member].input.asset_id.clone())
                    .collect::<Vec<_>>();
                ids.sort();
                ids
            },
            confidence: scene_confidence[index],
            reference_candidate_ids: scene_references[index].clone(),
        })
        .collect::<Vec<_>>();
    let lighting_contracts = lighting
        .iter()
        .enumerate()
        .map(|(index, members)| BatchGroup {
            group_id: lighting_ids[index].clone(),
            asset_ids: {
                let mut ids = members
                    .iter()
                    .map(|&member| prepared[member].input.asset_id.clone())
                    .collect::<Vec<_>>();
                ids.sort();
                ids
            },
            confidence: lighting_confidence[index],
            reference_candidate_ids: lighting_references[index].clone(),
        })
        .collect::<Vec<_>>();
    let sequence_contracts = sequences
        .iter()
        .enumerate()
        .map(
            |(index, (members, sequence_kind, confidence, source))| SequenceGroup {
                group_id: sequence_ids[index].clone(),
                asset_ids: {
                    let mut ids = members
                        .iter()
                        .map(|&member| prepared[member].input.asset_id.clone())
                        .collect::<Vec<_>>();
                    ids.sort();
                    ids
                },
                kind: *sequence_kind,
                confidence: *confidence,
                source_culling_group_id: source.clone(),
            },
        )
        .collect::<Vec<_>>();
    let mut scene_for = HashMap::<&str, (usize, &str, f64)>::new();
    for (group_index, members) in scenes.iter().enumerate() {
        for &member in members {
            scene_for.insert(
                prepared[member].input.asset_id.as_str(),
                (
                    group_index,
                    scene_ids[group_index].as_str(),
                    scene_confidence[group_index],
                ),
            );
        }
    }
    let mut lighting_for = HashMap::<&str, (usize, &str, f64)>::new();
    for (group_index, members) in lighting.iter().enumerate() {
        for &member in members {
            lighting_for.insert(
                prepared[member].input.asset_id.as_str(),
                (
                    group_index,
                    lighting_ids[group_index].as_str(),
                    lighting_confidence[group_index],
                ),
            );
        }
    }
    let mut sequence_for = HashMap::<&str, &str>::new();
    for (group_index, (members, _, _, _)) in sequences.iter().enumerate() {
        for &member in members {
            sequence_for.insert(
                prepared[member].input.asset_id.as_str(),
                sequence_ids[group_index].as_str(),
            );
        }
    }
    let rank_one = references
        .iter()
        .filter(|candidate| candidate.rank == 1)
        .map(|candidate| {
            (
                (candidate.group_kind, candidate.group_id.as_str()),
                candidate.asset_id.as_str(),
            )
        })
        .collect::<HashMap<_, _>>();
    let prepared_by_id = prepared
        .iter()
        .map(|asset| (asset.input.asset_id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let mut asset_contexts = Vec::with_capacity(inputs.len());
    let mut available = 0u32;
    let mut partial = 0u32;
    let mut unavailable = 0u32;
    let mut selected_ids = inputs
        .iter()
        .map(|input| input.asset_id.clone())
        .collect::<Vec<_>>();
    selected_ids.sort();
    for asset_id in &selected_ids {
        cancel.check()?;
        let input = inputs
            .iter()
            .find(|input| &input.asset_id == asset_id)
            .expect("selection identity rejected duplicates");
        let Some(asset) = prepared_by_id.get(asset_id.as_str()).copied() else {
            unavailable += 1;
            asset_contexts.push(AssetBatchContext {
                asset_id: asset_id.clone(),
                availability: ContextAvailability::Unavailable,
                scene_group_id: None,
                lighting_group_id: None,
                sequence_group_id: None,
                reference_asset_id: None,
                exposure_delta_from_group: None,
                wb_delta_from_group: None,
                group_confidence: 0.,
                consistency_notes: vec![ConsistencyNote {
                    code: ConsistencyNoteCode::AnalysisUnavailable,
                    message: input
                        .unavailable_reason
                        .clone()
                        .unwrap_or_else(|| "Current PhotoAnalysis is unavailable".into()),
                }],
            });
            continue;
        };
        let availability = if input
            .culling
            .as_ref()
            .and_then(|value| value.features.as_ref())
            .is_some()
        {
            available += 1;
            ContextAvailability::Available
        } else {
            partial += 1;
            ContextAvailability::Partial
        };
        let (scene_index, scene_id, scene_confidence) = scene_for[asset_id.as_str()];
        let (lighting_index, lighting_id, lighting_confidence) = lighting_for[asset_id.as_str()];
        let exposure_median = median(
            lighting[lighting_index]
                .iter()
                .map(|&index| prepared[index].exposure_ev),
        );
        let warm_median = median(
            lighting[lighting_index]
                .iter()
                .map(|&index| prepared[index].warm),
        );
        let tint_median = median(
            lighting[lighting_index]
                .iter()
                .map(|&index| prepared[index].tint),
        );
        let delta_ev = asset.exposure_ev - exposure_median;
        let warm_delta = asset.warm - warm_median;
        let tint_delta = asset.tint - tint_median;
        let confidence = if availability == ContextAvailability::Available {
            0.80
        } else {
            0.60
        };
        let reference = rank_one
            .get(&(BatchGroupKind::Lighting, lighting_id))
            .or_else(|| rank_one.get(&(BatchGroupKind::Scene, scene_id)))
            .copied()
            .map(str::to_string);
        let mut notes = Vec::new();
        if reference.as_deref() == Some(asset_id) {
            notes.push(ConsistencyNote {
                code: ConsistencyNoteCode::ExposureReference,
                message: "Highest-ranked technical reference for this source group".into(),
            });
        }
        notes.push(ConsistencyNote {
            code: if delta_ev < -0.30 {
                ConsistencyNoteCode::DarkerThanGroup
            } else if delta_ev > 0.30 {
                ConsistencyNoteCode::BrighterThanGroup
            } else {
                ConsistencyNoteCode::NearExposureMedian
            },
            message: format!("Source exposure is {delta_ev:+.2} EV from the lighting-group median"),
        });
        let mut wb_note = false;
        for (delta, negative, positive, axis) in [
            (
                warm_delta,
                ConsistencyNoteCode::CoolerThanGroup,
                ConsistencyNoteCode::WarmerThanGroup,
                "warm/cool",
            ),
            (
                tint_delta,
                ConsistencyNoteCode::MoreMagentaThanGroup,
                ConsistencyNoteCode::GreenerThanGroup,
                "green/magenta",
            ),
        ] {
            if delta.abs() > 0.08 {
                wb_note = true;
                notes.push(ConsistencyNote {
                    code: if delta < 0. { negative } else { positive },
                    message: format!("Source {axis} signal is {delta:+.3} from the group median"),
                });
            }
        }
        if !wb_note {
            notes.push(ConsistencyNote {
                code: ConsistencyNoteCode::NearWhiteBalanceMedian,
                message: "Source color balance is near the lighting-group median".into(),
            });
        }
        if let Some(sequence_id) = sequence_for.get(asset_id.as_str()) {
            if sequence_contracts
                .iter()
                .find(|sequence| sequence.group_id == **sequence_id)
                .is_some_and(|sequence| sequence.kind == SequenceKind::ExposureBracket)
            {
                notes.push(ConsistencyNote {
                    code: ConsistencyNoteCode::BracketMember,
                    message: "Exposure difference belongs to a recognized bracket sequence".into(),
                });
            }
        }
        if availability == ContextAvailability::Partial {
            notes.push(ConsistencyNote {
                code: ConsistencyNoteCode::PartialEvidence,
                message:
                    "PhotoAnalysis is usable, but current Phase 5 visual evidence is unavailable"
                        .into(),
            });
        }
        asset_contexts.push(AssetBatchContext {
            asset_id: asset_id.clone(),
            availability,
            scene_group_id: Some(scene_id.into()),
            lighting_group_id: Some(lighting_id.into()),
            sequence_group_id: sequence_for.get(asset_id.as_str()).map(|id| (*id).into()),
            reference_asset_id: reference,
            exposure_delta_from_group: Some(ExposureRelationship {
                delta_ev,
                confidence,
            }),
            wb_delta_from_group: Some(WhiteBalanceRelationship {
                warm_cool_delta: warm_delta,
                green_magenta_delta: tint_delta,
                confidence,
            }),
            group_confidence: scene_confidence.min(lighting_confidence),
            consistency_notes: notes,
        });
        let _ = scene_index;
    }
    references.sort_by(|left, right| {
        left.group_id
            .cmp(&right.group_id)
            .then(left.rank.cmp(&right.rank))
            .then(left.asset_id.cmp(&right.asset_id))
    });
    let context_ms = context_started.elapsed().as_millis() as u64;
    let mut warnings = inputs
        .iter()
        .filter_map(|input| {
            input.unavailable_reason.as_ref().map(|reason| {
                format!(
                    "{}: {}",
                    input.asset_id,
                    reason.chars().take(800).collect::<String>()
                )
            })
        })
        .collect::<Vec<_>>();
    warnings.truncate(256);
    let context = BatchContext {
        schema_version: BATCH_CONTEXT_SCHEMA_VERSION,
        batch_id: digest(&["batch-context-v1", &identity]),
        job_id: job.into(),
        photo_type: kind,
        selected_asset_ids: selected_ids,
        selection_identity: identity,
        created_at: chrono::Utc::now().to_rfc3339(),
        analysis_version: format!("photo-analysis-schema-{PHOTO_ANALYSIS_SCHEMA_VERSION}"),
        grouping_version: grouping_version(),
        scene_groups: scene_contracts,
        lighting_groups: lighting_contracts,
        sequence_groups: sequence_contracts,
        asset_contexts,
        reference_candidates: references,
        diagnostics: BatchDiagnostics {
            available_assets: available,
            partial_assets: partial,
            unavailable_assets: unavailable,
            candidate_comparisons: scene_comparisons + lighting_comparisons,
            candidate_limit_per_asset: CANDIDATE_LIMIT as u32,
            timings: BatchStageTimings {
                candidate_generation_ms,
                grouping_ms,
                context_ms,
                total_ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            },
            warnings,
        },
    };
    context.validate().map_err(internal)?;
    Ok(context)
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    use crate::models::NewJob;

    #[test]
    fn exact_identities_are_cached_and_changed_evidence_keeps_history() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = JobRepository::open(temporary.path().join("jobs.sqlite3")).unwrap();
        let job = repository
            .create_job(&NewJob {
                name: "Batch cache".into(),
                input_path: temporary.path().join("input"),
                output_path: temporary.path().join("output"),
            })
            .unwrap();
        let first = BatchAssetInput {
            asset_id: "asset-1".into(),
            source_fingerprint: "source-1".into(),
            analysis: None,
            culling: None,
            unavailable_reason: Some("Fixture intentionally has no analysis".into()),
        };
        let first_context = build_from_inputs(
            &job.id,
            PhotoType::Portrait,
            std::slice::from_ref(&first),
            &CancellationToken::default(),
        )
        .unwrap();
        repository.persist_batch_context(&first_context).unwrap();
        assert_eq!(
            repository
                .batch_context(
                    &job.id,
                    PhotoType::Portrait,
                    &first_context.selection_identity,
                )
                .unwrap(),
            Some(first_context.clone())
        );
        assert!(!repository
            .has_other_batch_context(
                &job.id,
                PhotoType::Portrait,
                Some(&first_context.selection_identity)
            )
            .unwrap());

        let mut changed = first;
        changed.source_fingerprint = "source-2".into();
        let changed_context = build_from_inputs(
            &job.id,
            PhotoType::Portrait,
            &[changed],
            &CancellationToken::default(),
        )
        .unwrap();
        repository.persist_batch_context(&changed_context).unwrap();
        assert_ne!(first_context.batch_id, changed_context.batch_id);
        assert!(repository
            .has_other_batch_context(
                &job.id,
                PhotoType::Portrait,
                Some(&changed_context.selection_identity)
            )
            .unwrap());
    }
}
