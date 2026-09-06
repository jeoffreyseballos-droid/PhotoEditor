//! Deterministic built-in creative styles resolved into authoritative edit recipes.
use crate::{culling::MAX_BATCH, rendering::internal, repository::JobRepository};
use photo_contracts::{
    EditRecipe, LocalAdjustments, MaskType, ProcessingError, ProcessingErrorCode, ProcessingResult,
    RecipeGlobal, RecipeLayer, RecipeOrigin, RecipeProvenance,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const BUILT_IN_PRESET_SOURCE: &str = "photo-editor/built-in-preset";
pub const BUILT_IN_PRESET_VERSION: &str = "1";
pub const POP_SUBJECT_LAYER_ID: &str = "built-in-pop-subject-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInPresetId {
    Pop,
    Warm,
    BlackAndWhite,
}
impl BuiltInPresetId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pop => "pop",
            Self::Warm => "warm",
            Self::BlackAndWhite => "black_and_white",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BuiltInPreset {
    pub id: BuiltInPresetId,
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresetEditingState {
    pub selected_asset_ids: Vec<String>,
    pub applied_preset: Option<BuiltInPresetId>,
    pub applied_count: u32,
    #[serde(default)]
    pub unresolved_subject_masks: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PresetApplyResult {
    pub preset: BuiltInPreset,
    pub selected_asset_ids: Vec<String>,
    pub recipes_updated: u32,
    pub recipes_unchanged: u32,
    #[serde(default)]
    pub unresolved_subject_masks: Vec<String>,
}

pub fn built_in_presets() -> Vec<BuiltInPreset> {
    vec![
        BuiltInPreset {
            id: BuiltInPresetId::Pop,
            name: "POP".into(),
            description: "Bright, clean subject emphasis".into(),
            version: BUILT_IN_PRESET_VERSION.into(),
        },
        BuiltInPreset {
            id: BuiltInPresetId::Warm,
            name: "WARM".into(),
            description: "Warmer overall color balance".into(),
            version: BUILT_IN_PRESET_VERSION.into(),
        },
        BuiltInPreset {
            id: BuiltInPresetId::BlackAndWhite,
            name: "BLACK & WHITE".into(),
            description: "Classic monochrome".into(),
            version: BUILT_IN_PRESET_VERSION.into(),
        },
    ]
}

pub fn preset_definition(id: BuiltInPresetId) -> BuiltInPreset {
    built_in_presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .expect("all built-in preset IDs have a definition")
}

/// Replaces creative intent while retaining per-asset identity, objective optics/geometry,
/// and non-creative recipe metadata. This boundary can later be shared by trained styles.
pub fn resolve_built_in_preset(
    current: &EditRecipe,
    id: BuiltInPresetId,
) -> ProcessingResult<EditRecipe> {
    let current = current.validated()?;
    let optics = current.global.optics;
    let geometry = current.global.geometry.clone();
    let mut recipe = current;
    recipe.global = RecipeGlobal {
        optics,
        geometry,
        ..Default::default()
    };
    recipe.local_layers.clear();
    recipe.provenance = RecipeProvenance {
        origin: RecipeOrigin::System,
        created_by: Some(BUILT_IN_PRESET_SOURCE.into()),
        source_recipe_id: None,
        style_id: Some(id.as_str().into()),
        model_id: Some("built-in-preset".into()),
        model_version: Some(BUILT_IN_PRESET_VERSION.into()),
        analysis_id: None,
        style_version: Some(BUILT_IN_PRESET_VERSION.into()),
        style_package_id: None,
        feature_schema_version: None,
        batch_context_id: None,
        batch_context_version: None,
        photo_analysis_version: None,
        manually_modified: false,
        acceptance: None,
    };
    match id {
        BuiltInPresetId::Pop => recipe.local_layers.push(RecipeLayer {
            id: POP_SUBJECT_LAYER_ID.into(),
            mask_type: MaskType::Subject,
            enabled: true,
            strength: 1.0,
            invert: false,
            mask_reference: None,
            confidence: None,
            adjustments: LocalAdjustments {
                exposure_ev: 0.35,
                ..Default::default()
            },
        }),
        BuiltInPresetId::Warm => {
            // The renderer's relative-WB control is neutral at 6500, so 7000 is +500 K
            // relative to every photograph's own camera/source white balance.
            recipe.global.basic.temperature = 7000.0;
            recipe.global.basic.tint = 2.0;
            recipe.global.basic.vibrance = 4.0;
        }
        BuiltInPresetId::BlackAndWhite => recipe.global.basic.saturation = -100.0,
    }
    recipe.validated().map_err(Into::into)
}

pub fn applied_built_in_preset(recipe: &EditRecipe) -> Option<BuiltInPresetId> {
    let provenance = &recipe.provenance;
    if provenance.origin != RecipeOrigin::System
        || provenance.created_by.as_deref() != Some(BUILT_IN_PRESET_SOURCE)
        || provenance.model_id.as_deref() != Some("built-in-preset")
        || provenance.model_version.as_deref() != Some(BUILT_IN_PRESET_VERSION)
        || provenance.manually_modified
    {
        return None;
    }
    let id = match provenance.style_id.as_deref()? {
        "pop" => Some(BuiltInPresetId::Pop),
        "warm" => Some(BuiltInPresetId::Warm),
        "black_and_white" => Some(BuiltInPresetId::BlackAndWhite),
        _ => None,
    }?;
    // Provenance is informative, but the current payload must also still equal the
    // resolver's deterministic output. This prevents a forged/stale tag from making
    // an edited recipe appear to be an applied built-in preset.
    resolve_built_in_preset(recipe, id)
        .ok()
        .filter(|expected| expected == recipe)
        .map(|_| id)
}

impl JobRepository {
    pub fn selected_editing_asset_ids(&self, job: &str) -> ProcessingResult<Vec<String>> {
        self.get_job(job).map_err(internal)?;
        let db = self.connect().map_err(internal)?;
        let mut statement = db
            .prepare(
                "SELECT a.id FROM assets a JOIN culling_user_state u ON u.job_id=a.job_id AND u.asset_id=a.id WHERE a.job_id=?1 AND u.selected=1 ORDER BY a.filename COLLATE NOCASE,a.id LIMIT ?2",
            )
            .map_err(internal)?;
        let ids = statement
            .query_map(params![job, MAX_BATCH as u32 + 1], |row| row.get(0))
            .map_err(internal)?
            .collect::<Result<Vec<String>, _>>()
            .map_err(internal)?;
        if ids.len() > MAX_BATCH {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Editing selection exceeds the supported batch size",
            ));
        }
        Ok(ids)
    }

    pub fn preset_editing_state(&self, job: &str) -> ProcessingResult<PresetEditingState> {
        let selected_asset_ids = self.selected_editing_asset_ids(job)?;
        let mut shared = None;
        let mut applied_count = 0u32;
        for asset in &selected_asset_ids {
            let state = self.get_recipe(job, asset)?;
            if let Some(error) = state.error {
                return Err(error.into());
            }
            let preset = applied_built_in_preset(&state.recipe);
            match (shared, preset) {
                (None, Some(id)) if applied_count == 0 => {
                    shared = Some(id);
                    applied_count = 1;
                }
                (Some(expected), Some(id)) if expected == id => applied_count += 1,
                _ => shared = None,
            }
        }
        if applied_count as usize != selected_asset_ids.len() {
            shared = None;
            applied_count = 0;
        }
        Ok(PresetEditingState {
            selected_asset_ids,
            applied_preset: shared,
            applied_count,
            unresolved_subject_masks: Vec::new(),
        })
    }

    /// Applies only to the explicit current editing snapshot. A stale caller cannot
    /// accidentally expand or merge the scope after the photographer changes selection.
    pub fn apply_built_in_preset_to_assets(
        &self,
        job: &str,
        id: BuiltInPresetId,
        asset_ids: &[String],
    ) -> ProcessingResult<PresetApplyResult> {
        if asset_ids.len() > MAX_BATCH {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Editing selection exceeds the supported batch size",
            ));
        }
        let requested = asset_ids.iter().map(String::as_str).collect::<HashSet<_>>();
        if requested.len() != asset_ids.len() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Editing selection contains duplicate asset IDs",
            ));
        }
        let persisted = self.selected_editing_asset_ids(job)?;
        let persisted_ids = persisted.iter().map(String::as_str).collect::<HashSet<_>>();
        if requested != persisted_ids {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Editing selection changed. Return to culling or reload editing before applying a preset",
            ));
        }
        let selected_asset_ids = asset_ids.to_vec();
        if selected_asset_ids.is_empty() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Select at least one photograph before running it for editing",
            ));
        }
        // Resolve and validate the complete batch before changing any current recipe.
        let mut resolved = Vec::with_capacity(selected_asset_ids.len());
        for asset in &selected_asset_ids {
            let state = self.get_recipe(job, asset)?;
            if let Some(error) = state.error.clone() {
                return Err(error.into());
            }
            let recipe = resolve_built_in_preset(&state.recipe, id)?;
            resolved.push((asset.clone(), state, recipe));
        }
        let mut recipes_updated = 0u32;
        let mut recipes_unchanged = 0u32;
        for (asset, state, recipe) in resolved {
            if recipe == state.recipe {
                recipes_unchanged += 1;
                continue;
            }
            self.save_recipe(
                job,
                &asset,
                &recipe,
                state.generation,
                Some(crate::recipes::RevisionReason::BuiltInPreset),
            )?;
            recipes_updated += 1;
        }
        Ok(PresetApplyResult {
            preset: preset_definition(id),
            selected_asset_ids,
            recipes_updated,
            recipes_unchanged,
            unresolved_subject_masks: Vec::new(),
        })
    }
}
