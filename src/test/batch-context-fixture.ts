import type { BatchContext, BatchContextState } from "../batch-context";

export function batchContextFixture(
  assetIds = ["photo-1", "photo-2"],
): BatchContext {
  const sceneId = "1".repeat(64);
  const lightingId = "2".repeat(64);
  return {
    schema_version: 1,
    batch_id: "a".repeat(64),
    job_id: "job-1",
    photo_type: "portrait",
    selected_asset_ids: [...assetIds].sort(),
    selection_identity: "b".repeat(64),
    created_at: "2026-09-05T12:00:00Z",
    analysis_version: "photo-analysis-schema-1",
    grouping_version: "batch-context-test-v1",
    scene_groups: [
      {
        group_id: sceneId,
        asset_ids: [...assetIds].sort(),
        confidence: 0.84,
        reference_candidate_ids: [assetIds[0]],
      },
    ],
    lighting_groups: [
      {
        group_id: lightingId,
        asset_ids: [...assetIds].sort(),
        confidence: 0.8,
        reference_candidate_ids: [assetIds[0]],
      },
    ],
    sequence_groups: [],
    asset_contexts: assetIds.map((assetId, index) => ({
      asset_id: assetId,
      availability: "available",
      scene_group_id: sceneId,
      lighting_group_id: lightingId,
      sequence_group_id: null,
      reference_asset_id: assetIds[0],
      exposure_delta_from_group: {
        delta_ev: index === 0 ? 0 : -0.4,
        confidence: 0.8,
      },
      wb_delta_from_group: {
        warm_cool_delta: index === 0 ? 0 : 0.12,
        green_magenta_delta: 0,
        confidence: 0.8,
      },
      group_confidence: 0.8,
      consistency_notes: [],
    })),
    reference_candidates: [
      {
        group_kind: "scene",
        group_id: sceneId,
        asset_id: assetIds[0],
        rank: 1,
        technical_score: 92,
        confidence: 0.86,
        reasons: ["Stable source"],
      },
      {
        group_kind: "lighting",
        group_id: lightingId,
        asset_id: assetIds[0],
        rank: 1,
        technical_score: 92,
        confidence: 0.86,
        reasons: ["Stable source"],
      },
    ],
    diagnostics: {
      available_assets: assetIds.length,
      partial_assets: 0,
      unavailable_assets: 0,
      candidate_comparisons: 3,
      candidate_limit_per_asset: 64,
      timings: {
        loading_ms: 2,
        candidate_generation_ms: 1,
        grouping_ms: 1,
        context_ms: 1,
        persistence_ms: 1,
        total_ms: 6,
      },
      warnings: [],
    },
  };
}

export function batchContextStateFixture(
  context: BatchContext | null = batchContextFixture(),
  stale = false,
): BatchContextState {
  return {
    selected_count: context?.selected_asset_ids.length ?? 2,
    selection_identity: context?.selection_identity ?? "c".repeat(64),
    context,
    cached: context !== null,
    stale,
    progress: null,
  };
}
