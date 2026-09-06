import type { PhotoType } from "./analysis";

export interface StyleSummary {
  style_id: string;
  name: string;
  version: string;
  model_version: string;
  package_identity: string;
  photo_type: PhotoType;
  description: string;
  development_only: boolean;
}

export interface PredictedCreativeAdjustments {
  exposure_ev: number;
  temperature_delta: number;
  tint: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  saturation: number;
  vibrance: number;
  texture: number;
  clarity: number;
  dehaze: number;
  sharpening_amount: number;
  noise_reduction: number;
  vignette_amount: number;
}

export interface StylePrediction {
  style_id: string;
  style_version: string;
  model_version: string;
  package_identity: string;
  feature_schema: string;
  adjustments: PredictedCreativeAdjustments;
  confidence: "high" | "medium" | "low" | "insufficient_evidence";
  confidence_score: number;
  diagnostics: {
    resolver: string;
    missing_feature_count: number;
    bounded_controls: string[];
    warnings: string[];
  };
}

export interface StyleFeatureSummary {
  median_luminance: number;
  batch_exposure_delta_ev: number | null;
  warm_cool_balance: number;
  batch_warm_cool_delta: number | null;
  group_confidence: number;
  missing_feature_count: number;
}

export interface StyleAssetInference {
  job_id: string;
  asset_id: string;
  style_id: string;
  style_version: string;
  model_version: string;
  package_identity: string;
  feature_schema: string;
  input_identity: string | null;
  analysis_id: string | null;
  batch_context_id: string | null;
  status: string;
  prediction: StylePrediction | null;
  feature_summary: StyleFeatureSummary | null;
  recipe_hash: string | null;
  error: string | null;
  stale: boolean;
}

export interface StyleApplyProgress {
  job_id: string;
  request_id: string;
  photo_type: PhotoType;
  style_id: string;
  status: string;
  stage: string;
  completed: number;
  total: number;
  succeeded: number;
  failed: number;
  duration_ms: number;
  error: string | null;
}

export interface StyleEditingState {
  styles: StyleSummary[];
  selected_asset_ids: string[];
  applied_style: StyleSummary | null;
  applied_count: number;
  stale_asset_ids: string[];
  needs_review: string[];
  inferences: StyleAssetInference[];
  progress: StyleApplyProgress | null;
}

export interface StyleApplyRequest {
  job_id: string;
  photo_type: PhotoType;
  style_id: string;
  selected_asset_ids: string[];
  request_id: string;
}

export interface StyleApplyResult {
  style: StyleSummary;
  selected_asset_ids: string[];
  predictions_attempted: number;
  predictions_succeeded: number;
  predictions_failed: number;
  recipes_updated: number;
  recipes_unchanged: number;
  needs_review: string[];
  inferences: StyleAssetInference[];
  duration_ms: number;
}
