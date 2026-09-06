import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../api";
import type { CullingOverview } from "../culling";
import type { BuiltInPreset } from "../presets";
import type { StyleAssetInference, StyleSummary } from "../trained-styles";
import { PresetEditingScreen } from "../screens/PresetEditingScreen";
import { asset, job } from "./fixtures";
import { developmentStateFixture } from "./recipe-fixture";

vi.mock("../api", () => ({
  api: {
    batchContextState: vi.fn(),
    runBatchContext: vi.fn(),
    batchContextProgress: vi.fn(),
    cancelBatchContext: vi.fn(),
    trainedStyleState: vi.fn(),
    applyTrainedStyle: vi.fn(),
    trainedStyleProgress: vi.fn(),
    cancelTrainedStyle: vi.fn(),
    builtinPresets: vi.fn(),
    presetEditingState: vi.fn(),
    applyBuiltInPreset: vi.fn(),
    cullingOverview: vi.fn(),
    development: vi.fn(),
    recipeMask: vi.fn(),
    renderRecipe: vi.fn(),
    cancelDevelopment: vi.fn(),
    thumbnail: vi.fn(),
  },
  errorMessage: (error: Error) => error.message,
}));

const presets: BuiltInPreset[] = [
  {
    id: "pop",
    name: "POP",
    description: "Bright, clean subject emphasis",
    version: "1",
  },
  {
    id: "warm",
    name: "WARM",
    description: "Warmer overall color balance",
    version: "1",
  },
  {
    id: "black_and_white",
    name: "BLACK & WHITE",
    description: "Classic monochrome",
    version: "1",
  },
];
const selectedAssets = [asset("photo-1"), asset("photo-2")];
const adaptiveStyle: StyleSummary = {
  style_id: "adaptive-natural-development",
  name: "Adaptive Natural — Development",
  version: "1.0.0",
  model_version: "adaptive-natural-linear-2026-09-v1",
  package_identity: "a".repeat(64),
  photo_type: "portrait",
  description: "Development-only adaptive style",
  development_only: true,
};
const adaptiveInference = (
  assetId: string,
  exposure: number,
): StyleAssetInference => ({
  job_id: job.id,
  asset_id: assetId,
  style_id: adaptiveStyle.style_id,
  style_version: adaptiveStyle.version,
  model_version: adaptiveStyle.model_version,
  package_identity: adaptiveStyle.package_identity,
  feature_schema: "style_features_v1",
  input_identity: "b".repeat(64),
  analysis_id: `analysis-${assetId}`,
  batch_context_id: "c".repeat(64),
  status: "applied",
  prediction: {
    style_id: adaptiveStyle.style_id,
    style_version: adaptiveStyle.version,
    model_version: adaptiveStyle.model_version,
    package_identity: adaptiveStyle.package_identity,
    feature_schema: "style_features_v1",
    adjustments: {
      exposure_ev: exposure,
      temperature_delta: assetId === "photo-1" ? -120 : 80,
      tint: 0,
      contrast: 4,
      highlights: -12,
      shadows: 16,
      whites: 2,
      blacks: -2,
      saturation: 1,
      vibrance: 5,
      texture: 3,
      clarity: 3,
      dehaze: 1,
      sharpening_amount: 25,
      noise_reduction: 4,
      vignette_amount: -2,
    },
    confidence: "medium",
    confidence_score: 0.76,
    diagnostics: {
      resolver: "photo-editor-linear-style-v1",
      missing_feature_count: 1,
      bounded_controls: [],
      warnings: [],
    },
  },
  feature_summary: {
    median_luminance: exposure > 0 ? 0.2 : 0.72,
    batch_exposure_delta_ev: exposure,
    warm_cool_balance: 0.04,
    batch_warm_cool_delta: -0.08,
    group_confidence: 0.85,
    missing_feature_count: 1,
  },
  recipe_hash: `recipe-${assetId}`,
  error: null,
  stale: false,
});

