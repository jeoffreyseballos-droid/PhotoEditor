import type { PhotoType } from "./analysis";

export type ContextAvailability = "available" | "partial" | "unavailable";
export type BatchGroupKind = "scene" | "lighting";
export type SequenceKind = "burst" | "exposure_bracket" | "repeated_frames";

export type ConsistencyNoteCode =
  | "exposure_reference"
  | "near_exposure_median"
  | "darker_than_group"
  | "brighter_than_group"
  | "near_white_balance_median"
  | "warmer_than_group"
  | "cooler_than_group"
  | "greener_than_group"
  | "more_magenta_than_group"
  | "bracket_member"
  | "partial_evidence"
  | "analysis_unavailable";

export interface ConsistencyNote {
  code: ConsistencyNoteCode;
  message: string;
}

export interface ExposureRelationship {
  delta_ev: number;
  confidence: number;
}

export interface WhiteBalanceRelationship {
  warm_cool_delta: number;
  green_magenta_delta: number;
  confidence: number;
}

export interface BatchGroup {
  group_id: string;
  asset_ids: string[];
  confidence: number;
  reference_candidate_ids: string[];
}

export interface SequenceGroup {
  group_id: string;
  asset_ids: string[];
  kind: SequenceKind;
  confidence: number;
  source_culling_group_id: string | null;
}

export interface ReferenceCandidate {
  group_kind: BatchGroupKind;
  group_id: string;
  asset_id: string;
  rank: number;
  technical_score: number;
  confidence: number;
  reasons: string[];
}

export interface AssetBatchContext {
  asset_id: string;
  availability: ContextAvailability;
  scene_group_id: string | null;
  lighting_group_id: string | null;
  sequence_group_id: string | null;
  reference_asset_id: string | null;
  exposure_delta_from_group: ExposureRelationship | null;
  wb_delta_from_group: WhiteBalanceRelationship | null;
  group_confidence: number;
  consistency_notes: ConsistencyNote[];
}

export interface BatchStageTimings {
  loading_ms: number;
  candidate_generation_ms: number;
  grouping_ms: number;
  context_ms: number;
  persistence_ms: number;
  total_ms: number;
}

export interface BatchDiagnostics {
  available_assets: number;
  partial_assets: number;
  unavailable_assets: number;
  candidate_comparisons: number;
  candidate_limit_per_asset: number;
  timings: BatchStageTimings;
  warnings: string[];
}

export interface BatchContext {
  schema_version: number;
  batch_id: string;
  job_id: string;
  photo_type: PhotoType;
  selected_asset_ids: string[];
  selection_identity: string;
  created_at: string;
  analysis_version: string;
  grouping_version: string;
  scene_groups: BatchGroup[];
  lighting_groups: BatchGroup[];
  sequence_groups: SequenceGroup[];
  asset_contexts: AssetBatchContext[];
  reference_candidates: ReferenceCandidate[];
  diagnostics: BatchDiagnostics;
}

export interface BatchContextRequest {
  job_id: string;
  photo_type: PhotoType;
  request_id: string;
  force: boolean;
}

export interface BatchContextProgress {
  job_id: string;
  request_id: string;
  photo_type: PhotoType;
  status: string;
  stage: string;
  completed: number;
  total: number;
  cached: boolean;
  duration_ms: number;
  error: string | null;
}

export interface BatchContextState {
  selected_count: number;
  selection_identity: string | null;
  context: BatchContext | null;
  cached: boolean;
  stale: boolean;
  progress: BatchContextProgress | null;
}
