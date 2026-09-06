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
  matchValidateTrainingDataset: (datasetId: string, requestId: string) =>
    call<import("./training").TrainingDataset>(
      "match_validate_training_dataset",
      { datasetId, requestId },
    ),
  trainingMatchingProgress: (requestId: string) =>
    call<import("./training").MatchingProgress | null>(
      "training_matching_progress",
      { requestId },
    ),
  cancelTrainingMatching: (requestId: string) =>
    call<void>("cancel_training_matching", { requestId }),
  trainingDatasets: (jobId?: string) =>
    call<import("./training").TrainingDataset[]>("training_datasets", {
      ...(jobId ? { jobId } : {}),
    }),
  createTrainingDataset: (
    jobIdOrStyle: string,
    styleOrPhoto: string | import("./analysis").PhotoType,
    legacyPhotoType?: import("./analysis").PhotoType,
  ) => {
    const standalone = legacyPhotoType === undefined;
    const styleName = standalone ? jobIdOrStyle : (styleOrPhoto as string);
    const photoType = (
      standalone ? styleOrPhoto : legacyPhotoType
    ) as import("./analysis").PhotoType;
    return call<import("./training").TrainingDataset>(
      "create_training_dataset",
      {
        request: {
          job_id: standalone ? "" : jobIdOrStyle,
          style_name: styleName,
          photo_type: photoType,
        },
      },
    );
  },
  trainingDataset: (datasetId: string) =>
    call<import("./training").TrainingDataset>("training_dataset", {
      datasetId,
    }),
  addTrainingPair: (
    datasetId: string,
    sourceAssetId: string,
    referencePath: string,
  ) =>
    call<import("./training").TrainingDataset>("add_training_pair", {
      request: {
        dataset_id: datasetId,
        source_asset_id: sourceAssetId,
        reference_path: referencePath,
      },
    }),
  addTrainingBeforeFiles: (datasetId: string, paths: string[]) =>
    call<import("./training").TrainingDataset>("add_training_before_files", {
      request: { dataset_id: datasetId, paths },
    }),
  addTrainingAfterFiles: (datasetId: string, paths: string[]) =>
    call<import("./training").TrainingDataset>("add_training_after_files", {
      request: { dataset_id: datasetId, paths },
    }),
  addTrainingBeforeFolder: (datasetId: string, folder: string) =>
    call<import("./training").TrainingDataset>("add_training_before_folder", {
      request: { dataset_id: datasetId, folder },
    }),
  addTrainingAfterFolder: (datasetId: string, folder: string) =>
    call<import("./training").TrainingDataset>("add_training_after_folder", {
      request: { dataset_id: datasetId, folder },
    }),
  addTrainingPathPair: (
    datasetId: string,
    beforePath: string,
    afterPath: string,
  ) =>
    call<import("./training").TrainingDataset>("add_training_path_pair", {
      request: {
        dataset_id: datasetId,
        before_path: beforePath,
        after_path: afterPath,
      },
    }),
  matchTrainingDataset: (datasetId: string) =>
    call<import("./training").AutoMatchResult>("match_training_dataset", {
      datasetId,
    }),
  autoMatchTrainingFolder: (datasetId: string, folder: string) =>
    call<import("./training").AutoMatchResult>("auto_match_training_folder", {
      datasetId,
      folder,
    }),
  setTrainingPairExcluded: (
    datasetId: string,
    pairId: string,
    excluded: boolean,
  ) =>
    call<import("./training").TrainingDataset>("set_training_pair_excluded", {
      datasetId,
      pairId,
      excluded,
    }),
  validateTrainingDataset: (datasetId: string) =>
    call<import("./training").TrainingDataset>("validate_training_dataset", {
      datasetId,
    }),
  runTraining: (
    datasetId: string,
    requestId: string,
    config: import("./training").TrainingConfig,
  ) =>
    call<import("./training").TrainingRun>("run_training", {
      request: { dataset_id: datasetId, request_id: requestId, config },
    }),
  trainingProgress: (datasetId: string) =>
    call<import("./training").TrainingRun | null>("training_progress", {
      datasetId,
    }),
  cancelTraining: (requestId: string) =>
    call<void>("cancel_training", { requestId }),
  trainingPairPreviews: (datasetId: string, pairId: string) =>
    call<import("./training").TrainingPreviewSet>("training_pair_previews", {
      datasetId,
      pairId,
    }),
  trainingFeedback: (
    datasetId: string,
    pairId: string,
    feedback: import("./training").ValidationFeedback,
  ) =>
    call<import("./training").TrainingDataset>("training_feedback", {
      datasetId,
      pairId,
      feedback,
    }),
  prepareTrainingValidation: (datasetId: string) =>
    call<import("./training").ValidationEditingSelection>(
      "prepare_training_validation",
      { datasetId },
    ),
  trainedStyles: (photoType: import("./analysis").PhotoType) =>
    call<import("./trained-styles").StyleSummary[]>("trained_styles", {
      photoType,
    }),
  trainedStyleState: (
    jobId: string,
    photoType: import("./analysis").PhotoType,
  ) =>
    call<import("./trained-styles").StyleEditingState>("trained_style_state", {
      jobId,
      photoType,
    }),
  applyTrainedStyle: (request: import("./trained-styles").StyleApplyRequest) =>
    call<import("./trained-styles").StyleApplyResult>("apply_trained_style", {
      request,
    }),
  trainedStyleProgress: (
    jobId: string,
    photoType: import("./analysis").PhotoType,
  ) =>
    call<import("./trained-styles").StyleApplyProgress | null>(
      "trained_style_progress",
      { jobId, photoType },
    ),
  cancelTrainedStyle: (requestId: string) =>
    call<void>("cancel_trained_style", { requestId }),
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
  chooseTrainingReference: async (): Promise<string | null> => {
    const result = await open({
      multiple: false,
      title: "Choose the finished reference edit",
      filters: [
        {
          name: "Finished photo",
          extensions: ["jpg", "jpeg", "tif", "tiff", "png"],
        },
      ],
    });
    return typeof result === "string" ? result : null;
  },
  chooseTrainingFiles: async (
    title: string,
    role: "before" | "after" = "before",
  ): Promise<string[]> => {
    if (!isTauri())
      throw new Error("File selection is available in the desktop app.");
    const result = await open({
      multiple: false,
      directory: false,
      title,
      filters: [
        {
          name: "Photo files",
          extensions:
            role === "after"
              ? ["jpg", "jpeg", "tif", "tiff", "png"]
              : [
                  "cr3",
                  "cr2",
                  "arw",
                  "dng",
                  "jpg",
                  "jpeg",
                  "tif",
                  "tiff",
                  "png",
                ],
        },
      ],
    });
    return Array.isArray(result)
      ? result
      : typeof result === "string"
        ? [result]
        : [];
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