function overview(): CullingOverview {
  return {
    items: [...selectedAssets, asset("photo-3")].map((photo, index) => ({
      asset: photo,
      ai_rating: 4,
      user_rating: null,
      effective_rating: 4,
      selected_for_editing: index < 2,
      stale: false,
      group_id: null,
      preferred: false,
      review_count: 0,
      relationship_kind: "unique",
      similarity: null,
      issues: [],
    })),
    counts: [0, 0, 0, 0, 3, 0],
    selected_count: 2,
    progress: null,
    issue_availability: { blurry: true, closed_eyes: false },
    duplicates: {
      exact_copies: 0,
      exact_groups: 0,
      near_groups: 0,
      burst_groups: 0,
      similar_groups: 0,
      unique_images: 3,
      unclassified_images: 0,
    },
  };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.batchContextState).mockResolvedValue({
    selected_count: 2,
    selection_identity: "b".repeat(64),
    context: null,
    cached: false,
    stale: false,
    progress: null,
  });
  vi.mocked(api.batchContextProgress).mockResolvedValue(null);
  vi.mocked(api.cancelBatchContext).mockResolvedValue();
  vi.mocked(api.trainedStyleState).mockResolvedValue({
    styles: [],
    selected_asset_ids: ["photo-1", "photo-2"],
    applied_style: null,
    applied_count: 0,
    stale_asset_ids: [],
    needs_review: [],
    inferences: [],
    progress: null,
  });
  vi.mocked(api.trainedStyleProgress).mockResolvedValue(null);
  vi.mocked(api.cancelTrainedStyle).mockResolvedValue();
  vi.mocked(api.builtinPresets).mockResolvedValue(presets);
  vi.mocked(api.presetEditingState).mockResolvedValue({
    selected_asset_ids: ["photo-1", "photo-2"],
    applied_preset: null,
    applied_count: 0,
    unresolved_subject_masks: [],
  });
  vi.mocked(api.cullingOverview).mockResolvedValue(overview());
  vi.mocked(api.thumbnail).mockResolvedValue(null);
  vi.mocked(api.development).mockResolvedValue(developmentStateFixture());
  vi.mocked(api.recipeMask).mockResolvedValue({
    diagnostic: {
      status: "ready",
      reference: "mask-v1",
      model_version: "modnet-v1",
      cache_path: "C:/Cache/mask.png",
      width: 512,
      height: 512,
      confidence: null,
      warnings: [],
    },
    overlay_data: null,
  });
  vi.mocked(api.renderRecipe).mockImplementation(async (request) => ({
    state: developmentStateFixture(
      undefined,
      request.preview
        ? {}
        : {
            state: "exported",
            export_path: `C:/Output/${request.asset_id}-edited.jpg`,
          },
    ),
    preview_data: request.preview
      ? `data:image/jpeg;base64,edited-${request.asset_id}`
      : null,
    width: request.preview ? 1600 : 6000,
    height: request.preview ? 1067 : 4000,
  }));
  vi.mocked(api.cancelDevelopment).mockResolvedValue();
  vi.mocked(api.applyBuiltInPreset).mockResolvedValue({
    preset: presets[0],
    selected_asset_ids: ["photo-1", "photo-2"],
    recipes_updated: 2,
    recipes_unchanged: 0,
    unresolved_subject_masks: ["photo-2"],
  });
});

function open(onBack = vi.fn()) {
  render(
    <PresetEditingScreen
      jobId={job.id}
      photoType="portrait"
      initialSelectedAssetIds={["photo-1", "photo-2"]}
      onBack={onBack}
    />,
  );
  return onBack;
}

