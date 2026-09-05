// Rust photo_contracts::culling remains authoritative. No culling state lives in a recipe.
import type { PhotoType, BoundingBox } from "./analysis";
interface ProviderIdentity {
  provider: string;
  model: string;
  version: string;
}
import type { Asset } from "./types";
export type Stars = 1 | 2 | 3 | 4 | 5;
export type Signal<T> =
  | { status: "available"; value: T; confidence: number }
  | {
      status: "unavailable" | "not_applicable" | "failed" | "uncertain";
      reason: string;
    };
export interface FaceFeatures {
  index: number;
  bbox: BoundingBox;
  detection_confidence: number;
  sharpness: Signal<number>;
  mean_luminance: number;
  highlight_clip_fraction: number;
  shadow_clip_fraction: number;
  eyes: Signal<"open" | "closed" | "uncertain" | "not_visible">;
  edge_distance: number;
  visible_fraction: number;
  relevant: boolean;
}
export type ReasonCode =
  | "technical_usable"
  | "source_unavailable"
  | "insufficient_evidence"
  | "face_detector_unavailable"
  | "no_faces_detected"
  | "eyes_open"
  | "eyes_closed"
  | "eyes_uncertain"
  | "group_integrity"
  | "face_soft"
  | "face_sharp"
  | "face_near_edge"
  | "face_partly_clipped"
  | "subject_near_edge"
  | "low_texture_or_blur"
  | "directional_detail"
  | "exposure_review"
  | "severe_clipping"
  | "noise_review"
  | "level_review"
  | "similar_alternative"
  | "preferred_candidate"
  | "bracket_like"
  | "selection_unchanged"
  | "exact_duplicate"
  | "preferred_copy"
  | "near_duplicate"
  | "burst_alternative"
  | "burst_sequence"
  | "similar_composition"
  | "duplicate_identity_unavailable"
  | "severe_subject_softness"
  | "group_focus_reference";
export interface CullingReason {
  code: ReasonCode;
  severity: "positive" | "info" | "review" | "issue" | "major";
  confidence: number;
  subject_index: number | null;
  measurement: { value: number; unit: string; reference: number | null } | null;
}
export interface CullingFeatures {
  asset_id: string;
  photo_type: PhotoType;
  source_fingerprint: string;
  source_analysis_id: string;
  source_analysis_version: number;
  feature_version: string;
  models: ProviderIdentity[];
  technical: {
    global_sharpness: number;
    global_edge_strength: number;
    noise_severity: Signal<number>;
    directional_detail: Signal<number>;
    subject_sharpness: Signal<number>;
  };
  people: {
    faces: Signal<FaceFeatures[]>;
    softest_subject: number | null;
    face_sharpness_spread: Signal<number>;
    outlier_subjects: number[];
  };
  framing: {
    subject_edge_distance: Signal<number>;
    subject_occupancy: Signal<number>;
  };
  composition: { level_angle: Signal<number>; aspect_ratio: number };
  exposure: {
    median_luminance: number;
    highlight_clip_fraction: number;
    shadow_clip_fraction: number;
    tonal_range: number;
    subject_background_ev: Signal<number>;
  };
  descriptor: {
    difference_hash: string;
    luminance_grid: number[];
    color_grid: number[];
    aspect_ratio: number;
    capture_timestamp: string | null;
    camera: string | null;
    mean_luminance: number;
  };
}
export type DuplicateKind =
  "exact" | "near_duplicate" | "burst" | "similar" | "unique";
export type RelationshipFilter =
  "all" | "exact" | "near_similar" | "preferred" | "unique";
