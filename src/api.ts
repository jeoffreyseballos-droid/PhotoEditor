import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  Asset,
  Job,
  MachineResources,
  NewJobInput,
  Page,
  PhotoFormat,
  IngestionWarning,
  DevelopmentState,
  DevelopmentRequest,
  DevelopmentResult,
  RenderAdjustments,
} from "./types";

export const desktopAvailable = isTauri;

async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    throw new Error(
      "Open the desktop app to work with local photos. Run npm run desktop after installing the native prerequisites.",
    );
  }
  return invoke<T>(command, args);
}

export const api = {
  batchContextState: (
    jobId: string,
    photoType: import("./analysis").PhotoType,
  ) =>
    call<import("./batch-context").BatchContextState>("batch_context_state", {
      jobId,
      photoType,
    }),
  runBatchContext: (request: import("./batch-context").BatchContextRequest) =>
    call<import("./batch-context").BatchContextState>("run_batch_context", {
      request,
    }),
  batchContextProgress: (
    jobId: string,
    photoType: import("./analysis").PhotoType,
  ) =>
    call<import("./batch-context").BatchContextProgress | null>(
      "batch_context_progress",
      { jobId, photoType },
    ),
  cancelBatchContext: (requestId: string) =>
    call<void>("cancel_batch_context", { requestId }),
  builtinPresets: () =>
    call<import("./presets").BuiltInPreset[]>("builtin_presets"),
  presetEditingState: (jobId: string) =>
    call<import("./presets").PresetEditingState>("preset_editing_state", {
      jobId,
    }),
  applyBuiltInPreset: (
    jobId: string,
    presetId: import("./presets").BuiltInPresetId,
    assetIds: string[],
  ) =>
    call<import("./presets").PresetApplyResult>("apply_builtin_preset", {
      jobId,
      presetId,
      assetIds,
    }),
  cullingOverview: (jobId: string, photoType: import("./analysis").PhotoType) =>
    call<import("./culling").CullingOverview>("culling_overview", {
      jobId,
      photoType,
    }),
  cullingDetail: (
    jobId: string,
    assetId: string,
    photoType: import("./analysis").PhotoType,
  ) =>
    call<import("./culling").CullingState>("culling_detail", {
      jobId,
      assetId,
      photoType,
    }),
  cullingProgress: (jobId: string) =>
    call<import("./culling").CullingProgress | null>("culling_progress", {
      jobId,
    }),
  runCulling: (request: import("./culling").CullingRequest) =>
    call<import("./culling").CullingProgress>("run_culling", { request }),
  cancelCulling: (requestId: string) =>
    call<void>("cancel_culling", { requestId }),
  cullingRating: (
    jobId: string,
    assetId: string,
    photoType: import("./analysis").PhotoType,
    rating: import("./culling").Stars | null,
  ) => call<void>("culling_rating", { jobId, assetId, photoType, rating }),
  cullingSelectAsset: (jobId: string, assetId: string, selected: boolean) =>
    call<void>("culling_select_asset", { jobId, assetId, selected }),
  cullingSelectAssets: (
    jobId: string,
    photoType: import("./analysis").PhotoType,
    assetIds: string[],
  ) => call<void>("culling_select_assets", { jobId, photoType, assetIds }),
  cullingSelectRatings: (
    jobId: string,
    photoType: import("./analysis").PhotoType,
    ratings: import("./culling").Stars[],
    relationshipFilter: import("./culling").RelationshipFilter = "all",
    selectedOnly = false,
    excludeExactDuplicates = true,
  ) =>
    call<void>("culling_select_ratings", {
      jobId,
      photoType,
      ratings,
      relationshipFilter,
      selectedOnly,
      excludeExactDuplicates,
    }),
  getAnalysis: (
    jobId: string,
    assetId: string,
    photoType: import("./analysis").PhotoType,
  ) =>
    call<import("./analysis").AnalysisState>("get_analysis", {
      jobId,
      assetId,
      photoType,
    }),
  analyzeAsset: (request: import("./analysis").AnalysisRequest) =>
    call<import("./analysis").AnalysisState>("analyze_asset", { request }),
  cancelAnalysis: (requestId: string) =>
    call<void>("cancel_analysis", { requestId }),
  invalidateAnalysis: (jobId: string, assetId: string) =>
    call<void>("invalidate_analysis", { jobId, assetId }),
  exportAnalysis: (
    jobId: string,
    assetId: string,
    photoType: import("./analysis").PhotoType,
  ) => call<string>("export_analysis", { jobId, assetId, photoType }),
  saveRecipe: (
    jobId: string,
    assetId: string,
    recipe: import("./recipe").EditRecipe,
    expectedGeneration: number,
    reason: import("./recipe").RevisionReason | null = null,
  ) =>
    call<DevelopmentState>("save_recipe", {
      jobId,
      assetId,
      recipe,
      expectedGeneration,
      reason,
    }),
  renderRecipe: (
    request: Omit<DevelopmentRequest, "adjustments"> & {
      expected_generation: number;
      commit: boolean;
    },
  ) => call<DevelopmentResult>("render_recipe", { request }),
  recipeMask: (
    request: Omit<import("./toolkit").MaskRequest, "adjustments"> & {
      expected_generation: number;
    },
  ) => call<import("./toolkit").MaskResult>("recipe_mask", { request }),
  recipeHistory: (jobId: string, assetId: string, offset = 0, limit = 100) =>
    call<import("./recipe").RecipeRevision[]>("recipe_history", {
      jobId,
      assetId,
      offset,
      limit,
    }),
  restoreRecipe: (
    jobId: string,
    assetId: string,
    revisionId: string,
    expectedGeneration: number,
  ) =>
    call<DevelopmentState>("restore_recipe", {
      jobId,
      assetId,
      revisionId,
      expectedGeneration,
    }),
  recipeDiff: (jobId: string, assetId: string, revisionId: string) =>
    call<import("./recipe").RecipeDifference[]>("recipe_diff", {
      jobId,
      assetId,
      revisionId,
    }),
  exportRecipe: (jobId: string, assetId: string) =>
    call<string>("export_recipe", { jobId, assetId }),
  importRecipe: (
    jobId: string,
    assetId: string,
    path: string,
    expectedGeneration: number,
  ) =>
    call<DevelopmentState>("import_recipe", {
      jobId,
      assetId,
      path,
      expectedGeneration,
    }),
  recipeJson: (jobId: string, assetId: string) =>
    call<string>("recipe_json", { jobId, assetId }),
  chooseRecipe: async (): Promise<string | null> => {
    const result = await open({
      multiple: false,
      title: "Import an edit recipe",
      filters: [{ name: "Edit recipe JSON", extensions: ["json"] }],
    });
    return typeof result === "string" ? result : null;
  },
  developmentMask: (request: import("./toolkit").MaskRequest) =>
    call<import("./toolkit").MaskResult>("development_mask", { request }),
  development: (jobId: string, assetId: string) =>
    call<DevelopmentState>("get_development", { jobId, assetId }),
  saveDevelopment: (
    jobId: string,
    assetId: string,
    adjustments: RenderAdjustments,
  ) =>
    call<DevelopmentState>("save_development", { jobId, assetId, adjustments }),
  renderDevelopment: (request: DevelopmentRequest) =>
    call<DevelopmentResult>("render_development", { request }),
  cancelDevelopment: (requestId: string) =>
    call<void>("cancel_development", { requestId }),
  listJobs: (offset = 0, limit = 12) =>
    call<Page<Job>>("list_jobs", { offset, limit }),
  getJob: (jobId: string) => call<Job>("get_job", { jobId }),
  createJob: (input: NewJobInput) => call<Job>("create_job", { input }),
  resumeJob: (jobId: string) => call<Job>("resume_job", { jobId }),
  listAssets: (jobId: string, offset: number, limit: number) =>
    call<Page<Asset>>("list_assets", { jobId, offset, limit }),
  thumbnail: (jobId: string, assetId: string) =>
    call<string | null>("get_thumbnail", { jobId, assetId }),
  resources: () => call<MachineResources>("machine_resources"),
  formats: () => call<PhotoFormat[]>("photo_formats"),
  warnings: (jobId: string, offset = 0, limit = 100) =>
    call<Page<IngestionWarning>>("list_warnings", { jobId, offset, limit }),
  chooseFolder: async (title: string): Promise<string | null> => {
    if (!isTauri())
      throw new Error("Folder selection is available in the desktop app.");
    const result = await open({ directory: true, multiple: false, title });
    return typeof result === "string" ? result : null;
  },
};

export function errorMessage(error: unknown): string {
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    return error.message;
  }
  return "Something went wrong. Please try again. Technical details are in the local application log.";
}