it("applies POP, prepares masks and shows only rendered edited previews", async () => {
  vi.mocked(api.recipeMask).mockImplementation(async (request) => ({
    diagnostic: {
      status: request.asset_id === "photo-1" ? "ready" : "failed",
      reference: null,
      model_version: "modnet-v1",
      cache_path: null,
      width: 0,
      height: 0,
      confidence: null,
      warnings: request.asset_id === "photo-1" ? [] : ["No subject found"],
    },
    overlay_data: null,
  }));
  open();
  expect(await screen.findByText("Choose a preset")).toBeInTheDocument();
  expect(screen.getByText("2 photos selected")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Apply Preset" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: /POP/ }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  await screen.findByText("Editing — POP");
  expect(api.applyBuiltInPreset).toHaveBeenCalledWith(job.id, "pop", [
    "photo-1",
    "photo-2",
  ]);
  expect(screen.getByText("2 recipes created or updated")).toBeInTheDocument();
  expect(
    await screen.findByText(/Subject mask could not be prepared for 1 photos/),
  ).toHaveTextContent("need attention");
  expect(api.recipeMask).toHaveBeenCalledTimes(2);
  expect(api.recipeMask).toHaveBeenNthCalledWith(
    1,
    expect.objectContaining({
      asset_id: "photo-1",
      layer_id: "built-in-pop-subject-v1",
      generate: true,
    }),
  );
  expect(api.renderRecipe).toHaveBeenCalledTimes(2);
  expect(
    screen.getByLabelText("POP edited preview for photo-1.nef"),
  ).toHaveAttribute("src", "data:image/jpeg;base64,edited-photo-1");
  expect(
    screen.getByLabelText("POP edited preview for photo-2.nef"),
  ).toHaveAttribute("src", "data:image/jpeg;base64,edited-photo-2");
  expect(api.thumbnail).not.toHaveBeenCalled();
  expect(
    screen.getByRole("button", { name: "Select photo-1.nef" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Select photo-2.nef" }),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Select photo-3.nef" }),
  ).toBeNull();
});

it("replaces a black-and-white rendered preview with the warm recipe preview", async () => {
  let renderedPreset = "black-and-white";
  vi.mocked(api.renderRecipe).mockImplementation(async (request) => ({
    state: developmentStateFixture(),
    preview_data: `data:image/jpeg;base64,${renderedPreset}-${request.asset_id}`,
    width: 1600,
    height: 1067,
  }));
  open();
  fireEvent.click(await screen.findByRole("button", { name: /BLACK & WHITE/ }));
  vi.mocked(api.applyBuiltInPreset).mockResolvedValueOnce({
    preset: presets[2],
    selected_asset_ids: ["photo-1", "photo-2"],
    recipes_updated: 2,
    recipes_unchanged: 0,
    unresolved_subject_masks: [],
  });
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  await screen.findByText("Editing — BLACK & WHITE");
  expect(
    await screen.findByLabelText(
      "BLACK & WHITE edited preview for photo-1.nef",
    ),
  ).toHaveAttribute("src", "data:image/jpeg;base64,black-and-white-photo-1");
  fireEvent.click(screen.getByRole("button", { name: "Change Preset" }));
  fireEvent.click(screen.getByRole("button", { name: /WARM/ }));
  renderedPreset = "warm";
  vi.mocked(api.applyBuiltInPreset).mockResolvedValueOnce({
    preset: presets[1],
    selected_asset_ids: ["photo-1", "photo-2"],
    recipes_updated: 2,
    recipes_unchanged: 0,
    unresolved_subject_masks: [],
  });
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  await screen.findByText("Editing — WARM");
  expect(api.applyBuiltInPreset).toHaveBeenLastCalledWith(job.id, "warm", [
    "photo-1",
    "photo-2",
  ]);
  expect(
    await screen.findByLabelText("WARM edited preview for photo-1.nef"),
  ).toHaveAttribute("src", "data:image/jpeg;base64,warm-photo-1");
  expect(
    screen.queryByText("data:image/jpeg;base64,black-and-white-photo-1"),
  ).toBeNull();
  expect(screen.queryByText(/Subject mask could not/)).toBeNull();
});

it("restores the applied preset and safe unresolved-mask state after reopening", async () => {
  vi.mocked(api.presetEditingState).mockResolvedValue({
    selected_asset_ids: ["photo-1", "photo-2"],
    applied_preset: "pop",
    applied_count: 2,
    unresolved_subject_masks: ["photo-1", "photo-2"],
  });
  vi.mocked(api.recipeMask).mockImplementation(async () => ({
    diagnostic: {
      status: "failed",
      reference: null,
      model_version: "modnet-v1",
      cache_path: null,
      width: 0,
      height: 0,
      confidence: null,
      warnings: ["No subject found"],
    },
    overlay_data: null,
  }));
  const onBack = open();
  await screen.findByText("Editing — POP");
  expect(screen.getByText("2 recipes already saved")).toBeInTheDocument();
  expect(
    await screen.findByText(/Subject mask could not be prepared for 2 photos/),
  ).toBeInTheDocument();
  expect(api.applyBuiltInPreset).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Back to Culling" }));
  expect(onBack).toHaveBeenCalledOnce();
});

it("keeps an empty persisted selection safely out of the editing action", async () => {
  vi.mocked(api.presetEditingState).mockResolvedValue({
    selected_asset_ids: [],
    applied_preset: null,
    applied_count: 0,
    unresolved_subject_masks: [],
  });
  open();
  await screen.findByText(
    "Return to culling and select at least one photograph.",
  );
  fireEvent.click(screen.getByRole("button", { name: /POP/ }));
  expect(screen.getByRole("button", { name: "Apply Preset" })).toBeDisabled();
  await waitFor(() => expect(api.applyBuiltInPreset).not.toHaveBeenCalled());
});

it("continues rendering after one POP mask fails and exposes cancellable bounded progress", async () => {
  let releaseMask!: (value: Awaited<ReturnType<typeof api.recipeMask>>) => void;
  vi.mocked(api.recipeMask)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          releaseMask = resolve;
        }),
    )
    .mockResolvedValueOnce({
      diagnostic: {
        status: "failed",
        reference: null,
        model_version: "modnet-v1",
        cache_path: null,
        width: 0,
        height: 0,
        confidence: null,
        warnings: ["No subject found"],
      },
      overlay_data: null,
    });
  open();
  fireEvent.click(await screen.findByRole("button", { name: /POP/ }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  expect(
    await screen.findByText(/Preparing subject masks... 0 \/ 2/),
  ).toBeInTheDocument();
  expect(api.recipeMask).toHaveBeenCalledTimes(1);
  releaseMask({
    diagnostic: {
      status: "ready",
      reference: "cached-mask",
      model_version: "modnet-v1",
      cache_path: "C:/Cache/mask.png",
      width: 512,
      height: 512,
      confidence: null,
      warnings: [],
    },
    overlay_data: null,
  });
  await screen.findByText(/Edited previews ready · 2 \/ 2/);
  expect(api.recipeMask).toHaveBeenCalledTimes(2);
  expect(api.renderRecipe).toHaveBeenCalledTimes(2);
  expect(screen.getByText("Needs attention")).toBeInTheDocument();
});