export type DuplicateVisibility = "show" | "hide";
export interface CullingViewFilters {
  duplicates?: DuplicateVisibility;
  hideBlurry?: boolean;
  hideClosedEyes?: boolean;
}
export interface DuplicateContent {
  sha256: string;
  byte_length: number;
}
export interface SimilarityContext {
  group_id: string | null;
  group_size: number;
  preferred: boolean;
  preferred_assets: string[];
  relative_score: number | null;
  confidence: number;
  bracket_like: boolean;
  kind: DuplicateKind;
  similarity_score: number | null;
  exact: {
    group_id: string;
    group_size: number;
    canonical_asset_id: string;
    content: DuplicateContent;
  } | null;
}
export interface CullingAssessment {
  schema_version: 2;
  assessment_id: string;
  asset_id: string;
  created_at: string;
  photo_type: PhotoType;
  ai_rating: Stars | null;
  confidence: number;
  absolute_score: number;
  final_score: number;
  reasons: CullingReason[];
  features: CullingFeatures | null;
  similarity: SimilarityContext;
  duplicate_content: DuplicateContent | null;
  duplicate_stamp: string | null;
  membership_key: string | null;
  culling_engine_version: string;
  model_versions: ProviderIdentity[];
  source_analysis_id: string | null;
  source_fingerprint: string;
  cache_key: string;
}
export interface CullingState {
  assessment: CullingAssessment | null;
  user_rating: Stars | null;
  effective_rating: Stars | null;
  selected_for_editing: boolean;
  stale: boolean;
  updated_at: string | null;
}
export interface CullingItem {
  asset: Asset;
  ai_rating: Stars | null;
  user_rating: Stars | null;
  effective_rating: Stars | null;
  selected_for_editing: boolean;
  stale: boolean;
  group_id: string | null;
  preferred: boolean;
  review_count: number;
  relationship_kind: DuplicateKind | null;
  similarity: SimilarityContext | null;
  issues: CullingIssue[];
}
export type CullingIssue = "blurry" | "closed_eyes";
export interface CullingProgress {
  job_id: string;
  request_id: string;
  photo_type: PhotoType;
  status: string;
  stage: string;
  completed: number;
  total: number;
  failed: number;
  cached: number;
  duration_ms: number;
  error: string | null;
  hash_bytes: number;
  hash_cached: number;
  hash_duration_ms: number;
  hash_failures: number;
}
export interface CullingOverview {
  items: CullingItem[];
  counts: number[];
  selected_count: number;
  progress: CullingProgress | null;
  duplicates: {
    exact_copies: number;
    exact_groups: number;
    near_groups: number;
    burst_groups: number;
    similar_groups: number;
    unique_images: number;
    unclassified_images: number;
  };
  issue_availability: {
    blurry: boolean;
    closed_eyes: boolean;
  };
}
export interface CullingRequest {
  job_id: string;
  photo_type: PhotoType;
  request_id: string;
  force: boolean;
}
export const starValues: Stars[] = [1, 2, 3, 4, 5];
export const starText = (rating: Stars | null) =>
  rating === null
    ? "Not rated"
    : `${"★".repeat(rating)}${"☆".repeat(5 - rating)}`;
