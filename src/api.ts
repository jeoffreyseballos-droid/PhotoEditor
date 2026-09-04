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
