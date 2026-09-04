import type { RenderAdjustments } from "./types";
export interface CurvePoint {
  x: number;
  y: number;
}
export interface ToneCurve {
  master: CurvePoint[];
  red: CurvePoint[];
  green: CurvePoint[];
  blue: CurvePoint[];
}
export interface HslBand {
  hue: number;
  saturation: number;
  luminance: number;
}
export interface Presence {
  texture: number;
  clarity: number;
  dehaze: number;
}
export interface Detail {
  sharpening: {
    amount: number;
    radius: number;
    detail: number;
    masking: number;
  };
  noise: {
    luminance: number;
    luminance_detail: number;
    color: number;
    color_detail: number;
  };
}
export interface Basic {
  exposure_ev: number;
  temperature: number;
  tint: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  saturation: number;
  vibrance: number;
}
export interface LocalAdjustments extends Basic {
  presence: Presence;
  detail: Detail;
}
export interface LocalLayer {
  id: string;
  mask_type: "subject" | "background" | "custom";
  enabled: boolean;
  strength: number;
  invert: boolean;
  confidence: number | null;
  mask_reference: string | null;
  adjustments: LocalAdjustments;
}
export interface Toolkit {
  schema_version: number;
  curve: ToneCurve;
  hsl: HslBand[];
  presence: Presence;
  detail: Detail;
  optics: {
    enabled: boolean;
    distortion: boolean;
    vignette: boolean;
    chromatic_aberration: boolean;
    distortion_strength: number;
    vignette_strength: number;
    manual_distortion: number;
    manual_vignette: number;
  };
  effects: {
    vignette: {
      amount: number;
      midpoint: number;
      feather: number;
      roundness: number;
    };
  };
  local_layers: LocalLayer[];
  batch_context: {
    scene_cluster_id: string | null;
    sequence_id: string | null;
    reference_asset_id: string | null;
    consistency_note: string | null;
  } | null;
}
export interface LensDiagnostic {
  state: string;
  profile: string | null;
  database_version: string | null;
  applied: string[];
  warnings: string[];
}
export interface MaskDiagnostic {
  status:
    "ready" | "generating" | "unavailable" | "failed" | "unsupported" | "stale";
  reference: string | null;
  model_version: string | null;
  cache_path: string | null;
  width: number;
  height: number;
  confidence: number | null;
  warnings: string[];
}
export interface ToolkitDiagnostics {
  lens: LensDiagnostic;
  mask: MaskDiagnostic;
}
export interface MaskRequest {
  job_id: string;
  asset_id: string;
  request_id: string;
  adjustments: RenderAdjustments;
  layer_id: string | null;
  generate: boolean;
}
export interface MaskResult {
  diagnostic: MaskDiagnostic;
  overlay_data: string | null;
}
export const neutralBasic = (): Basic => ({
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
});
export const neutralPresence = (): Presence => ({
  texture: 0,
  clarity: 0,
  dehaze: 0,
});
export const neutralDetail = (): Detail => ({
  sharpening: { amount: 0, radius: 1, detail: 25, masking: 0 },
  noise: { luminance: 0, luminance_detail: 50, color: 0, color_detail: 50 },
});
export const neutralLocal = (): LocalAdjustments => ({
  ...neutralBasic(),
  presence: neutralPresence(),
  detail: neutralDetail(),
});
export function neutralAdjustments(): RenderAdjustments {
  const identity = () => [
    { x: 0, y: 0 },
    { x: 1, y: 1 },
  ];
  return {
    ...neutralBasic(),
    schema_version: 2,
    curve: {
      master: identity(),
      red: identity(),
      green: identity(),
      blue: identity(),
    },
    hsl: Array.from({ length: 8 }, () => ({
      hue: 0,
      saturation: 0,
      luminance: 0,
    })),
    presence: neutralPresence(),
    detail: neutralDetail(),
    optics: {
      enabled: false,
      distortion: true,
      vignette: true,
      chromatic_aberration: true,
      distortion_strength: 1,
      vignette_strength: 1,
      manual_distortion: 0,
      manual_vignette: 0,
    },
    effects: {
      vignette: { amount: 0, midpoint: 50, feather: 75, roundness: 0 },
    },
    local_layers: [],
    batch_context: null,
    rotation_degrees: 0,
    crop: { x: 0, y: 0, width: 1, height: 1 },
    sharpening: 0,
    noise_reduction: 0,
  };
}
