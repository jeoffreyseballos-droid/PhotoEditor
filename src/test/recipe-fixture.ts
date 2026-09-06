import { neutralAdjustments } from "../toolkit";
import { recipeControls, updateRecipeControls } from "../recipe";
import type { EditRecipe, RecipeState } from "../recipe";
import type { DevelopmentState, RenderAdjustments } from "../types";
import type { ToolkitDiagnostics } from "../toolkit";
export function recipeFixture(
  a: RenderAdjustments = neutralAdjustments(),
): EditRecipe {
  const r: EditRecipe = {
    schema_version: 1,
    recipe_id: "recipe-1",
    asset_id: "photo-1",
    created_at: "2026-09-04T00:00:00Z",
    updated_at: "2026-09-04T00:00:00Z",
    global: {
      basic: {
        exposure_ev: 0,
        temperature: 6500,
        tint: 0,
        contrast: 0,
        highlights: 0,
        shadows: 0,
        whites: 0,
        blacks: 0,
        saturation: 0,
        vibrance: 0,
      },
      curve: a.curve,
      color_mixer: {
        red: a.hsl[0],
        orange: a.hsl[1],
        yellow: a.hsl[2],
        green: a.hsl[3],
        aqua: a.hsl[4],
        blue: a.hsl[5],
        purple: a.hsl[6],
        magenta: a.hsl[7],
      },
      presence: a.presence,
      detail: { ...a.detail, legacy_sharpening: 0, legacy_noise_reduction: 0 },
      optics: a.optics,
      effects: a.effects,
      geometry: { crop: a.crop, rotation_degrees: 0 },
    },
    local_layers: [],
    metadata: {
      scene_cluster_id: null,
      sequence_id: null,
      reference_asset_id: null,
      consistency_group_id: null,
      consistency_note: null,
      confidence: null,
      needs_review: null,
    },
    provenance: {
      origin: "manual",
      created_by: null,
      source_recipe_id: null,
      style_id: null,
      model_id: null,
      model_version: null,
      analysis_id: null,
      style_version: null,
      style_package_id: null,
      feature_schema_version: null,
      batch_context_id: null,
      batch_context_version: null,
      photo_analysis_version: null,
      manually_modified: false,
      acceptance: null,
    },
  };
  return structuredClone(updateRecipeControls(r, a));
}
export function recipeStateFixture(a?: RenderAdjustments): RecipeState {
  return {
    recipe: recipeFixture(a),
    // Opaque stand-in for the Rust-owned SHA-256; UI fixtures do not implement hashing.
    recipe_hash: "a".repeat(64),
    generation: 1,
    current_revision: 1,
    modified: false,
    error: null,
  };
}

// Unlike the compatibility DTO, a successful Phase 3 response always has a recipe.
export type DevelopmentFixture = DevelopmentState & {
  recipe_state: RecipeState;
  diagnostics: ToolkitDiagnostics;
  unresolved_masks: string[];
};

export function developmentStateFixture(
  recipeState: RecipeState = recipeStateFixture(),
  checkpoint: Partial<
    Pick<
      DevelopmentState,
      "revision" | "state" | "preview_path" | "export_path"
    >
  > = {},
): DevelopmentFixture {
  const recipe_state = structuredClone(recipeState);
  return {
    recipe_state,
    // Derive the compatibility projection from the recipe, never maintain two edit fixtures.
    adjustments: structuredClone(recipeControls(recipe_state.recipe)),
    revision: recipe_state.generation,
    state: "source_ready",
    source_identity: null,
    preview_path: null,
    export_path: null,
    error: null,
    warnings: [],
    diagnostics: {
      lens: {
        state: "correction_disabled",
        profile: null,
        database_version: null,
        applied: [],
        warnings: [],
      },
      mask: {
        status: "unavailable",
        reference: null,
        model_version: null,
        cache_path: null,
        width: 0,
        height: 0,
        confidence: null,
        warnings: [],
      },
    },
    unresolved_masks: recipe_state.recipe.local_layers
      .filter((layer) => layer.enabled && layer.strength > 0)
      .map((layer) => layer.id),
    ...checkpoint,
  };
}
