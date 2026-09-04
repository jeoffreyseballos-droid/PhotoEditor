//! Resolution of portable intent against objective, asset-specific dependencies.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectiveRenderRecipe {
    pub recipe_hash: String,
    pub source_fingerprint: String,
    pub dependency_hash: String,
    pub adjustments: RenderAdjustments,
    pub unresolved_masks: Vec<String>,
    pub mask: MaskDiagnostic,
}
pub fn recipe_preview_key(
    source: &str,
    recipe_hash: &str,
    dependencies: &str,
    backend: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        source,
        recipe_hash,
        dependencies,
        backend,
        RENDERER_VERSION,
        "recipe-preview-1600-half-jpeg90-v1",
    ] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
impl CpuProcessingEngine {
    pub fn effective_recipe(
        &self,
        recipe: &EditRecipe,
        source: &Path,
        metadata: &OpticsMetadata,
    ) -> ProcessingResult<EffectiveRenderRecipe> {
        let recipe = recipe.validated()?;
        let mut adjustments = recipe.adjustments()?;
        let identity = source_identity(source)?;
        let source_fingerprint = format!("{:x}", Sha256::digest(identity.as_bytes()));
        let active = recipe
            .local_layers
            .iter()
            .any(|l| l.enabled && l.strength > 0.);
        let (mask, diagnostic) = if active {
            self.masks
                .as_ref()
                .map(|m| m.load(&identity, self.raw.id()))
                .unwrap_or_default()
        } else {
            (None, MaskDiagnostic::default())
        };
        let mut mask_hash = Sha256::new();
        // Hash the validated samples, not a filename/mtime. Replacement and deletion rekey previews.
        if let Some(mask) = &mask {
            mask_hash.update(mask.width.to_le_bytes());
            mask_hash.update(mask.height.to_le_bytes());
            for sample in &mask.values {
                mask_hash.update(sample.to_le_bytes());
            }
        }
        let mut unresolved = Vec::new();
        for (intent, effective) in recipe
            .local_layers
            .iter()
            .zip(&mut adjustments.local_layers)
        {
            if !intent.enabled || intent.strength == 0. {
                continue;
            }
            let matches = intent.mask_reference.as_ref().is_none_or(|r| {
                diagnostic.reference.as_ref() == Some(&r.content_id)
                    && r.source_fingerprint
                        .as_ref()
                        .is_none_or(|v| v == &source_fingerprint)
                    && r.model_id.as_deref().is_none_or(|v| v == "modnet")
                    && r.model_version
                        .as_ref()
                        .is_none_or(|v| Some(v) == diagnostic.model_version.as_ref())
                    && r.geometry_version
                        .as_deref()
                        .is_none_or(|v| v == MASK_GEOMETRY_VERSION)
            });
            if mask.is_none() || !matches || intent.mask_type == MaskType::Custom {
                effective.enabled = false;
                unresolved.push(intent.id.clone());
            } else {
                effective.mask_reference = diagnostic.reference.clone();
            }
        }
        let dependencies = serde_json::json!({
            "source":source_fingerprint,"decoder":self.raw.id(),"geometry":MASK_GEOMETRY_VERSION,
            "mask": if active { Some(serde_json::json!({"content":format!("{:x}",mask_hash.finalize()),"reference":diagnostic.reference,"model":diagnostic.model_version,"status":diagnostic.status})) } else { None },
            "profile": if recipe.global.optics.enabled { Some(self.lenses.dependency_identity()) } else { None },
            "profile_version": if recipe.global.optics.enabled { Some(optics::DATABASE_VERSION) } else { None },
            "objective_metadata": if recipe.global.optics.enabled { Some(metadata) } else { None }
        });
        Ok(EffectiveRenderRecipe {
            recipe_hash: recipe.content_hash()?,
            source_fingerprint,
            dependency_hash: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&dependencies).map_err(internal)?)
            ),
            adjustments,
            unresolved_masks: unresolved,
            mask: diagnostic,
        })
    }
    /// Independent entry point for future batch workers; request.adjustments is ignored.
    pub fn render_recipe(
        &self,
        recipe: &EditRecipe,
        request: &RenderRequest,
        cancel: &CancellationToken,
    ) -> ProcessingResult<RenderResult> {
        if recipe.asset_id != request.asset_id {
            return Err(RecipeError::new(
                RecipeErrorCode::InvalidRecipe,
                "Recipe/render asset binding mismatch",
            )
            .into());
        }
        let effective =
            self.effective_recipe(recipe, &request.original, &request.source_metadata)?;
        let mut resolved = request.clone();
        resolved.adjustments = effective.adjustments;
        let mut result = self.render_with_metadata(&resolved, &request.source_metadata, cancel)?;
        if !effective.unresolved_masks.is_empty() {
            result.warnings.push(format!(
                "Unresolved local masks (intent retained): {}",
                effective.unresolved_masks.join(", ")
            ));
            result.diagnostics.mask = effective.mask;
        }
        let after = self.effective_recipe(recipe, &request.original, &request.source_metadata)?;
        if after.dependency_hash != effective.dependency_hash {
            // Callers render into a temporary location, never publish this obsolete result.
            return Err(ProcessingError::new(
                ProcessingErrorCode::SourceChanged,
                "Source/mask/profile dependencies changed during render; retry",
            ));
        }
        Ok(result)
    }
}