export const reasonText: Record<ReasonCode, string> = {
  technical_usable: "Technical measurements are usable",
  source_unavailable:
    "Source could not be analyzed; image quality is not rated",
  insufficient_evidence: "Limited evidence — photographer review recommended",
  face_detector_unavailable:
    "Face detector unavailable or failed; people were not assessed",
  no_faces_detected:
    "No relevant faces detected; check photo type and subject size",
  eyes_open: "Eyes reported open",
  eyes_closed: "Eyes reported closed",
  eyes_uncertain:
    "Eye state unavailable or uncertain — not a blink determination",
  group_integrity: "At least one person needs review",
  face_soft: "Face detail is substantially below the group median",
  face_sharp: "Face has strong local detail",
  face_near_edge: "Face lies near the frame boundary",
  face_partly_clipped:
    "Detected face box crosses the frame boundary; may be intentional",
  subject_near_edge: "Subject mask lies near the frame edge",
  low_texture_or_blur:
    "Low texture or blur; intent and focus cannot be determined",
  directional_detail: "Directional detail — possible motion or scene structure",
  exposure_review: "Exposure deserves review; may be intentional or editable",
  severe_clipping: "Extensive clipping with little retained detail",
  noise_review: "Noise estimate merits review",
  level_review: "Candidate level reference is tilted",
  similar_alternative: "A stronger candidate exists in this similar group",
  preferred_candidate: "Preferred candidate within this similar group",
  bracket_like: "Possible exposure bracket; retain alternatives as needed",
  selection_unchanged: "Editing selection is unchanged",
  exact_duplicate: "Exact duplicate of the preferred copy",
  preferred_copy:
    "Preferred copy of identical file content; normal quality rating retained",
  near_duplicate:
    "Near-duplicate frames; expressions and small subject changes still need review",
  burst_alternative:
    "Alternative in a likely burst / sequence; retain worthwhile expressions",
  burst_sequence:
    "Likely burst / sequence supported by capture time, camera and visual similarity",
  similar_composition:
    "Similar composition, potentially a different moment; no relative rating penalty",
  duplicate_identity_unavailable:
    "Full-file identity unavailable; exact duplication could not be checked",
  severe_subject_softness:
    "Very low reliable-scale face detail — likely unusable softness or blur",
  group_focus_reference: "Face detail compared with this visual group",
};
export const relationshipLabels: Record<DuplicateKind, string> = {
  exact: "Exact duplicate",
  near_duplicate: "Near duplicate",
  burst: "Burst / sequence",
  similar: "Similar composition",
  unique: "Unique image",
};
export function matchesRelationship(
  i: CullingItem,
  filter: RelationshipFilter,
) {
  if (filter === "exact") return i.relationship_kind === "exact";
  if (filter === "near_similar") return !!i.similarity?.group_id;
  if (filter === "preferred") return i.preferred;
  if (filter === "unique") return i.relationship_kind === "unique";
  return true;
}
export function exactSelectionEligible(
  item: CullingItem,
  excludeExactDuplicates: boolean,
) {
  if (!excludeExactDuplicates) return true;
  const exact = item.similarity?.exact;
  return !exact || exact.canonical_asset_id === item.asset.id;
}
export function relationshipBadge(i: CullingItem) {
  const s = i.similarity;
  if (s?.exact)
    return s.exact.canonical_asset_id === i.asset.id ? "BEST" : "DUPLICATE";
  if (s?.group_id) {
    if (s.kind === "similar") return "SIMILAR";
    return i.preferred ? "BEST" : "DUPLICATE";
  }
  return null;
}
export function duplicateFilterEligible(i: CullingItem) {
  const exact = i.similarity?.exact;
  if (exact && exact.canonical_asset_id !== i.asset.id) return false;
  const similarity = i.similarity;
  if (
    similarity?.group_id &&
    (similarity.kind === "near_duplicate" || similarity.kind === "burst")
  )
    return i.preferred;
  return true;
}
export function relationshipReason(
  r: CullingReason,
  a: CullingAssessment,
  items: CullingItem[],
) {
  const s = a.similarity;
  const name = (id: string) =>
    items.find((i) => i.asset.id === id)?.asset.filename ?? id;
  if (r.code === "exact_duplicate" && s.exact)
    return `Exact duplicate of ${name(s.exact.canonical_asset_id)}`;
  if (r.code === "near_duplicate")
    return `Near-duplicate of ${s.group_size - 1} other photographs; inspect expression and subject changes`;
  if (r.code === "preferred_candidate")
    return `Preferred technical candidate in a group of ${s.group_size} photographs`;
  if (r.code === "similar_alternative")
    return `Stronger measured technical candidate(s): ${s.preferred_assets.map(name).join(", ")}. ${s.kind === "similar" ? "Similar composition only; no rating penalty." : "Alternatives remain available for editing."}`;
  return reasonText[r.code];
}
export function filterItems(
  items: CullingItem[],
  ratings: Stars[],
  selectedOnly: boolean,
  sort: string,
  relationship: RelationshipFilter = "all",
  view: CullingViewFilters = {},
) {
  return items
    .filter(
      (i) =>
        (ratings.length === 0 ||
          (i.effective_rating !== null &&
            ratings.includes(i.effective_rating))) &&
        (!selectedOnly || i.selected_for_editing) &&
        matchesRelationship(i, relationship) &&
        (view.duplicates !== "hide" || duplicateFilterEligible(i)) &&
        (!view.hideBlurry || !i.issues.includes("blurry")) &&
        (!view.hideClosedEyes || !i.issues.includes("closed_eyes")),
    )
    .sort((a, b) =>
      sort === "rating"
        ? (b.effective_rating ?? 0) - (a.effective_rating ?? 0) ||
          a.asset.filename.localeCompare(b.asset.filename)
        : a.asset.filename.localeCompare(b.asset.filename),
    );
}
