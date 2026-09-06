import type { TrainingDataset, TrainingPair, TrainingRun } from "../training";
import { recipeFixture } from "./recipe-fixture";

export function trainingPairFixture(
  overrides: Partial<TrainingPair> = {},
): TrainingPair {
  return {
    schema_version: 1,
    pair_id: "pair-1",
    dataset_id: "dataset-1",
    source_job_id: "job-1",
    source_asset_id: "photo-1",
    source_path: "C:\\photos\\IMG_1001.CR3",
    reference_path: "C:\\edits\\IMG_1001_EDIT.jpg",
    photo_type: "portrait",
    source_fingerprint: "a".repeat(64),
    reference_fingerprint: "b".repeat(64),
    validation: {
      status: "ready",
      geometry: "exact_or_near",
      structural_similarity: 0.93,
      source_width: 6000,
      source_height: 4000,
      reference_width: 3000,
      reference_height: 2000,
      diagnostics: [],
    },
    source_analysis_id: "analysis-1",
    batch_context: null,
    scene_group_id: "scene-1",
    target: {
      schema_version: 1,
      optimizer_version: "staged-photographic-proxy-v1",
      cache_identity: "c".repeat(64),
      recipe: recipeFixture(),
      controls: {
        exposure_ev: 0.84,
        temperature_delta: 310,
        tint: 2,
        contrast: 0,
        highlights: -24,
        shadows: 28,
        whites: 4,
        blacks: -5,
        saturation: 2,
        vibrance: 7,
        texture: 0,
        clarity: 3,
        dehaze: 0,
        sharpening_amount: 0,
        noise_reduction: 0,
        vignette_amount: 0,
      },
      confidence: "medium",
      loss: {
        total: 0.047,
        luminance: 0.03,
        color_balance: 0.02,
        saturation: 0.01,
        structure: 0.08,
      },
      iterations: 61,
      unsupported_differences: ["crop preference is not learned in v1"],
      warnings: [],
    },
    split: "validation",
    excluded: false,
    feedback: null,
    diagnostics: [],
    ...overrides,
  };
}

export function trainingDatasetFixture(
  overrides: Partial<TrainingDataset> = {},
): TrainingDataset {
  return {
    schema_version: 1,
    dataset_id: "dataset-1",
    job_id: "job-1",
    style_name: "Jeoffrey Portrait",
    photo_type: "portrait",
    pairs: [trainingPairFixture()],
    created_at: "2026-09-05T00:00:00Z",
    updated_at: "2026-09-05T00:00:00Z",
    dataset_fingerprint: "d".repeat(64),
    feature_schema: "style_features_v1",
    renderer_version: "cpu-recipe-renderer-v1",
    target_recipe_schema: 1,
    batch_context_id: "e".repeat(64),
    warnings: [
      "1 training pairs — experimental dataset; more varied examples are recommended",
    ],
    before_files: ["C:\\photos\\IMG_1001.CR3"],
    after_files: ["C:\\edits\\IMG_1001_EDIT.jpg"],
    alignment: null,
    ...overrides,
  };
}

export function trainingRunFixture(
  overrides: Partial<TrainingRun> = {},
): TrainingRun {
  return {
    schema_version: 1,
    run_id: "run-1",
    dataset_id: "dataset-1",
    style_id: "jeoffrey-portrait-v1",
    style_name: "Jeoffrey Portrait",
    style_version: "1.0.0",
    status: "complete",
    stage: "complete",
    completed: 1,
    total: 1,
    config: {
      validation_percent: 20,
      regularization: 0.08,
      learning_rate: 0.025,
      epochs: 1200,
      exclude_low_confidence: true,
      deterministic: true,
    },
    metrics: {
      train: { recipe_mae: {}, mean_recipe_mae: 0.03, rendered_loss: 0.04 },
      validation: {
        recipe_mae: { exposure_ev: 0.12 },
        mean_recipe_mae: 0.05,
        rendered_loss: 0.06,
      },
      neutral_baseline: {
        recipe_mae: {},
        mean_recipe_mae: 0.2,
        rendered_loss: 0.21,
      },
      mean_baseline: {
        recipe_mae: {},
        mean_recipe_mae: 0.11,
        rendered_loss: 0.12,
      },
      beats_mean_baseline: true,
      overfitting_warning: null,
      warnings: [],
    },
    artifact_path: "C:\\app-data\\trained-styles\\jeoffrey-portrait-v1",
    started_at: "2026-09-05T00:00:00Z",
    updated_at: "2026-09-05T00:00:01Z",
    duration_ms: 1000,
    error: null,
    ...overrides,
  };
}