it("cancels the active preset mask request without starting the remaining batch", async () => {
  let releaseMask!: (value: Awaited<ReturnType<typeof api.recipeMask>>) => void;
  vi.mocked(api.recipeMask).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        releaseMask = resolve;
      }),
  );
  open();
  fireEvent.click(await screen.findByRole("button", { name: /POP/ }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  await screen.findByText(/Preparing subject masks... 0 \/ 2/);
  const requestId = vi.mocked(api.recipeMask).mock.calls[0][0].request_id;
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(api.cancelDevelopment).toHaveBeenCalledWith(requestId);
  expect(
    screen.getByText(
      "Preview preparation stopped. Completed previews are preserved.",
    ),
  ).toBeInTheDocument();
  releaseMask({
    diagnostic: {
      status: "ready",
      reference: "mask-v1",
      model_version: "modnet-v1",
      cache_path: "C:/Cache/mask.png",
      width: 512,
      height: 512,
      confidence: null,
      warnings: [],
    },
    overlay_data: null,
  });
  await waitFor(() => expect(api.recipeMask).toHaveBeenCalledTimes(1));
  expect(api.renderRecipe).not.toHaveBeenCalled();
});

it("exports only the explicit editing selection sequentially with progress", async () => {
  let releaseExport!: (
    value: Awaited<ReturnType<typeof api.renderRecipe>>,
  ) => void;
  vi.mocked(api.applyBuiltInPreset).mockResolvedValueOnce({
    preset: presets[2],
    selected_asset_ids: ["photo-1", "photo-2"],
    recipes_updated: 2,
    recipes_unchanged: 0,
    unresolved_subject_masks: [],
  });
  vi.mocked(api.renderRecipe).mockImplementation((request) => {
    if (request.preview)
      return Promise.resolve({
        state: developmentStateFixture(),
        preview_data: `data:image/jpeg;base64,edited-${request.asset_id}`,
        width: 1600,
        height: 1067,
      });
    if (request.asset_id === "photo-1")
      return new Promise((resolve) => {
        releaseExport = resolve;
      });
    return Promise.resolve({
      state: developmentStateFixture(undefined, {
        state: "exported",
        export_path: `C:/Output/${request.asset_id}-edited.jpg`,
      }),
      preview_data: null,
      width: 6000,
      height: 4000,
    });
  });
  open();
  fireEvent.click(await screen.findByRole("button", { name: /BLACK & WHITE/ }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  await screen.findByText(/Edited previews ready · 2 \/ 2/);
  fireEvent.click(screen.getByRole("button", { name: "Export All" }));
  expect(await screen.findByText(/Exporting... 0 \/ 2/)).toBeInTheDocument();
  expect(
    vi
      .mocked(api.renderRecipe)
      .mock.calls.filter(([request]) => !request.preview),
  ).toHaveLength(1);
  releaseExport({
    state: developmentStateFixture(undefined, {
      state: "exported",
      export_path: "C:/Output/photo-1-edited.jpg",
    }),
    preview_data: null,
    width: 6000,
    height: 4000,
  });
  await screen.findByText(/Export complete · 2 files exported · 0 failed/);
  const exports = vi
    .mocked(api.renderRecipe)
    .mock.calls.map(([request]) => request)
    .filter((request) => !request.preview);
  expect(exports.map((request) => request.asset_id)).toEqual([
    "photo-1",
    "photo-2",
  ]);
  expect(exports).toEqual(
    expect.arrayContaining([
      expect.objectContaining({
        asset_id: "photo-1",
        preview: false,
        output_format: "jpeg",
        jpeg_quality: 95,
        commit: true,
      }),
      expect.objectContaining({
        asset_id: "photo-2",
        preview: false,
        output_format: "jpeg",
        commit: true,
      }),
    ]),
  );
  expect(exports.some((request) => request.asset_id === "photo-3")).toBe(false);
});

it("exports valid neutral recipes and cancels before starting the next selected asset", async () => {
  let releaseExport!: (
    value: Awaited<ReturnType<typeof api.renderRecipe>>,
  ) => void;
  vi.mocked(api.renderRecipe).mockImplementation(
    () =>
      new Promise((resolve) => {
        releaseExport = resolve;
      }),
  );
  open();
  await screen.findByText("Choose a preset");
  const exportButton = screen.getByRole("button", { name: "Export All" });
  expect(exportButton).toBeEnabled();
  fireEvent.click(exportButton);
  expect(await screen.findByText(/Exporting... 0 \/ 2/)).toBeInTheDocument();
  expect(api.renderRecipe).toHaveBeenCalledTimes(1);
  const request = vi.mocked(api.renderRecipe).mock.calls[0][0];
  expect(request).toEqual(
    expect.objectContaining({
      asset_id: "photo-1",
      preview: false,
      jpeg_quality: 95,
      commit: true,
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Cancel Export" }));
  expect(api.cancelDevelopment).toHaveBeenCalledWith(request.request_id);
  expect(
    screen.getByText(
      "Export stopped. Files already completed remain in the output folder.",
    ),
  ).toBeInTheDocument();
  releaseExport({
    state: developmentStateFixture(undefined, {
      state: "exported",
      export_path: "C:/Output/photo-1-edited.jpg",
    }),
    preview_data: null,
    width: 6000,
    height: 4000,
  });
  await waitFor(() => expect(api.renderRecipe).toHaveBeenCalledTimes(1));
  expect(screen.queryByText(/Export complete/)).toBeNull();
});

it("continues Export All after one selected asset fails", async () => {
  vi.mocked(api.applyBuiltInPreset).mockResolvedValueOnce({
    preset: presets[1],
    selected_asset_ids: ["photo-1", "photo-2"],
    recipes_updated: 2,
    recipes_unchanged: 0,
    unresolved_subject_masks: [],
  });
  vi.mocked(api.renderRecipe).mockImplementation(async (request) => {
    if (request.preview)
      return {
        state: developmentStateFixture(),
        preview_data: `data:image/jpeg;base64,edited-${request.asset_id}`,
        width: 1600,
        height: 1067,
      };
    if (request.asset_id === "photo-1") throw new Error("Disk write failed");
    return {
      state: developmentStateFixture(undefined, {
        state: "exported",
        export_path: "C:/Output/photo-2-edited.jpg",
      }),
      preview_data: null,
      width: 6000,
      height: 4000,
    };
  });
  open();
  fireEvent.click(await screen.findByRole("button", { name: /WARM/ }));
  fireEvent.click(screen.getByRole("button", { name: "Apply Preset" }));
  await screen.findByText(/Edited previews ready · 2 \/ 2/);
  fireEvent.click(screen.getByRole("button", { name: "Export All" }));
  await screen.findByText(/Export complete · 1 file exported · 1 failed/);
  const exports = vi
    .mocked(api.renderRecipe)
    .mock.calls.map(([request]) => request)
    .filter((request) => !request.preview);
  expect(exports.map((request) => request.asset_id)).toEqual([
    "photo-1",
    "photo-2",
  ]);
  expect(screen.getByText("Needs attention")).toBeInTheDocument();
});

it("reloads the persisted selection instead of stale handoff IDs after returning from culling", async () => {
  vi.mocked(api.presetEditingState).mockResolvedValueOnce({
    selected_asset_ids: ["photo-1", "photo-2"],
    applied_preset: "black_and_white",
    applied_count: 2,
    unresolved_subject_masks: [],
  });
  const onBack = vi.fn();
  const first = render(
    <PresetEditingScreen
      jobId={job.id}
      photoType="portrait"
      initialSelectedAssetIds={["photo-1", "photo-2"]}
      onBack={onBack}
    />,
  );
  await screen.findByLabelText("BLACK & WHITE edited preview for photo-1.nef");
  fireEvent.click(screen.getByRole("button", { name: "Back to Culling" }));
  expect(onBack).toHaveBeenCalledOnce();
  first.unmount();

  const refreshed = overview();
  refreshed.items.forEach((item) => {
    item.selected_for_editing = item.asset.id === "photo-3";
  });
  refreshed.selected_count = 1;
  vi.mocked(api.cullingOverview).mockResolvedValueOnce(refreshed);
  vi.mocked(api.presetEditingState).mockResolvedValueOnce({
    selected_asset_ids: ["photo-3"],
    applied_preset: "warm",
    applied_count: 1,
    unresolved_subject_masks: [],
  });
  render(
    <PresetEditingScreen
      jobId={job.id}
      photoType="portrait"
      initialSelectedAssetIds={["photo-1", "photo-2"]}
      onBack={() => {}}
    />,
  );
  await screen.findByLabelText("WARM edited preview for photo-3.nef");
  expect(
    screen.queryByRole("button", { name: "Select photo-1.nef" }),
  ).toBeNull();
  expect(
    screen.queryByRole("button", { name: "Select photo-2.nef" }),
  ).toBeNull();
  expect(
    screen.getByRole("button", { name: "Select photo-3.nef" }),
  ).toBeInTheDocument();
});

it("applies the development AI style only to the persisted selection and shows prediction details", async () => {
  vi.mocked(api.trainedStyleState).mockResolvedValue({
    styles: [adaptiveStyle],
    selected_asset_ids: ["photo-1", "photo-2"],
    applied_style: null,
    applied_count: 0,
    stale_asset_ids: [],
    needs_review: [],
    inferences: [],
    progress: null,
  });
  vi.mocked(api.applyTrainedStyle).mockResolvedValue({
    style: adaptiveStyle,
    selected_asset_ids: ["photo-1", "photo-2"],
    predictions_attempted: 2,
    predictions_succeeded: 2,
    predictions_failed: 0,
    recipes_updated: 2,
    recipes_unchanged: 0,
    needs_review: [],
    inferences: [
      adaptiveInference("photo-1", 0.64),
      adaptiveInference("photo-2", 0.08),
    ],
    duration_ms: 4,
  });
  open();
  fireEvent.click(
    await screen.findByRole("button", { name: /Adaptive Natural/ }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Apply AI Style" }));
  await screen.findByText(`AI Style: ${adaptiveStyle.name}`);
  expect(api.applyTrainedStyle).toHaveBeenCalledWith(
    expect.objectContaining({
      job_id: job.id,
      style_id: adaptiveStyle.style_id,
      selected_asset_ids: ["photo-1", "photo-2"],
    }),
  );
  expect(
    vi.mocked(api.applyTrainedStyle).mock.calls[0][0].selected_asset_ids,
  ).toHaveLength(2);
  expect(api.renderRecipe).toHaveBeenCalledTimes(2);
  expect(
    await screen.findByLabelText(
      `${adaptiveStyle.name} edited preview for photo-1.nef`,
    ),
  ).toHaveAttribute("src", "data:image/jpeg;base64,edited-photo-1");
  fireEvent.click(screen.getByRole("button", { name: "Select photo-1.nef" }));
  expect(
    await screen.findByText(/Style Inference details/),
  ).toBeInTheDocument();
  expect(screen.getByText("medium")).toBeInTheDocument();
  expect(screen.getAllByText("+0.64 EV")).toHaveLength(2);
});

it("cancels an in-flight AI style request without starting preview work", async () => {
  vi.mocked(api.trainedStyleState).mockResolvedValue({
    styles: [adaptiveStyle],
    selected_asset_ids: ["photo-1", "photo-2"],
    applied_style: null,
    applied_count: 0,
    stale_asset_ids: [],
    needs_review: [],
    inferences: [],
    progress: null,
  });
  vi.mocked(api.applyTrainedStyle).mockImplementation(
    () => new Promise(() => {}),
  );
  open();
  fireEvent.click(
    await screen.findByRole("button", { name: /Adaptive Natural/ }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Apply AI Style" }));
  expect(
    await screen.findByText(/Applying Adaptive Natural/),
  ).toBeInTheDocument();
  const requestId = vi.mocked(api.applyTrainedStyle).mock.calls[0][0]
    .request_id;
  fireEvent.click(screen.getByRole("button", { name: "Cancel Style" }));
  await waitFor(() =>
    expect(api.cancelTrainedStyle).toHaveBeenCalledWith(requestId),
  );
  expect(
    screen.getByText(
      "AI style stopped. Completed recipes are preserved; remaining photographs were not changed.",
    ),
  ).toBeInTheDocument();
  expect(api.renderRecipe).not.toHaveBeenCalled();
});
