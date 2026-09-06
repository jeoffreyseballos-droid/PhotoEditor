//! One bounded progress snapshot. Candidate validation is published atomically.
use super::*;
use std::sync::MutexGuard;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchingProgress {
    pub request_id: String,
    pub dataset_id: String,
    pub status: String,
    pub stage: String,
    pub processed: usize,
    pub total: usize,
    pub error: Option<String>,
}

#[derive(Default)]
pub(super) struct MatchingSlot {
    progress: Option<MatchingProgress>,
    token: Option<CancellationToken>,
}

impl TrainingService {
    pub(super) fn idle_matching(&self) -> ProcessingResult<MutexGuard<'_, MatchingSlot>> {
        let slot = self.matching.lock().map_err(internal)?;
        if slot.token.is_some() {
            return Err(internal("Dataset matching is already running"));
        }
        Ok(slot)
    }

    pub fn matching_progress(
        &self,
        request_id: &str,
    ) -> ProcessingResult<Option<MatchingProgress>> {
        Ok(self
            .matching
            .lock()
            .map_err(internal)?
            .progress
            .clone()
            .filter(|p| p.request_id == request_id))
    }

    pub fn cancel_matching(&self, request_id: &str) -> ProcessingResult<()> {
        let slot = self.matching.lock().map_err(internal)?;
        if slot
            .progress
            .as_ref()
            .is_some_and(|p| p.request_id == request_id)
        {
            if let Some(token) = &slot.token {
                token.cancel();
            }
        }
        Ok(())
    }

    fn matching_stage(&self, stage: &str, processed: usize, total: usize) -> ProcessingResult<()> {
        let mut slot = self.matching.lock().map_err(internal)?;
        if let Some(token) = &slot.token {
            token.check()?;
        }
        if let Some(progress) = &mut slot.progress {
            progress.stage = stage.into();
            progress.processed = processed;
            progress.total = total;
        }
        Ok(())
    }

    pub fn match_and_validate(
        &self,
        dataset_id: &str,
        request_id: &str,
    ) -> ProcessingResult<TrainingDataset> {
        if request_id.is_empty() || request_id.len() > 128 {
            return Err(internal("Invalid matching request ID"));
        }
        let token = CancellationToken::default();
        {
            let mut slot = self.idle_matching()?;
            if self.active.lock().map_err(internal)?.is_some() {
                return Err(internal("Training is already running"));
            }
            slot.progress = Some(MatchingProgress {
                request_id: request_id.into(),
                dataset_id: dataset_id.into(),
                status: "running".into(),
                stage: "scanning_before".into(),
                processed: 0,
                total: 0,
                error: None,
            });
            slot.token = Some(token.clone());
        }
        // Catch worker panics as failures so the reservation cannot remain stuck.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.match_work(dataset_id, &token)
        }))
        .unwrap_or_else(|_| Err(internal("Dataset matching worker failed")));
        let mut slot = self.matching.lock().map_err(internal)?;
        // Cancellation and publication share this lock: no half-validated dataset
        // is ever stored, and cancellation before publication retains prior state.
        let result = result.and_then(|dataset| {
            token.check()?;
            self.repository.save_training_dataset(&dataset)?;
            Ok(dataset)
        });
        if let Some(progress) = &mut slot.progress {
            match &result {
                Ok(dataset) => {
                    progress.status = "complete".into();
                    progress.stage = "complete".into();
                    progress.processed = dataset.pairs.len();
                    progress.total = dataset.pairs.len();
                }
                Err(error) => {
                    progress.status = if token.check().is_err() {
                        "cancelled"
                    } else {
                        "failed"
                    }
                    .into();
                    progress.error = Some(error.message.clone());
                }
            }
        }
        slot.token = None;
        result
    }

    fn match_work(
        &self,
        dataset_id: &str,
        token: &CancellationToken,
    ) -> ProcessingResult<TrainingDataset> {
        let mut dataset = self.dataset(dataset_id)?;
        for (stage, paths) in [
            ("scanning_before", &dataset.before_files),
            ("scanning_after", &dataset.after_files),
        ] {
            self.matching_stage(stage, 0, paths.len())?;
            for (i, path) in paths.iter().enumerate() {
                token.check()?;
                if !path.is_file() {
                    return Err(internal(format!(
                        "Image is unavailable: {}",
                        path.display()
                    )));
                }
                self.matching_stage(stage, i + 1, paths.len())?;
            }
        }
        if dataset.before_files.is_empty() || dataset.after_files.is_empty() {
            return Err(internal("Add Before and After images first"));
        }
        self.matching_stage(
            "sorting",
            0,
            dataset.before_files.len() + dataset.after_files.len(),
        )?;
        dataset
            .before_files
            .sort_by(|a, b| matcher::natural_cmp(a, b));
        dataset
            .after_files
            .sort_by(|a, b| matcher::natural_cmp(a, b));
        self.matching_stage("building_pair_candidates", 0, dataset.before_files.len())?;
        let matching = matcher::match_paths(&dataset.before_files, &dataset.after_files);
        let old = std::mem::take(&mut dataset.pairs);
        for (i, candidate) in matching.matched.iter().enumerate() {
            token.check()?;
            let source_path = candidate
                .source_path
                .as_ref()
                .ok_or_else(|| internal("Missing candidate source"))?;
            if matching.order_fallback_used
                && old.iter().any(|pair| {
                    pair.diagnostics.iter().any(|d| d == "Manual pairing")
                        && (&pair.source_path == source_path
                            || pair.reference_path == candidate.reference_path)
                })
            {
                self.matching_stage("building_pair_candidates", i + 1, matching.matched.len())?;
                continue;
            }
            let source = self.ensure_training_asset(&dataset, source_path)?;
            let mut pair = self.pair_from_paths_with_token(
                &dataset,
                source.id,
                source_path.clone(),
                candidate.reference_path.clone(),
                token,
            )?;
            if let Some(previous) = old.iter().find(|p| {
                p.source_path == pair.source_path
                    && p.reference_path == pair.reference_path
                    && p.source_fingerprint == pair.source_fingerprint
                    && p.reference_fingerprint == pair.reference_fingerprint
            }) {
                pair = previous.clone();
            }
            dataset.pairs.push(pair);
            self.matching_stage("building_pair_candidates", i + 1, matching.matched.len())?;
        }
        // Retain explicit manual mappings that don't conflict with automatic ones.
        for pair in old {
            if pair.diagnostics.iter().any(|d| d == "Manual pairing")
                && !dataset.pairs.iter().any(|p| {
                    p.source_path == pair.source_path || p.reference_path == pair.reference_path
                })
            {
                dataset.pairs.push(pair);
            }
        }
        let total = dataset.pairs.len();
        self.matching_stage("structural_validation", 0, total)?;
        for (i, pair) in dataset.pairs.iter_mut().enumerate() {
            token.check()?;
            pair.validation = match self.optimizer.validate_pair(pair, token) {
                Ok(validation) => validation,
                Err(error) => {
                    token.check()?;
                    PairValidation {
                        status: PairValidationStatus::Unusable,
                        diagnostics: vec![error.message],
                        ..Default::default()
                    }
                }
            };
            self.matching_stage("structural_validation", i + 1, total)?;
        }
        self.matching_stage("finalizing_matches", total, total)?;
        let strong = |before: Option<&PathBuf>, after: Option<&PathBuf>| {
            dataset.pairs.iter().any(|p| {
                Some(&p.source_path) == before
                    && Some(&p.reference_path) == after
                    && p.validation.status == PairValidationStatus::Ready
                    && p.validation.geometry == GeometryRelationship::ExactOrNear
            })
        };
        dataset.alignment = Some(TrainingAlignment {
            before_count: dataset.before_files.len() as u32,
            after_count: dataset.after_files.len() as u32,
            matched_count: dataset
                .pairs
                .iter()
                .filter(|p| p.validation.status == PairValidationStatus::Ready)
                .count() as u32,
            ambiguous_count: matching
                .ambiguous_sources
                .iter()
                .filter(|source| {
                    !dataset
                        .pairs
                        .iter()
                        .any(|pair| pair.source_path.to_string_lossy() == source.as_str())
                })
                .count() as u32,
            unmatched_before: dataset
                .before_files
                .iter()
                .filter(|path| !dataset.pairs.iter().any(|p| &p.source_path == *path))
                .cloned()
                .collect(),
            unmatched_after: dataset
                .after_files
                .iter()
                .filter(|path| !dataset.pairs.iter().any(|p| &p.reference_path == *path))
                .cloned()
                .collect(),
            first_before: dataset.before_files.first().cloned(),
            first_after: dataset.after_files.first().cloned(),
            last_before: dataset.before_files.last().cloned(),
            last_after: dataset.after_files.last().cloned(),
            start_aligned: strong(dataset.before_files.first(), dataset.after_files.first()),
            end_aligned: strong(dataset.before_files.last(), dataset.after_files.last()),
            order_fallback_used: matching.order_fallback_used,
            diagnostics: matching.diagnostics,
        });
        dataset.updated_at = chrono::Utc::now().to_rfc3339();
        dataset.dataset_fingerprint = Some(dataset_identity(&dataset));
        Ok(dataset)
    }
}
