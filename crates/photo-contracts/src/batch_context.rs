//! Relationships among source photographs in one explicit editing selection.
//! This contract contains context only: it deliberately has no recipe or edit fields.
use crate::analysis::PhotoType;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const BATCH_CONTEXT_SCHEMA_VERSION: u32 = 1;
pub const MAX_BATCH_CONTEXT_ASSETS: usize = 5_000;
pub const MAX_BATCH_CONTEXT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextAvailability {
    Available,
    Partial,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchGroupKind {
    Scene,
    Lighting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceKind {
    Burst,
    ExposureBracket,
    RepeatedFrames,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyNoteCode {
    ExposureReference,
    NearExposureMedian,
    DarkerThanGroup,
    BrighterThanGroup,
    NearWhiteBalanceMedian,
    WarmerThanGroup,
    CoolerThanGroup,
    GreenerThanGroup,
    MoreMagentaThanGroup,
    BracketMember,
    PartialEvidence,
    AnalysisUnavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsistencyNote {
    pub code: ConsistencyNoteCode,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureRelationship {
    /// Signed source relationship to the lighting-group median. Negative is darker.
    pub delta_ev: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhiteBalanceRelationship {
    /// Signed differences in the Phase 4 source-observation axes, not Kelvin/tint edits.
    pub warm_cool_delta: f64,
    pub green_magenta_delta: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchGroup {
    pub group_id: String,
    pub asset_ids: Vec<String>,
    pub confidence: f64,
    pub reference_candidate_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceGroup {
    pub group_id: String,
    pub asset_ids: Vec<String>,
    pub kind: SequenceKind,
    pub confidence: f64,
    /// Existing Phase 5 relationship identity when the sequence came from culling evidence.
    pub source_culling_group_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceCandidate {
    pub group_kind: BatchGroupKind,
    pub group_id: String,
    pub asset_id: String,
    pub rank: u32,
    pub technical_score: f64,
    pub confidence: f64,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetBatchContext {
    pub asset_id: String,
    pub availability: ContextAvailability,
    pub scene_group_id: Option<String>,
    pub lighting_group_id: Option<String>,
    pub sequence_group_id: Option<String>,
    pub reference_asset_id: Option<String>,
    pub exposure_delta_from_group: Option<ExposureRelationship>,
    pub wb_delta_from_group: Option<WhiteBalanceRelationship>,
    pub group_confidence: f64,
    pub consistency_notes: Vec<ConsistencyNote>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchStageTimings {
    pub loading_ms: u64,
    pub candidate_generation_ms: u64,
    pub grouping_ms: u64,
    pub context_ms: u64,
    pub persistence_ms: u64,
    pub total_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchDiagnostics {
    pub available_assets: u32,
    pub partial_assets: u32,
    pub unavailable_assets: u32,
    pub candidate_comparisons: u64,
    pub candidate_limit_per_asset: u32,
    pub timings: BatchStageTimings,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchContext {
    pub schema_version: u32,
    pub batch_id: String,
    pub job_id: String,
    pub photo_type: PhotoType,
    /// Canonically sorted; UI ordering never affects identity.
    pub selected_asset_ids: Vec<String>,
    pub selection_identity: String,
    pub created_at: String,
    pub analysis_version: String,
    pub grouping_version: String,
    pub scene_groups: Vec<BatchGroup>,
    pub lighting_groups: Vec<BatchGroup>,
    pub sequence_groups: Vec<SequenceGroup>,
    pub asset_contexts: Vec<AssetBatchContext>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostics: BatchDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum BatchContextError {
    #[error("Unsupported batch-context version: {0}")]
    UnsupportedVersion(u32),
    #[error("Invalid batch context: {0}")]
    Invalid(String),
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn ids_are_valid(ids: &[String], selected: &HashSet<&str>, minimum: usize) -> bool {
    ids.len() >= minimum
        && ids.len() <= MAX_BATCH_CONTEXT_ASSETS
        && ids
            .iter()
            .all(|id| !id.is_empty() && id.len() <= 128 && selected.contains(id.as_str()))
        && ids.windows(2).all(|pair| pair[0] < pair[1])
}

impl BatchContext {
    pub fn parse(json: &str) -> Result<Self, BatchContextError> {
        if json.len() > MAX_BATCH_CONTEXT_BYTES {
            return Err(BatchContextError::Invalid(
                "Payload exceeds size limit".into(),
            ));
        }
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| BatchContextError::Invalid(error.to_string()))?;
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| BatchContextError::Invalid("Missing schema version".into()))?;
        if version != u64::from(BATCH_CONTEXT_SCHEMA_VERSION) {
            return Err(BatchContextError::UnsupportedVersion(
                version.min(u64::from(u32::MAX)) as u32,
            ));
        }
        let context: Self = serde_json::from_value(value)
            .map_err(|error| BatchContextError::Invalid(error.to_string()))?;
        context.validate()?;
        Ok(context)
    }

    pub fn validate(&self) -> Result<(), BatchContextError> {
        let bad = |message: &str| BatchContextError::Invalid(message.into());
        if self.schema_version != BATCH_CONTEXT_SCHEMA_VERSION {
            return Err(BatchContextError::UnsupportedVersion(self.schema_version));
        }
        if !is_digest(&self.batch_id)
            || !is_digest(&self.selection_identity)
            || self.job_id.is_empty()
            || self.job_id.len() > 128
            || self.analysis_version.is_empty()
            || self.analysis_version.len() > 256
            || self.grouping_version.is_empty()
            || self.grouping_version.len() > 256
            || chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err()
        {
            return Err(bad("Invalid envelope"));
        }
        if self.selected_asset_ids.is_empty()
            || self.selected_asset_ids.len() > MAX_BATCH_CONTEXT_ASSETS
            || self
                .selected_asset_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 128)
            || self
                .selected_asset_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(bad("Selected asset IDs must be unique and sorted"));
        }
        let selected = self
            .selected_asset_ids
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let validate_group = |group: &BatchGroup| {
            is_digest(&group.group_id)
                && ids_are_valid(&group.asset_ids, &selected, 1)
                && group.confidence.is_finite()
                && (0. ..=1.).contains(&group.confidence)
                && group.reference_candidate_ids.len() <= 3
                && group
                    .reference_candidate_ids
                    .iter()
                    .all(|id| group.asset_ids.binary_search(id).is_ok())
                && group
                    .reference_candidate_ids
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
        };
        if self.scene_groups.iter().any(|group| !validate_group(group))
            || self
                .lighting_groups
                .iter()
                .any(|group| !validate_group(group))
        {
            return Err(bad("Invalid group"));
        }
        let scenes = self
            .scene_groups
            .iter()
            .map(|group| (group.group_id.as_str(), group))
            .collect::<HashMap<_, _>>();
        let lighting = self
            .lighting_groups
            .iter()
            .map(|group| (group.group_id.as_str(), group))
            .collect::<HashMap<_, _>>();
        let sequences = self
            .sequence_groups
            .iter()
            .map(|group| (group.group_id.as_str(), group))
            .collect::<HashMap<_, _>>();
        if scenes.len() != self.scene_groups.len()
            || lighting.len() != self.lighting_groups.len()
            || sequences.len() != self.sequence_groups.len()
        {
            return Err(bad("Duplicate group identity"));
        }
        for sequence in &self.sequence_groups {
            if !is_digest(&sequence.group_id)
                || !ids_are_valid(&sequence.asset_ids, &selected, 2)
                || !sequence.confidence.is_finite()
                || !(0. ..=1.).contains(&sequence.confidence)
                || sequence
                    .source_culling_group_id
                    .as_ref()
                    .is_some_and(|id| !is_digest(id))
            {
                return Err(bad("Invalid sequence group"));
            }
        }
        if self.asset_contexts.len() != selected.len() {
            return Err(bad("Missing per-asset context"));
        }
        let mut context_ids = HashSet::new();
        for context in &self.asset_contexts {
            if !selected.contains(context.asset_id.as_str())
                || !context_ids.insert(context.asset_id.as_str())
                || !context.group_confidence.is_finite()
                || !(0. ..=1.).contains(&context.group_confidence)
                || context
                    .scene_group_id
                    .as_deref()
                    .is_some_and(|id| !scenes.contains_key(id))
                || context
                    .lighting_group_id
                    .as_deref()
                    .is_some_and(|id| !lighting.contains_key(id))
                || context
                    .sequence_group_id
                    .as_deref()
                    .is_some_and(|id| !sequences.contains_key(id))
                || context
                    .reference_asset_id
                    .as_deref()
                    .is_some_and(|id| !selected.contains(id))
                || context.consistency_notes.len() > 16
                || context
                    .consistency_notes
                    .iter()
                    .any(|note| note.message.is_empty() || note.message.len() > 512)
            {
                return Err(bad("Invalid per-asset context"));
            }
            if let Some(exposure) = &context.exposure_delta_from_group {
                if !exposure.delta_ev.is_finite()
                    || exposure.delta_ev.abs() > 16.
                    || !exposure.confidence.is_finite()
                    || !(0. ..=1.).contains(&exposure.confidence)
                {
                    return Err(bad("Invalid exposure relationship"));
                }
            }
            if let Some(wb) = &context.wb_delta_from_group {
                if !wb.warm_cool_delta.is_finite()
                    || !wb.green_magenta_delta.is_finite()
                    || wb.warm_cool_delta.abs() > 2.
                    || wb.green_magenta_delta.abs() > 2.
                    || !wb.confidence.is_finite()
                    || !(0. ..=1.).contains(&wb.confidence)
                {
                    return Err(bad("Invalid white-balance relationship"));
                }
            }
        }
        let mut candidate_keys = HashSet::new();
        for candidate in &self.reference_candidates {
            let group = match candidate.group_kind {
                BatchGroupKind::Scene => scenes.get(candidate.group_id.as_str()),
                BatchGroupKind::Lighting => lighting.get(candidate.group_id.as_str()),
            };
            if group.is_none_or(|group| {
                group.asset_ids.binary_search(&candidate.asset_id).is_err()
                    || group
                        .reference_candidate_ids
                        .binary_search(&candidate.asset_id)
                        .is_err()
            }) || candidate.rank == 0
                || candidate.rank > 3
                || !candidate.technical_score.is_finite()
                || !(0. ..=100.).contains(&candidate.technical_score)
                || !candidate.confidence.is_finite()
                || !(0. ..=1.).contains(&candidate.confidence)
                || candidate.reasons.is_empty()
                || candidate.reasons.len() > 8
                || candidate
                    .reasons
                    .iter()
                    .any(|reason| reason.is_empty() || reason.len() > 256)
                || !candidate_keys.insert((
                    candidate.group_kind,
                    candidate.group_id.as_str(),
                    candidate.asset_id.as_str(),
                ))
            {
                return Err(bad("Invalid reference candidate"));
            }
        }
        let diagnostics_total = u64::from(self.diagnostics.available_assets)
            + u64::from(self.diagnostics.partial_assets)
            + u64::from(self.diagnostics.unavailable_assets);
        if diagnostics_total != self.selected_asset_ids.len() as u64
            || self.diagnostics.candidate_limit_per_asset == 0
            || self.diagnostics.candidate_limit_per_asset > 256
            || self.diagnostics.warnings.len() > 256
            || self
                .diagnostics
                .warnings
                .iter()
                .any(|warning| warning.is_empty() || warning.len() > 1024)
        {
            return Err(bad("Invalid diagnostics"));
        }
        let json = serde_json::to_string(self)
            .map_err(|error| BatchContextError::Invalid(error.to_string()))?;
        if json.len() > MAX_BATCH_CONTEXT_BYTES {
            return Err(bad("Payload exceeds size limit"));
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<String, BatchContextError> {
        self.validate()?;
        serde_json::to_string(
            &serde_json::to_value(self)
                .map_err(|error| BatchContextError::Invalid(error.to_string()))?,
        )
        .map_err(|error| BatchContextError::Invalid(error.to_string()))
    }
}
