import type { PhotoType } from "./analysis";
import type { AssetBatchContext } from "./batch-context";
import type { EditRecipe } from "./recipe";

export type PairValidationStatus =
  "pending" | "ready" | "needs_review" | "rejected" | "unusable";
export type GeometryRelationship =
  | "unknown"
  | "exact_or_near"
  | "crop_difference"
  | "major_mismatch"
  | "unusable";
export type TrainingSplit = "unassigned" | "train" | "validation" | "excluded";
export type TargetFitConfidence = "high" | "medium" | "low";
export type ValidationFeedback = "accept" | "needs_adjustment" | "reject";

export interface PairValidation {
  status: PairValidationStatus;
  geometry: GeometryRelationship;
  structural_similarity: number | null;
  source_width: number | null;
  source_height: number | null;
  reference_width: number | null;
  reference_height: number | null;
  diagnostics: string[];
}

export interface CreativeControls {
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

export interface TargetRecipeResult {
  schema_version: number;
  optimizer_version: string;
  cache_identity: string;
  recipe: EditRecipe;
  controls: CreativeControls;
  confidence: TargetFitConfidence;
  loss: {
    total: number;
    luminance: number;
    color_balance: number;
    saturation: number;
    structure: number;
  };
  iterations: number;
  unsupported_differences: string[];
  warnings: string[];
}

export interface TrainingPair {
  schema_version: number;
  pair_id: string;
  dataset_id: string;
  source_job_id: string;
  source_asset_id: string;
  source_path: string;
  reference_path: string;
  photo_type: PhotoType;
  source_fingerprint: string;
  reference_fingerprint: string;
  validation: PairValidation;
  source_analysis_id: string | null;
  batch_context: AssetBatchContext | null;
  scene_group_id: string | null;
  target: TargetRecipeResult | null;
  split: TrainingSplit;
  excluded: boolean;
  feedback: ValidationFeedback | null;
  diagnostics: string[];
}

export interface TrainingDataset {
  schema_version: number;
  dataset_id: string;
  job_id: string;
  style_name: string;
  photo_type: PhotoType;
  pairs: TrainingPair[];
  created_at: string;
  updated_at: string;
  dataset_fingerprint: string | null;
  feature_schema: string;
  renderer_version: string;
  target_recipe_schema: number;
  batch_context_id: string | null;
  warnings: string[];
  before_files: string[];
  after_files: string[];
  alignment: TrainingAlignment | null;
}

export interface TrainingAlignment {
  before_count: number;
  after_count: number;
  matched_count: number;
  ambiguous_count: number;
  unmatched_before: string[];
  unmatched_after: string[];
  first_before: string | null;
  first_after: string | null;
  last_before: string | null;
  last_after: string | null;
  start_aligned: boolean;
  end_aligned: boolean;
  order_fallback_used: boolean;
  diagnostics: string[];
}

export interface TrainingConfig {
  validation_percent: number;
  regularization: number;
  learning_rate: number;
  epochs: number;
  exclude_low_confidence: boolean;
  deterministic: boolean;
}

export const DEFAULT_TRAINING_CONFIG: TrainingConfig = {
  validation_percent: 20,
  regularization: 0.08,
  learning_rate: 0.025,
  epochs: 1200,
  exclude_low_confidence: true,
  deterministic: true,
};

export type TrainingRunStatus =
  "queued" | "running" | "complete" | "failed" | "cancelled" | "interrupted";
export type TrainingStage =
  | "queued"
  | "validating_pairs"
  | "analyzing"
  | "estimating_target_recipes"
  | "building_examples"
  | "training"
  | "validating"
  | "exporting_style"
  | "complete"
  | "stopped";

export interface MetricSet {
  recipe_mae: Record<string, number>;
  mean_recipe_mae: number;
  rendered_loss: number | null;
}

export interface TrainingMetrics {
  train: MetricSet;
  validation: MetricSet;
  neutral_baseline: MetricSet;
  mean_baseline: MetricSet;
  beats_mean_baseline: boolean;
  overfitting_warning: string | null;
  warnings: string[];
}

export interface TrainingRun {
  schema_version: number;
  run_id: string;
  dataset_id: string;
  style_id: string | null;
  style_name: string;
  style_version: string | null;
  status: TrainingRunStatus;
  stage: TrainingStage;
  completed: number;
  total: number;
  config: TrainingConfig;
  metrics: TrainingMetrics | null;
  artifact_path: string | null;
  started_at: string;
  updated_at: string;
  duration_ms: number;
  error: string | null;
}

export interface AutoMatchResult {
  dataset: TrainingDataset;
  matching: {
    matched: Array<{
      source_asset_id: string;
      source_filename: string;
      source_path?: string;
      reference_path: string;
    }>;
    ambiguous_sources: string[];
    unmatched_references: string[];
    unmatched_sources?: string[];
    before_count?: number;
    after_count?: number;
    start_aligned?: boolean;
    end_aligned?: boolean;
    order_fallback_used?: boolean;
    diagnostics?: string[];
  };
}

export interface TrainingPreviewSet {
  source_data: string;
  ai_data: string | null;
  target_data: string | null;
  reference_data: string;
}

export interface MatchingProgress {
  request_id: string;
  dataset_id: string;
  status: "running" | "complete" | "failed" | "cancelled";
  stage: string;
  processed: number;
  total: number;
  error: string | null;
}

export interface ValidationEditingSelection {
  photo_type: PhotoType;
  asset_ids: string[];
}
