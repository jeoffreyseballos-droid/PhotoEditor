import type {
  Basic,
  Detail,
  LocalLayer,
  Toolkit,
  ToneCurve,
  HslBand,
  Presence,
} from "./toolkit";
import { neutralAdjustments } from "./toolkit";
import type { RenderAdjustments } from "./types";
export const colorBands = [
  "red",
  "orange",
  "yellow",
  "green",
  "aqua",
  "blue",
  "purple",
  "magenta",
] as const;
export interface MaskReference {
  content_id: string;
  source_fingerprint: string | null;
  model_id: string | null;
  model_version: string | null;
  geometry_version: string | null;
}
export interface RecipeLayer extends Omit<LocalLayer, "mask_reference"> {
  mask_reference: MaskReference | null;
}
export interface EditRecipe {
  schema_version: number;
  recipe_id: string;
  asset_id: string;
  created_at: string;
  updated_at: string;
  global: {
    basic: Basic;
    curve: ToneCurve;
    color_mixer: Record<(typeof colorBands)[number], HslBand>;
    presence: Presence;
    detail: Detail & {
      legacy_sharpening: number;
      legacy_noise_reduction: number;
    };
    optics: Toolkit["optics"];
    effects: Toolkit["effects"];
    geometry: { rotation_degrees: number; crop: RenderAdjustments["crop"] };
  };
  local_layers: RecipeLayer[];
  metadata: {
    scene_cluster_id: string | null;
    sequence_id: string | null;
    reference_asset_id: string | null;
    consistency_group_id: string | null;
    consistency_note: string | null;
    confidence: number | null;
    needs_review: boolean | null;
  };
  provenance: {
    origin:
      | "manual"
      | "imported"
      | "migrated"
      | "system"
      | "trained_style"
      | "ai_generated"
      | "correction"
      | "batch_consistency";
    created_by: string | null;
    source_recipe_id: string | null;
    style_id: string | null;
    model_id: string | null;
    model_version: string | null;
    analysis_id: string | null;
    manually_modified: boolean;
    acceptance: "accepted" | "rejected" | null;
  };
}
export interface RecipeState {
  recipe: EditRecipe;
  recipe_hash: string;
  generation: number;
  current_revision: number;
  modified: boolean;
  error: { code: string; message: string } | null;
}
export type RevisionReason =
  "snapshot" | "reset" | "manual_edit" | "built_in_preset";
export interface RecipeRevision {
  revision_id: string;
  revision_number: number;
  recipe_hash: string;
  origin: string;
  reason: string;
  created_at: string;
}
export interface RecipeDifference {
  control: string;
  before: unknown;
  after: unknown;
}

/** A view adapter for the existing controls, never an alternative renderer or validation path. */
export function recipeControls(recipe: EditRecipe): RenderAdjustments {
  const g = recipe.global;
  return {
    ...neutralAdjustments(),
    ...g.basic,
    curve: g.curve,
    hsl: colorBands.map((b) => g.color_mixer[b]),
    presence: g.presence,
    detail: { sharpening: g.detail.sharpening, noise: g.detail.noise },
    sharpening: g.detail.legacy_sharpening,
    noise_reduction: g.detail.legacy_noise_reduction,
    optics: g.optics,
    effects: g.effects,
    ...g.geometry,
    local_layers: recipe.local_layers.map((l) => ({
      ...l,
      mask_reference: l.mask_reference?.content_id ?? null,
    })),
    batch_context: {
      scene_cluster_id: recipe.metadata.scene_cluster_id,
      sequence_id: recipe.metadata.sequence_id,
      reference_asset_id: recipe.metadata.reference_asset_id,
      consistency_note: recipe.metadata.consistency_note,
    },
  };
}
export function updateRecipeControls(
  recipe: EditRecipe,
  a: RenderAdjustments,
): EditRecipe {
  return {
    ...recipe,
    global: {
      basic: {
        exposure_ev: a.exposure_ev,
        temperature: a.temperature,
        tint: a.tint,
        contrast: a.contrast,
        highlights: a.highlights,
        shadows: a.shadows,
        whites: a.whites,
        blacks: a.blacks,
        saturation: a.saturation,
        vibrance: a.vibrance,
      },
      curve: a.curve,
      color_mixer: Object.fromEntries(
        colorBands.map((b, i) => [b, a.hsl[i]]),
      ) as EditRecipe["global"]["color_mixer"],
      presence: a.presence,
      detail: {
        ...a.detail,
        legacy_sharpening: a.sharpening,
        legacy_noise_reduction: a.noise_reduction,
      },
      optics: a.optics,
      effects: a.effects,
      geometry: { rotation_degrees: a.rotation_degrees, crop: a.crop },
    },
    local_layers: a.local_layers.map((l) => {
      const previous = recipe.local_layers.find(
        (p) => p.id === l.id,
      )?.mask_reference;
      return {
        ...l,
        mask_reference: l.mask_reference
          ? previous?.content_id === l.mask_reference
            ? previous
            : {
                content_id: l.mask_reference,
                source_fingerprint: null,
                model_id: null,
                model_version: null,
                geometry_version: null,
              }
          : null,
      };
    }),
    metadata: { ...recipe.metadata, ...a.batch_context },
    provenance: {
      ...recipe.provenance,
      origin:
        recipe.provenance.origin === "system"
          ? "manual"
          : recipe.provenance.origin,
      manually_modified: true,
      acceptance: null,
    },
  };
}
