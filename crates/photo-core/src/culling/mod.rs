//! Local source culling. Recipes, ratings and selection are independent lifecycles.
pub mod content;
pub mod features;
pub mod score;
pub mod similarity;
mod storage;
use crate::{
    analysis::{AnalysisRequest, AnalysisService},
    models::Asset,
    rendering::{self, internal, CpuProcessingEngine},
    repository::JobRepository,
};
use photo_contracts::{
    analysis::{PhotoType, ProviderIdentity},
    culling::*,
    CancellationToken, ProcessingError, ProcessingErrorCode, ProcessingResult,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};
pub const MAX_BATCH: usize = 5000;
pub fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    format!("{:x}", h.finalize())
}
pub fn feature_key(
    source: &str,
    analysis: &str,
    kind: PhotoType,
    models: &[ProviderIdentity],
) -> String {
    digest(&[
        source,
        analysis,
        kind.as_str(),
        &CULLING_SCHEMA_VERSION.to_string(),
        &photo_contracts::analysis::PHOTO_ANALYSIS_SCHEMA_VERSION.to_string(),
        features::FEATURE_VERSION,
        score::CULLING_ENGINE_VERSION,
        similarity::SIMILARITY_VERSION,
        &serde_json::to_string(models).expect("provider serializable"),
    ])
}
fn source(a: &Asset) -> ProcessingResult<String> {
    Ok(digest(&[
        &rendering::source_identity(&a.original_path)?,
        &serde_json::to_string(&a.metadata).map_err(internal)?,
    ]))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CullingRequest {
    pub job_id: String,
    pub photo_type: PhotoType,
    pub request_id: String,
    pub force: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CullingProgress {
    pub job_id: String,
    pub request_id: String,
    pub photo_type: PhotoType,
    pub status: String,
    pub stage: String,
    pub completed: u32,
    pub total: u32,
    pub failed: u32,
    pub cached: u32,
    pub duration_ms: u64,
    pub error: Option<String>,
    #[serde(default)]
    pub hash_bytes: u64,
    #[serde(default)]
    pub hash_cached: u32,
    #[serde(default)]
    pub hash_duration_ms: u64,
    #[serde(default)]
    pub hash_failures: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CullingItem {
    pub asset: Asset,
    pub ai_rating: Option<Stars>,
    pub user_rating: Option<Stars>,
    pub effective_rating: Option<Stars>,
    pub selected_for_editing: bool,
    pub stale: bool,
    pub group_id: Option<String>,
    pub preferred: bool,
    pub review_count: usize,
    pub relationship_kind: Option<DuplicateKind>,
    pub similarity: Option<SimilarityContext>,
    #[serde(default)]
    pub issues: Vec<CullingIssue>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CullingIssue {
    Blurry,
    ClosedEyes,
}
pub fn culling_issues(assessment: &CullingAssessment) -> Vec<CullingIssue> {
    let mut issues = Vec::new();
    if assessment
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::SevereSubjectSoftness)
    {
        issues.push(CullingIssue::Blurry);
    }
    if assessment
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::EyesClosed)
    {
        issues.push(CullingIssue::ClosedEyes);
    }
    issues
}
fn choose_display_preferred(items: &mut [CullingItem]) {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        let Some(similarity) = &item.similarity else {
            continue;
        };
        if matches!(
            similarity.kind,
            DuplicateKind::NearDuplicate | DuplicateKind::Burst
        ) {
            if let Some(group_id) = &similarity.group_id {
                groups.entry(group_id.clone()).or_default().push(index);
            }
        }
    }
    for members in groups.into_values() {
        let winner = members
            .iter()
            .copied()
            .filter(|&index| {
                let item = &items[index];
                item.similarity
                    .as_ref()
                    .and_then(|similarity| similarity.exact.as_ref())
                    .is_none_or(|exact| exact.canonical_asset_id == item.asset.id)
            })
            .min_by(|&a, &b| {
                let a = &items[a];
                let b = &items[b];
                let relative = |item: &CullingItem| {
                    item.similarity
                        .as_ref()
                        .and_then(|similarity| similarity.relative_score)
                        .unwrap_or(f64::INFINITY)
                };
                relative(a)
                    .total_cmp(&relative(b))
                    .then_with(|| a.asset.filename.cmp(&b.asset.filename))
                    .then_with(|| a.asset.original_path.cmp(&b.asset.original_path))
                    .then_with(|| a.asset.id.cmp(&b.asset.id))
            });
        for index in members {
            items[index].preferred = Some(index) == winner;
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CullingIssueAvailability {
    pub blurry: bool,
    pub closed_eyes: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DuplicateCounts {
    pub exact_copies: u32,
    pub exact_groups: u32,
    pub near_groups: u32,
    pub burst_groups: u32,
    pub similar_groups: u32,
    pub unique_images: u32,
    pub unclassified_images: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CullingOverview {
    pub items: Vec<CullingItem>,
    pub counts: [u32; 6],
    pub selected_count: u32,
    pub progress: Option<CullingProgress>,
    pub duplicates: DuplicateCounts,
    pub issue_availability: CullingIssueAvailability,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipFilter {
    #[default]
    All,
    Exact,
    NearSimilar,
    Preferred,
    Unique,
}
impl RelationshipFilter {
    fn matches(self, i: &CullingItem) -> bool {
        match self {
            Self::All => true,
            Self::Exact => i.relationship_kind == Some(DuplicateKind::Exact),
            Self::NearSimilar => i.similarity.as_ref().is_some_and(|s| s.group_id.is_some()),
            Self::Preferred => i.preferred,
            Self::Unique => i.relationship_kind == Some(DuplicateKind::Unique),
        }
    }
}
struct Active {
    request: CullingRequest,
    token: CancellationToken,
    nested: Option<String>,
}
type ActiveSlot = Arc<Mutex<Option<Active>>>;
pub struct CullingPermit {
    request: CullingRequest,
    token: CancellationToken,
    active: ActiveSlot,
    repository: JobRepository,
}
impl Drop for CullingPermit {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.active.lock() {
            if slot
                .as_ref()
                .is_some_and(|a| a.request.request_id == self.request.request_id)
            {
                if let Ok(Some(mut p)) = self.repository.culling_progress(&self.request.job_id) {
                    if matches!(p.status.as_str(), "queued" | "running") {
                        p.status = "cancelled".into();
                        p.stage = "Reservation released; completed ratings preserved".into();
                        let _ = self.repository.save_culling_progress(&p);
                    }
                }
                *slot = None;
            }
        }
    }
}
pub struct CullingService {
    repository: JobRepository,
    analysis: Arc<AnalysisService>,
    engine: Arc<CpuProcessingEngine>,
    faces: Arc<dyn features::FaceDetector>,
    eyes: Arc<dyn features::EyeStateDetector>,
    active: ActiveSlot,
}
impl CullingService {
    pub fn new(
        repository: JobRepository,
        analysis: Arc<AnalysisService>,
        engine: Arc<CpuProcessingEngine>,
        faces: Arc<dyn features::FaceDetector>,
        eyes: Arc<dyn features::EyeStateDetector>,
    ) -> Self {
        Self {
            repository,
            analysis,
            engine,
            faces,
            eyes,
            active: Arc::new(Mutex::new(None)),
        }
    }
    fn models(&self, kind: PhotoType) -> Vec<ProviderIdentity> {
        if kind == PhotoType::Portrait {
            vec![self.faces.identity(), self.eyes.identity()]
        } else {
            vec![]
        }
    }
    pub fn reserve(&self, request: CullingRequest) -> ProcessingResult<CullingPermit> {
        if request.request_id.is_empty() || request.request_id.len() > 128 {
            return Err(internal("Invalid culling request ID"));
        }
        let mut slot = self.active.lock().map_err(internal)?;
        if slot.is_some() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::Busy,
                "One culling batch is already active",
            ));
        }
        let job = self.repository.get_job(&request.job_id).map_err(internal)?;
        if job.status == "scanning" || job.asset_count > MAX_BATCH as u64 {
            return Err(internal(format!(
                "Wait for scanning to finish; Phase 5 supports up to {MAX_BATCH} photos per job"
            )));
        }
        let token = CancellationToken::default();
        self.repository.save_culling_progress(&CullingProgress {
            job_id: request.job_id.clone(),
            request_id: request.request_id.clone(),
            photo_type: request.photo_type,
            status: "queued".into(),
            stage: "Queued".into(),
            completed: 0,
            total: job.asset_count as u32,
            failed: 0,
            cached: 0,
            duration_ms: 0,
            error: None,
            hash_bytes: 0,
            hash_cached: 0,
            hash_duration_ms: 0,
            hash_failures: 0,
        })?;
        *slot = Some(Active {
            request: request.clone(),
            token: token.clone(),
            nested: None,
        });
        Ok(CullingPermit {
            request,
            token,
            active: self.active.clone(),
            repository: self.repository.clone(),
        })
    }
    pub fn cancel(&self, id: &str) -> ProcessingResult<()> {
        let slot = self.active.lock().map_err(internal)?;
        if let Some(a) = slot.as_ref().filter(|a| a.request.request_id == id) {
            a.token.cancel();
            if let Some(n) = &a.nested {
                self.analysis.cancel(n)?;
            }
        }
        Ok(())
    }
    pub fn progress(&self, job: &str) -> ProcessingResult<Option<CullingProgress>> {
        self.repository.culling_progress(job)
    }
    fn assets(&self, job: &str) -> ProcessingResult<Vec<Asset>> {
        let j = self.repository.get_job(job).map_err(internal)?;
        if j.asset_count > MAX_BATCH as u64 {
            return Err(internal(format!(
                "Culling view supports at most {MAX_BATCH} assets; use smaller jobs"
            )));
        }
        let mut assets = Vec::new();
        loop {
            let page = self
                .repository
                .assets(job, assets.len() as u32, 100)
                .map_err(internal)?;
            if page.total > MAX_BATCH as u64 || assets.len() + page.items.len() > MAX_BATCH {
                return Err(internal("Job grew beyond the culling batch limit; wait for scanning and use a smaller job"));
            }
            assets.extend(page.items);
            if assets.len() as u64 >= page.total {
                break;
            }
        }
        Ok(assets)
    }
    fn current(&self, a: &Asset, kind: PhotoType) -> ProcessingResult<CullingState> {
        let mut s = self.repository.culling_state(&a.job_id, &a.id, kind)?;
        if let Some(v) = &s.assessment {
            let analysis = self
                .analysis
                .get_analysis(&a.job_id, &a.id, kind)
                .ok()
                .and_then(|s| s.analysis);
            let src = source(a).unwrap_or_else(|_| digest(&["unavailable", &a.fingerprint]));
            s.stale = src != v.source_fingerprint
                || v.culling_engine_version != score::CULLING_ENGINE_VERSION
                || v.model_versions != self.models(kind)
                || v.source_analysis_id != analysis.as_ref().map(|a| a.analysis_id.clone())
                || v.features.as_ref().is_some_and(|f| {
                    f.feature_version != features::FEATURE_VERSION
                        || f.source_analysis_version
                            != photo_contracts::analysis::PHOTO_ANALYSIS_SCHEMA_VERSION
                })
                || v.cache_key != assessment_key(v);
            if let Some(stamp) = &v.duplicate_stamp {
                s.stale |= content::current_stamp(&a.original_path).as_ref().ok() != Some(stamp);
            }
            if s.stale {
                s.effective_rating = s.user_rating;
            }
        }
        Ok(s)
    }
    fn states(&self, job: &str, kind: PhotoType) -> ProcessingResult<Vec<(Asset, CullingState)>> {
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let assets = self.assets(job)?;
        let membership = membership_key(&assets);
        for a in assets {
            let mut s = self.current(&a, kind)?;
            if s.assessment
                .as_ref()
                .and_then(|a| a.membership_key.as_ref())
                .is_some_and(|key| *key != membership)
            {
                s.stale = true;
                s.effective_rating = s.user_rating;
            }
            bytes += s
                .assessment
                .as_ref()
                .map(|a| serde_json::to_string(a).map(|j| j.len()).unwrap_or(0))
                .unwrap_or(0);
            if bytes > 64 * 1024 * 1024 {
                return Err(internal(
                    "Culling view exceeded the 64 MiB evidence budget; use smaller jobs",
                ));
            }
            rows.push((a, s));
        }
        // A changed/deleted/re-cull-in-progress group member invalidates old relative context.
        let mut groups: HashMap<String, (Vec<usize>, u32)> = HashMap::new();
        for (index, (_, s)) in rows.iter().enumerate() {
            if let Some(a) = &s.assessment {
                let relationships = a
                    .similarity
                    .group_id
                    .as_ref()
                    .map(|id| (id, a.similarity.group_size))
                    .into_iter()
                    .chain(
                        a.similarity
                            .exact
                            .as_ref()
                            .map(|e| (&e.group_id, e.group_size)),
                    );
                for (id, size) in relationships {
                    groups
                        .entry(id.clone())
                        .or_insert((Vec::new(), size))
                        .0
                        .push(index);
                }
            }
        }
        // Propagate across overlapping exact + visual relationships, visiting each group once.
        let mut queue: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, (_, s))| s.stale)
            .map(|(i, _)| i)
            .collect();
        for (members, size) in groups.values() {
            if members.len() != *size as usize {
                queue.extend(members);
            }
        }
        while let Some(index) = queue.pop() {
            let s = &mut rows[index].1;
            s.stale = true;
            s.effective_rating = s.user_rating;
            if let Some(a) = &s.assessment {
                for id in [
                    a.similarity.group_id.as_ref(),
                    a.similarity.exact.as_ref().map(|e| &e.group_id),
                ]
                .into_iter()
                .flatten()
                {
                    if let Some((members, _)) = groups.remove(id) {
                        queue.extend(members);
                    }
                }
            }
        }
        Ok(rows)
    }
    pub fn overview(&self, job: &str, kind: PhotoType) -> ProcessingResult<CullingOverview> {
        let mut counts = [0; 6];
        let mut selected = 0;
        let mut items = self
            .states(job, kind)?
            .into_iter()
            .map(|(asset, s)| {
                counts[s.effective_rating.map(Stars::get).unwrap_or(0) as usize] += 1;
                selected += u32::from(s.selected_for_editing);
                let a = s.assessment.as_ref();
                CullingItem {
                    relationship_kind: if s.stale {
                        None
                    } else {
                        a.and_then(classified_kind)
                    },
                    similarity: if s.stale {
                        None
                    } else {
                        a.filter(|a| a.membership_key.is_some())
                            .map(|a| a.similarity.clone())
                    },
                    asset,
                    ai_rating: if s.stale {
                        None
                    } else {
                        a.and_then(|a| a.ai_rating)
                    },
                    user_rating: s.user_rating,
                    effective_rating: s.effective_rating,
                    selected_for_editing: s.selected_for_editing,
                    stale: s.stale,
                    group_id: if s.stale {
                        None
                    } else {
                        a.and_then(|a| a.similarity.group_id.clone())
                    },
                    preferred: !s.stale
                        && a.is_some_and(|a| {
                            a.similarity
                                .exact
                                .as_ref()
                                .map(|e| e.canonical_asset_id == a.asset_id)
                                .unwrap_or(a.similarity.preferred)
                        }),
                    review_count: a
                        .map(|a| {
                            a.reasons
                                .iter()
                                .filter(|r| {
                                    matches!(
                                        r.severity,
                                        Severity::Review | Severity::Issue | Severity::Major
                                    )
                                })
                                .count()
                        })
                        .unwrap_or(0),
                    issues: if s.stale {
                        Vec::new()
                    } else {
                        a.map(culling_issues).unwrap_or_default()
                    },
                }
            })
            .collect::<Vec<_>>();
        // The scoring contract may retain several candidates within its technical tie
        // tolerance. The overview exposes one deterministic display representative so
        // photographer-facing duplicate hiding actually collapses each visual group.
        choose_display_preferred(&mut items);
        let duplicates = duplicate_counts(&items);
        Ok(CullingOverview {
            items,
            counts,
            selected_count: selected,
            progress: self.progress(job)?,
            duplicates,
            issue_availability: CullingIssueAvailability {
                blurry: true,
                closed_eyes: self.eyes.identity().model != "none",
            },
        })
    }
    pub fn detail(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
    ) -> ProcessingResult<CullingState> {
        self.states(job, kind)?
            .into_iter()
            .find(|(a, _)| a.id == asset)
            .map(|(_, s)| s)
            .ok_or_else(|| internal("Asset not found"))
    }
    pub fn set_rating(
        &self,
        job: &str,
        asset: &str,
        kind: PhotoType,
        rating: Option<Stars>,
    ) -> ProcessingResult<()> {
        self.repository.culling_override(job, asset, kind, rating)
    }
    pub fn select_asset(&self, job: &str, asset: &str, selected: bool) -> ProcessingResult<()> {
        self.repository.asset(job, asset).map_err(internal)?;
        self.repository
            .culling_select(job, &[(asset.into(), selected)])
    }
    pub fn select_assets(
        &self,
        job: &str,
        kind: PhotoType,
        selected_assets: &[String],
    ) -> ProcessingResult<()> {
        if selected_assets.len() > MAX_BATCH {
            return Err(internal("Selection exceeds batch limit"));
        }
        let selected = selected_assets
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        if selected.len() != selected_assets.len() {
            return Err(internal("Selection contains duplicate asset IDs"));
        }
        let items = self.overview(job, kind)?.items;
        let known = items
            .iter()
            .map(|item| item.asset.id.as_str())
            .collect::<HashSet<_>>();
        if !selected.iter().all(|asset| known.contains(asset)) {
            return Err(internal("Selection contains an asset outside this job"));
        }
        let values = items
            .into_iter()
            .map(|item| {
                let is_selected = selected.contains(item.asset.id.as_str());
                (item.asset.id, is_selected)
            })
            .collect::<Vec<_>>();
        self.repository.culling_select(job, &values)
    }
    pub fn select_ratings(
        &self,
        job: &str,
        kind: PhotoType,
        ratings: &[Stars],
    ) -> ProcessingResult<()> {
        self.select_filtered(job, kind, ratings, RelationshipFilter::All, false, true)
    }
    pub fn select_filtered(
        &self,
        job: &str,
        kind: PhotoType,
        ratings: &[Stars],
        relationship: RelationshipFilter,
        selected_only: bool,
        exclude_exact_duplicates: bool,
    ) -> ProcessingResult<()> {
        if ratings.len() > 5 {
            return Err(internal("Select at most five rating values"));
        }
        // Explicit snapshot replacement, never an ongoing rule that silently reselects photos.
        let items = self.overview(job, kind)?.items;
        let values = items
            .iter()
            .map(|i| {
                let selected = i.effective_rating.is_some_and(|r| ratings.contains(&r))
                    && relationship.matches(i)
                    && (!selected_only || i.selected_for_editing)
                    && (!exclude_exact_duplicates || exact_selection_eligible(i));
                (i.asset.id.clone(), selected)
            })
            .collect::<Vec<_>>();
        self.repository.culling_select(job, &values)
    }
    pub fn run(&self, permit: CullingPermit) -> ProcessingResult<CullingProgress> {
        if !Arc::ptr_eq(&self.active, &permit.active) {
            return Err(internal("Culling permit belongs to another service"));
        }
        let started = Instant::now();
        let mut p = self
            .progress(&permit.request.job_id)?
            .ok_or_else(|| internal("Missing run"))?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_inner(&permit, &mut p)
        }))
        .unwrap_or_else(|_| {
            Err(internal(
                "Culling worker stopped unexpectedly; completed ratings are preserved",
            ))
        });
        p.duration_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(()) => {
                p.status = "complete".into();
                p.stage = "Complete; editing selection unchanged".into();
            }
            Err(e) => {
                p.status = if e.code == ProcessingErrorCode::Cancelled {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                p.error = Some(e.message);
                p.stage = "Stopped; completed ratings preserved".into();
            }
        }
        self.repository.save_culling_progress(&p)?;
        Ok(p)
    }
    fn run_inner(&self, permit: &CullingPermit, p: &mut CullingProgress) -> ProcessingResult<()> {
        let r = &permit.request;
        let cancel = &permit.token;
        let assets = self.assets(&r.job_id)?;
        let mut collected = Vec::new();
        let mut byte_budget = 0;
        p.status = "running".into();
        let mut hashes = HashMap::new();
        for (index, a) in assets.iter().enumerate() {
            cancel.check()?;
            p.stage = format!(
                "Checking full-file duplicate identity · {}/{} · {}",
                index + 1,
                assets.len(),
                a.filename
            );
            self.repository.save_culling_progress(p)?;
            match content::identify(&self.repository, a, r.force, cancel) {
                Ok(h) => {
                    p.hash_bytes += h.bytes_hashed;
                    p.hash_duration_ms += h.duration_ms;
                    p.hash_cached += u32::from(h.cached);
                    hashes.insert(a.id.clone(), h);
                }
                Err(e) => {
                    cancel.check()?;
                    p.hash_failures += 1;
                    p.error = Some(format!(
                        "{}: duplicate identity unavailable: {}",
                        a.filename, e.message
                    ));
                }
            }
        }
        let exact = similarity::exact_groups(
            &hashes
                .iter()
                .map(|(id, h)| (id.clone(), h.content.clone()))
                .collect::<Vec<_>>(),
            cancel,
        )?;
        let mut results = HashMap::new();
        for a in &assets {
            cancel.check()?;
            p.stage = format!("Source analysis and face measurements · {}", a.filename);
            self.repository.save_culling_progress(p)?;
            let result = self.bound_features(a, r, hashes.get(&a.id), cancel);
            let mut assessment = match result {
                Ok((f, cached)) => {
                    if cached {
                        p.cached += 1;
                    }
                    byte_budget += serde_json::to_string(&f).map_err(internal)?.len();
                    if byte_budget > 64 * 1024 * 1024 {
                        return Err(internal(
                            "Feature budget exceeded; completed ratings preserved",
                        ));
                    }
                    let a = assess(f.clone(), SimilarityContext::default())?;
                    collected.push(f);
                    a
                }
                Err(e) => {
                    cancel.check()?;
                    p.failed += 1;
                    p.error = Some(format!("{}: {}", a.filename, e.message));
                    unrated(a, r.photo_type, &self.models(r.photo_type), &e.message)
                }
            };
            cancel.check()?;
            bind_identity(&mut assessment, hashes.get(&a.id), None)?;
            // Do not replace a valid final group just to resume a cached job; new/forced rows commit immediately.
            let old = self.current(a, r.photo_type)?;
            if r.force || old.stale || old.assessment.is_none() {
                self.repository.persist_culling(
                    &r.job_id,
                    std::slice::from_ref(&assessment),
                    cancel,
                )?;
            }
            results.insert(a.id.clone(), assessment);
            p.completed += 1;
            self.repository.save_culling_progress(p)?;
        }
        p.stage = "Comparing similar frames".into();
        self.repository.save_culling_progress(p)?;
        let latest = self.assets(&r.job_id)?;
        if latest.len() != assets.len()
            || latest
                .iter()
                .zip(&assets)
                .any(|(a, b)| a.id != b.id || a.fingerprint != b.fingerprint)
        {
            return Err(internal(
                "Job membership changed during culling; resume after scanning finishes",
            ));
        }
        let membership = membership_key(&assets);
        let contexts = similarity::group_with_exact(&collected, &exact, cancel)?;
        let group_focus = group_focus_medians(&collected, &contexts);
        for (f, context) in collected.into_iter().zip(contexts) {
            let focus = primary_face_detail(&f);
            let reference = context
                .group_id
                .as_ref()
                .and_then(|group_id| group_focus.get(group_id))
                .copied();
            let mut assessment = assess(f, context)?;
            if let (Some((value, confidence)), Some(median)) = (focus, reference) {
                assessment.reasons.push(CullingReason {
                    code: ReasonCode::GroupFocusReference,
                    severity: if median > 0. && value < median * 0.7 {
                        Severity::Review
                    } else {
                        Severity::Info
                    },
                    confidence,
                    subject_index: None,
                    measurement: Some(ReasonMeasurement {
                        value,
                        unit: "normalized_detail".into(),
                        reference: Some(median),
                    }),
                });
            }
            results.insert(assessment.asset_id.clone(), assessment);
        }
        let mut final_results = Vec::new();
        for asset in &assets {
            cancel.check()?;
            let mut result = results
                .remove(&asset.id)
                .ok_or_else(|| internal("Missing assessment"))?;
            // Final membership is new evidence, including for unprocessable sources.
            result.assessment_id = uuid::Uuid::new_v4().to_string();
            result.created_at = chrono::Utc::now().to_rfc3339();
            if let Some(h) = hashes.get(&asset.id) {
                if content::current_stamp(&asset.original_path)? != h.stamp {
                    return Err(ProcessingError::new(
                        ProcessingErrorCode::SourceChanged,
                        "Source changed after duplicate hashing; resume to refresh relationships",
                    ));
                }
            }
            if let Some(f) = &result.features {
                if source(asset)? != f.source_fingerprint
                    || self
                        .analysis
                        .get_analysis(&asset.job_id, &asset.id, r.photo_type)?
                        .analysis
                        .as_ref()
                        .is_none_or(|a| a.analysis_id != f.source_analysis_id)
                {
                    return Err(ProcessingError::new(
                        ProcessingErrorCode::SourceChanged,
                        "Source changed during culling; rerun to refresh groups",
                    ));
                }
            } else if let Some(e) = exact.get(&asset.id) {
                result.similarity.exact = Some(e.clone());
                let redundant = e.canonical_asset_id != asset.id;
                if redundant {
                    result.ai_rating = Some(Stars::new(1).map_err(internal)?);
                    result.final_score = 5.;
                    result.confidence = 1.;
                }
                result.reasons.push(CullingReason {
                    code: if redundant {
                        ReasonCode::ExactDuplicate
                    } else {
                        ReasonCode::PreferredCopy
                    },
                    severity: if redundant {
                        Severity::Major
                    } else {
                        Severity::Positive
                    },
                    confidence: 1.,
                    subject_index: None,
                    measurement: None,
                });
            }
            bind_identity(&mut result, hashes.get(&asset.id), Some(&membership))?;
            let old = self
                .repository
                .culling_state(&r.job_id, &asset.id, r.photo_type)?;
            if r.force
                || old
                    .assessment
                    .as_ref()
                    .is_none_or(|a| a.cache_key != result.cache_key)
            {
                final_results.push(result);
            }
        }
        p.stage = "Saving similarity ratings".into();
        self.repository.save_culling_progress(p)?;
        self.repository
            .persist_culling(&r.job_id, &final_results, cancel)
    }
    fn bound_features(
        &self,
        a: &Asset,
        r: &CullingRequest,
        hash: Option<&content::ContentHash>,
        cancel: &CancellationToken,
    ) -> ProcessingResult<(CullingFeatures, bool)> {
        if let Some(h) = hash {
            let old = self
                .repository
                .culling_state(&a.job_id, &a.id, r.photo_type)?
                .assessment;
            // Bind legacy/unbound analysis to full content once. A size/mtime-preserving byte
            // edit must not reuse Phase 4's path/size/mtime analysis cache.
            if old
                .as_ref()
                .filter(|a| a.features.is_some())
                .and_then(|a| a.duplicate_content.as_ref())
                != Some(&h.content)
            {
                self.analysis.invalidate_analysis(&a.job_id, &a.id)?;
            }
        }
        let result = self.asset_features(a, r, cancel)?;
        if let Some(h) = hash {
            if content::current_stamp(&a.original_path)? != h.stamp {
                return Err(ProcessingError::new(
                    ProcessingErrorCode::SourceChanged,
                    "Source changed between hashing and feature extraction",
                ));
            }
        }
        Ok(result)
    }
    fn asset_features(
        &self,
        a: &Asset,
        r: &CullingRequest,
        cancel: &CancellationToken,
    ) -> ProcessingResult<(CullingFeatures, bool)> {
        let mut analysis = self
            .analysis
            .get_analysis(&a.job_id, &a.id, r.photo_type)?
            .analysis;
        if analysis.is_none() {
            let id = format!("cull-{}", uuid::Uuid::new_v4());
            let permit = self.analysis.reserve(AnalysisRequest {
                job_id: a.job_id.clone(),
                asset_id: a.id.clone(),
                photo_type: r.photo_type,
                request_id: id.clone(),
            })?;
            {
                let mut slot = self.active.lock().map_err(internal)?;
                cancel.check()?;
                if let Some(active) = slot.as_mut() {
                    active.nested = Some(id);
                }
            }
            let result = self.analysis.analyze_asset(permit);
            if let Some(active) = self.active.lock().map_err(internal)?.as_mut() {
                active.nested = None;
            }
            cancel.check()?;
            analysis = result?.analysis;
        }
        let analysis = analysis.ok_or_else(|| internal("No source analysis"))?;
        let key = feature_key(
            &analysis.source_fingerprint,
            &analysis.analysis_id,
            r.photo_type,
            &self.models(r.photo_type),
        );
        if !r.force {
            if let Some(f) = self
                .repository
                .culling_state(&a.job_id, &a.id, r.photo_type)?
                .assessment
                .and_then(|a| a.features)
            {
                if feature_key(
                    &f.source_fingerprint,
                    &f.source_analysis_id,
                    f.photo_type,
                    &f.models,
                ) == key
                    && f.feature_version == features::FEATURE_VERSION
                {
                    return Ok((f, true));
                }
            }
        }
        cancel.check()?;
        let input = self.engine.analysis_input(&a.original_path, cancel)?;
        let f = features::extract(
            &input.image,
            &analysis,
            self.faces.as_ref(),
            self.eyes.as_ref(),
            cancel,
        )?;
        if source(a)? != f.source_fingerprint {
            return Err(ProcessingError::new(
                ProcessingErrorCode::SourceChanged,
                "Source changed during feature extraction",
            ));
        }
        Ok((f, false))
    }
}
fn exact_selection_eligible(item: &CullingItem) -> bool {
    if item
        .similarity
        .as_ref()
        .and_then(|similarity| similarity.exact.as_ref())
        .is_some_and(|exact| exact.canonical_asset_id != item.asset.id)
    {
        return false;
    }
    true
}
fn primary_face_detail(features: &CullingFeatures) -> Option<(f64, f64)> {
    features
        .people
        .faces
        .value()?
        .iter()
        .filter(|face| face.relevant && face.sharpness.confidence() >= 0.7)
        .filter_map(|face| {
            face.sharpness
                .value()
                .map(|detail| (*detail, face.sharpness.confidence()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}
fn group_focus_medians(
    features: &[CullingFeatures],
    contexts: &[SimilarityContext],
) -> HashMap<String, f64> {
    let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
    for (features, context) in features.iter().zip(contexts) {
        if let (Some(group_id), Some((detail, _))) =
            (context.group_id.as_ref(), primary_face_detail(features))
        {
            groups.entry(group_id.clone()).or_default().push(detail);
        }
    }
    groups
        .into_iter()
        .filter_map(|(group_id, mut values)| {
            values.sort_by(f64::total_cmp);
            let len = values.len();
            let median = match len {
                0 => return None,
                n if n % 2 == 0 => (values[n / 2 - 1] + values[n / 2]) / 2.,
                n => values[n / 2],
            };
            Some((group_id, median))
        })
        .collect()
}
fn bind_identity(
    a: &mut CullingAssessment,
    hash: Option<&content::ContentHash>,
    membership: Option<&str>,
) -> ProcessingResult<()> {
    a.duplicate_content = hash.map(|h| h.content.clone());
    a.duplicate_stamp = hash.map(|h| h.stamp.clone());
    a.membership_key = membership.map(str::to_owned);
    if hash.is_none()
        && !a
            .reasons
            .iter()
            .any(|r| r.code == ReasonCode::DuplicateIdentityUnavailable)
    {
        a.reasons.push(CullingReason {
            code: ReasonCode::DuplicateIdentityUnavailable,
            severity: Severity::Review,
            confidence: 0.,
            subject_index: None,
            measurement: None,
        });
    }
    a.cache_key = assessment_key(a);
    a.validate().map_err(internal)
}
fn assessment_key(a: &CullingAssessment) -> String {
    digest(&[
        &feature_key(
            &a.source_fingerprint,
            a.source_analysis_id.as_deref().unwrap_or("unavailable"),
            a.photo_type,
            &a.model_versions,
        ),
        &serde_json::to_string(&a.similarity).expect("serializable similarity"),
        &serde_json::to_string(&a.duplicate_content).expect("serializable content"),
        a.duplicate_stamp.as_deref().unwrap_or("unverified"),
        a.membership_key.as_deref().unwrap_or("not-grouped"),
    ])
}
pub fn assess(
    features: CullingFeatures,
    similarity: SimilarityContext,
) -> ProcessingResult<CullingAssessment> {
    let scored = score::score(&features, &similarity).map_err(internal)?;
    let mut a = CullingAssessment {
        schema_version: CULLING_SCHEMA_VERSION,
        assessment_id: uuid::Uuid::new_v4().to_string(),
        asset_id: features.asset_id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        photo_type: features.photo_type,
        ai_rating: Some(scored.rating),
        confidence: scored.confidence,
        absolute_score: scored.absolute_score,
        final_score: scored.score,
        reasons: scored.reasons,
        duplicate_content: similarity.exact.as_ref().map(|e| e.content.clone()),
        similarity,
        culling_engine_version: score::CULLING_ENGINE_VERSION.into(),
        model_versions: features.models.clone(),
        source_analysis_id: Some(features.source_analysis_id.clone()),
        source_fingerprint: features.source_fingerprint.clone(),
        cache_key: String::new(),
        features: Some(features),
        duplicate_stamp: None,
        membership_key: None,
    };
    a.cache_key = assessment_key(&a);
    a.validate().map_err(internal)?;
    Ok(a)
}

fn membership_key(assets: &[Asset]) -> String {
    let mut ids: Vec<_> = assets.iter().map(|a| a.id.as_str()).collect();
    ids.sort();
    digest(&["culling-job-members-v1", &ids.join("|")])
}
fn classified_kind(a: &CullingAssessment) -> Option<DuplicateKind> {
    if a.membership_key.is_none() {
        None
    } else if a.similarity.exact.is_some() {
        Some(DuplicateKind::Exact)
    } else if a.similarity.kind != DuplicateKind::Unique
        || a.duplicate_content.is_some() && a.features.is_some()
    {
        Some(a.similarity.kind)
    } else {
        None
    }
}
fn duplicate_counts(items: &[CullingItem]) -> DuplicateCounts {
    let mut c = DuplicateCounts::default();
    let mut exact = std::collections::HashSet::new();
    let mut near = std::collections::HashSet::new();
    let mut burst = std::collections::HashSet::new();
    let mut similar = std::collections::HashSet::new();
    for i in items {
        match i.relationship_kind {
            None => c.unclassified_images += 1,
            Some(DuplicateKind::Unique) => c.unique_images += 1,
            _ => (),
        };
        if let Some(s) = &i.similarity {
            if let Some(e) = &s.exact {
                exact.insert(&e.group_id);
                if e.canonical_asset_id != i.asset.id {
                    c.exact_copies += 1;
                }
            }
            if let Some(id) = &s.group_id {
                match s.kind {
                    DuplicateKind::NearDuplicate => {
                        near.insert(id);
                    }
                    DuplicateKind::Burst => {
                        burst.insert(id);
                    }
                    DuplicateKind::Similar => {
                        similar.insert(id);
                    }
                    _ => (),
                }
            }
        }
    }
    c.exact_groups = exact.len() as u32;
    c.near_groups = near.len() as u32;
    c.burst_groups = burst.len() as u32;
    c.similar_groups = similar.len() as u32;
    c
}
fn unrated(
    asset: &Asset,
    kind: PhotoType,
    models: &[ProviderIdentity],
    _error: &str,
) -> CullingAssessment {
    let mut a = CullingAssessment {
        schema_version: CULLING_SCHEMA_VERSION,
        assessment_id: uuid::Uuid::new_v4().to_string(),
        asset_id: asset.id.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        photo_type: kind,
        ai_rating: None,
        confidence: 0.,
        absolute_score: 0.,
        final_score: 0.,
        reasons: vec![CullingReason {
            code: ReasonCode::SourceUnavailable,
            severity: Severity::Review,
            confidence: 0.,
            subject_index: None,
            measurement: None,
        }],
        features: None,
        similarity: SimilarityContext::default(),
        culling_engine_version: score::CULLING_ENGINE_VERSION.into(),
        model_versions: models.to_vec(),
        source_analysis_id: None,
        source_fingerprint: source(asset)
            .unwrap_or_else(|_| digest(&["unavailable", &asset.fingerprint])),
        cache_key: String::new(),
        duplicate_content: None,
        duplicate_stamp: None,
        membership_key: None,
    };
    a.cache_key = assessment_key(&a);
    a
}
