//! Local recipe lifecycle, independently usable without React/Tauri.
use crate::{
    rendering::{internal, io_error},
    repository::JobRepository,
};
use photo_contracts::*;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Initial evidence plus the most recent 199 meaningful snapshots. Drafts are not history.
pub const MAX_REVISIONS: u32 = 200;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeState {
    pub recipe: EditRecipe,
    pub recipe_hash: String,
    pub generation: u64,
    pub current_revision: u64,
    pub modified: bool,
    pub error: Option<RecipeError>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionReason {
    Initial,
    Migration,
    ManualEdit,
    Preview,
    Export,
    Reset,
    Restore,
    Imported,
    Snapshot,
    BuiltInPreset,
}
impl RevisionReason {
    fn name(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Migration => "migration",
            Self::ManualEdit => "manual_edit",
            Self::Preview => "preview",
            Self::Export => "export",
            Self::Reset => "reset",
            Self::Restore => "restore",
            Self::Imported => "imported",
            Self::Snapshot => "snapshot",
            Self::BuiltInPreset => "built_in_preset",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeRevision {
    pub revision_id: String,
    pub revision_number: u64,
    pub recipe_hash: String,
    pub origin: String,
    pub reason: String,
    pub created_at: String,
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn origin(r: &EditRecipe) -> ProcessingResult<String> {
    serde_json::to_value(r.provenance.origin)
        .map_err(internal)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| internal("Invalid origin"))
}
fn conflict() -> ProcessingError {
    RecipeError::new(
        RecipeErrorCode::Conflict,
        "Recipe changed since it was loaded. Reload before saving.",
    )
    .into()
}
fn corrupt(e: impl std::fmt::Display) -> RecipeError {
    RecipeError::new(RecipeErrorCode::CorruptStoredRecipe, format!("Stored recipe could not be read: {e}. Original payload retained. Reset All, import, or restore a revision to recover."))
}
fn archive(
    tx: &Transaction<'_>,
    job: &str,
    asset: &str,
    payload: &str,
    error: &RecipeError,
) -> ProcessingResult<()> {
    tx.execute("INSERT INTO recipe_recovery(recovery_id,job_id,asset_id,payload,error_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![id(),job,asset,payload,serde_json::to_string(error).map_err(internal)?,now()]).map_err(internal)?;
    Ok(())
}
fn snapshot(
    tx: &Transaction<'_>,
    job: &str,
    asset: &str,
    recipe: &EditRecipe,
    reason: RevisionReason,
) -> ProcessingResult<u64> {
    let json = recipe.canonical_json()?;
    let hash = recipe.content_hash()?;
    let previous: Option<(u64,String,String)> = tx.query_row(
        "SELECT revision_number,recipe_hash,recipe_json FROM recipe_revisions WHERE job_id=?1 AND asset_id=?2 ORDER BY revision_number DESC LIMIT 1",
        params![job,asset], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(internal)?;
    // Preserve meaningful metadata changes too; clocks alone are not meaningful snapshots.
    if let Some((n, _, old_json)) = &previous {
        if let Ok(mut old) = parse_recipe(old_json) {
            old.updated_at = recipe.updated_at.clone();
            if old == *recipe {
                return Ok(*n);
            }
        }
    }
    let number = previous.map_or(1, |p| p.0 + 1);
    tx.execute("INSERT INTO recipe_revisions(revision_id,job_id,asset_id,revision_number,recipe_json,recipe_hash,origin,reason,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![id(),job,asset,number,json,hash,origin(recipe)?,reason.name(),now()]).map_err(internal)?;
    tx.execute("DELETE FROM recipe_revisions WHERE job_id=?1 AND asset_id=?2 AND revision_number != 1 AND revision_number NOT IN (SELECT revision_number FROM recipe_revisions WHERE job_id=?1 AND asset_id=?2 ORDER BY revision_number DESC LIMIT ?3)",
        params![job,asset,MAX_REVISIONS-1]).map_err(internal)?;
    Ok(number)
}
impl JobRepository {
    /// Lazy migration: job grids never parse recipe histories or all 3,000 current payloads.
    pub fn get_recipe(&self, job: &str, asset: &str) -> ProcessingResult<RecipeState> {
        self.asset(job, asset).map_err(internal)?;
        let mut db = self.connect().map_err(internal)?;
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM asset_recipes WHERE job_id=?1 AND asset_id=?2)",
                params![job, asset],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if !exists {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(internal)?;
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM asset_recipes WHERE job_id=?1 AND asset_id=?2)",
                    params![job, asset],
                    |r| r.get(0),
                )
                .map_err(internal)?;
            if !exists {
                let legacy: Option<(String,String)> = tx.query_row("SELECT adjustments_json,updated_at FROM development_state WHERE job_id=?1 AND asset_id=?2", params![job,asset],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(internal)?;
                let mut recipe = EditRecipe::neutral(id(), asset.into(), now());
                let mut error = None;
                let reason = if let Some((json, timestamp)) = legacy {
                    let result = serde_json::from_str::<RenderAdjustments>(&json)
                        .map_err(internal)
                        .and_then(|a| recipe.clone().with_adjustments(&a).map_err(Into::into));
                    match result {
                        Ok(mut r) => {
                            if chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok() {
                                r.created_at = timestamp.clone();
                                r.updated_at = timestamp;
                            }
                            recipe = r;
                        }
                        Err(e) => {
                            let e = corrupt(e);
                            archive(&tx, job, asset, &json, &e)?;
                            error = Some(e);
                        }
                    }
                    recipe.provenance.origin = RecipeOrigin::Migrated;
                    RevisionReason::Migration
                } else {
                    RevisionReason::Initial
                };
                let json = recipe.canonical_json()?;
                let hash = recipe.content_hash()?;
                snapshot(&tx, job, asset, &recipe, reason)?;
                tx.execute("INSERT INTO asset_recipes(job_id,asset_id,recipe_json,schema_version,recipe_hash,origin,created_at,updated_at,error_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![job,asset,json,RECIPE_SCHEMA_VERSION,hash,origin(&recipe)?,recipe.created_at,recipe.updated_at,error.map(|e|serde_json::to_string(&e)).transpose().map_err(internal)?]).map_err(internal)?;
            }
            tx.commit().map_err(internal)?;
        }
        let (payload,stored_hash,generation,current_revision,error_json,schema): (String,String,u64,u64,Option<String>,u32) = db.query_row(
            "SELECT recipe_json,recipe_hash,generation,current_revision,error_json,schema_version FROM asset_recipes WHERE job_id=?1 AND asset_id=?2",
            params![job,asset],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(internal)?;
        let parsed = parse_recipe(&payload).and_then(|r| {
            if r.asset_id != asset || r.content_hash()? != stored_hash || schema != r.schema_version
            {
                Err(corrupt("Binding, schema or hash mismatch"))
            } else {
                Ok(r)
            }
        });
        let (recipe, error) = match parsed {
            Ok(recipe) => (
                recipe,
                error_json.map(|s| {
                    serde_json::from_str(&s)
                        .unwrap_or_else(|_| corrupt("Invalid stored error record"))
                }),
            ),
            Err(e) => (
                EditRecipe::neutral(format!("recovery-{}", id()), asset.into(), now()),
                Some(corrupt(e)),
            ),
        };
        let hash = recipe.content_hash()?;
        let previous: Option<String> = db.query_row("SELECT recipe_json FROM recipe_revisions WHERE job_id=?1 AND asset_id=?2 AND revision_number=?3",params![job,asset,current_revision],|r|r.get(0)).optional().map_err(internal)?;
        let modified = previous
            .and_then(|json| parse_recipe(&json).ok())
            .is_none_or(|mut old| {
                old.updated_at = recipe.updated_at.clone();
                old != recipe
            });
        Ok(RecipeState {
            recipe,
            modified,
            recipe_hash: hash,
            generation,
            current_revision,
            error,
        })
    }
    /// Atomic optimistic save, optional commit point, history and checkpoint projection.
    pub fn save_recipe(
        &self,
        job: &str,
        asset: &str,
        recipe: &EditRecipe,
        expected_generation: u64,
        reason: Option<RevisionReason>,
    ) -> ProcessingResult<RecipeState> {
        let current = self.get_recipe(job, asset)?;
        if current.generation != expected_generation {
            return Err(conflict());
        }
        if recipe.asset_id != asset {
            return Err(RecipeError::new(
                RecipeErrorCode::InvalidRecipe,
                "Recipe is bound to a different asset; use import/template instantiation",
            )
            .into());
        }
        let mut recipe = recipe.validated()?;
        if current.error.is_none()
            && (recipe.recipe_id != current.recipe.recipe_id
                || recipe.created_at != current.recipe.created_at)
        {
            return Err(RecipeError::new(
                RecipeErrorCode::InvalidRecipe,
                "Current recipe identity and creation timestamp are immutable",
            )
            .into());
        }
        if let Some(error) = &current.error {
            if !matches!(
                reason,
                Some(RevisionReason::Reset | RevisionReason::Imported | RevisionReason::Restore)
            ) {
                return Err(error.clone().into());
            }
        }
        recipe.updated_at = now();
        let json = recipe.canonical_json()?;
        let hash = recipe.content_hash()?;
        let mut db = self.connect().map_err(internal)?;
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(internal)?;
        let (generation, payload): (u64, String) = tx
            .query_row(
                "SELECT generation,recipe_json FROM asset_recipes WHERE job_id=?1 AND asset_id=?2",
                params![job, asset],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(internal)?;
        if generation != expected_generation {
            return Err(conflict());
        }
        if let Some(e) = &current.error {
            archive(&tx, job, asset, &payload, e)?;
        }
        let mut revision = current.current_revision;
        if let Some(reason) = reason {
            // A reset/import/restore must also retain any unsnapshotted edits it replaces.
            if current.error.is_none()
                && matches!(
                    reason,
                    RevisionReason::Reset | RevisionReason::Imported | RevisionReason::Restore
                )
            {
                snapshot(&tx, job, asset, &current.recipe, RevisionReason::ManualEdit)?;
            }
            revision = snapshot(&tx, job, asset, &recipe, reason)?;
        }
        tx.execute("UPDATE asset_recipes SET recipe_json=?3,schema_version=?4,recipe_hash=?5,origin=?6,generation=generation+1,current_revision=?7,updated_at=?8,created_at=?9,error_json=NULL WHERE job_id=?1 AND asset_id=?2",
            params![job,asset,json,RECIPE_SCHEMA_VERSION,hash,origin(&recipe)?,revision,recipe.updated_at,recipe.created_at]).map_err(internal)?;
        let adjustments = serde_json::to_string(&recipe.adjustments()?).map_err(internal)?;
        // Compatibility projection only: reads and rendering are authoritative from asset_recipes.
        tx.execute("INSERT INTO development_state(job_id,asset_id,adjustments_json,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,asset_id) DO UPDATE SET adjustments_json=excluded.adjustments_json,revision=development_state.revision+1,state='source_ready',request_id=NULL,preview_path=NULL,error_json=NULL,updated_at=excluded.updated_at",
            params![job,asset,adjustments,recipe.updated_at]).map_err(internal)?;
        tx.commit().map_err(internal)?;
        self.get_recipe(job, asset)
    }
    pub fn create_revision(
        &self,
        job: &str,
        asset: &str,
        expected_generation: u64,
        reason: RevisionReason,
    ) -> ProcessingResult<RecipeState> {
        let current = self.get_recipe(job, asset)?;
        self.save_recipe(
            job,
            asset,
            &current.recipe,
            expected_generation,
            Some(reason),
        )
    }
    /// Metadata only, bounded and lazy. Snapshot payloads are fetched individually.
    pub fn recipe_history(
        &self,
        job: &str,
        asset: &str,
        offset: u32,
        limit: u32,
    ) -> ProcessingResult<Vec<RecipeRevision>> {
        self.get_recipe(job, asset)?;
        let db = self.connect().map_err(internal)?;
        let mut stmt = db.prepare("SELECT revision_id,revision_number,recipe_hash,origin,reason,created_at FROM recipe_revisions WHERE job_id=?1 AND asset_id=?2 ORDER BY revision_number DESC LIMIT ?3 OFFSET ?4").map_err(internal)?;
        let rows = stmt
            .query_map(params![job, asset, limit.clamp(1, 100), offset], |r| {
                Ok(RecipeRevision {
                    revision_id: r.get(0)?,
                    revision_number: r.get(1)?,
                    recipe_hash: r.get(2)?,
                    origin: r.get(3)?,
                    reason: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)?;
        Ok(rows)
    }
    pub fn revision_recipe(
        &self,
        job: &str,
        asset: &str,
        revision_id: &str,
    ) -> ProcessingResult<EditRecipe> {
        let (json,hash): (String,String) = self.connect().map_err(internal)?.query_row("SELECT recipe_json,recipe_hash FROM recipe_revisions WHERE job_id=?1 AND asset_id=?2 AND revision_id=?3",params![job,asset,revision_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(internal)?;
        let recipe = parse_recipe(&json)?;
        if recipe.asset_id != asset || recipe.content_hash()? != hash {
            return Err(corrupt("Revision binding/hash mismatch").into());
        }
        Ok(recipe)
    }
    pub fn restore_revision(
        &self,
        job: &str,
        asset: &str,
        revision_id: &str,
        generation: u64,
    ) -> ProcessingResult<RecipeState> {
        let current = self.get_recipe(job, asset)?;
        let mut recipe = self.revision_recipe(job, asset, revision_id)?;
        if current.error.is_none() {
            recipe.recipe_id = current.recipe.recipe_id;
            recipe.created_at = current.recipe.created_at;
        }
        self.save_recipe(
            job,
            asset,
            &recipe,
            generation,
            Some(RevisionReason::Restore),
        )
    }
    pub fn recipe_diff(
        &self,
        job: &str,
        asset: &str,
        revision_id: &str,
    ) -> ProcessingResult<Vec<RecipeDifference>> {
        Ok(diff_recipes(
            &self.revision_recipe(job, asset, revision_id)?,
            &self.get_recipe(job, asset)?.recipe,
        )?)
    }
    pub fn import_recipe(
        &self,
        job: &str,
        asset: &str,
        json: &str,
        generation: u64,
    ) -> ProcessingResult<RecipeState> {
        let imported = parse_recipe(json)?;
        let current = self.get_recipe(job, asset)?;
        let mut recipe = imported.clone();
        recipe.recipe_id = current.recipe.recipe_id;
        recipe.asset_id = asset.into();
        recipe.created_at = current.recipe.created_at;
        recipe.provenance.source_recipe_id = Some(imported.recipe_id);
        recipe.provenance.origin = RecipeOrigin::Imported;
        recipe.provenance.manually_modified = false;
        recipe.provenance.acceptance = None;
        recipe.metadata.confidence = None;
        recipe.metadata.needs_review = None;
        // Rebind logical subject/background selectors only to the target's own mask cache.
        // Even same-asset imports discard references: the source may have changed meanwhile.
        for layer in &mut recipe.local_layers {
            layer.mask_reference = None;
            layer.confidence = None;
        }
        self.save_recipe(
            job,
            asset,
            &recipe,
            generation,
            Some(RevisionReason::Imported),
        )
    }
    pub fn import_recipe_file(
        &self,
        job: &str,
        asset: &str,
        path: &Path,
        generation: u64,
    ) -> ProcessingResult<RecipeState> {
        let mut json = String::new();
        std::fs::File::open(path)
            .map_err(io_error)?
            .take(MAX_RECIPE_BYTES as u64 + 1)
            .read_to_string(&mut json)
            .map_err(io_error)?;
        self.import_recipe(job, asset, &json, generation)
    }
    pub fn export_recipe(&self, job: &str, asset: &str) -> ProcessingResult<PathBuf> {
        let state = self.get_recipe(job, asset)?;
        if let Some(e) = state.error {
            return Err(e.into());
        }
        let job_record = self.get_job(job).map_err(internal)?;
        let asset_record = self.asset(job, asset).map_err(internal)?;
        let output = job_record.output_path.canonicalize().map_err(io_error)?;
        let input = job_record.input_path.canonicalize().map_err(io_error)?;
        if crate::paths::same_or_descendant(&input, &output) {
            return Err(internal("Recipe output aliases input folder"));
        }
        let stem = asset_record
            .original_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let stem: String = stem
            .chars()
            .take(100)
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let mut temp = tempfile::NamedTempFile::new_in(&output).map_err(io_error)?;
        temp.write_all(state.recipe.canonical_json()?.as_bytes())
            .map_err(io_error)?;
        temp.as_file().sync_all().map_err(io_error)?;
        // A unique suffix and no-clobber publish protect every original and prior export.
        let destination = output.join(format!("{stem}-{}.recipe.json", id()));
        temp.persist_noclobber(&destination)
            .map_err(|e| io_error(e.error))?;
        Ok(destination)
    }
}
