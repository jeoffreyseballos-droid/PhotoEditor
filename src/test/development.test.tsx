import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { DevelopmentPanel } from "../components/DevelopmentPanel";
import { api } from "../api";
import { asset } from "./fixtures";
import { neutralAdjustments, neutralLocal } from "../toolkit";
import type { MaskDiagnostic } from "../toolkit";
import { developmentStateFixture, recipeStateFixture } from "./recipe-fixture";
import type { DevelopmentFixture } from "./recipe-fixture";
import { recipeControls } from "../recipe";
vi.mock("../api", () => ({
  errorMessage: (e: { message: string }) => e.message,
  api: {
    development: vi.fn(),
    saveRecipe: vi.fn(),
    thumbnail: vi.fn(),
    renderRecipe: vi.fn(),
    cancelDevelopment: vi.fn(),
    recipeMask: vi.fn(),
    recipeHistory: vi.fn(),
    restoreRecipe: vi.fn(),
    recipeDiff: vi.fn(),
    exportRecipe: vi.fn(),
    importRecipe: vi.fn(),
    recipeJson: vi.fn(),
    chooseRecipe: vi.fn(),
  },
}));
const state = developmentStateFixture(
  recipeStateFixture({
    ...neutralAdjustments(),
    exposure_ev: 1.2,
  }),
  { revision: 2 },
);
let savedState: DevelopmentFixture;
beforeEach(() => {
  vi.resetAllMocks();
  savedState = structuredClone(state);
  vi.mocked(api.development).mockImplementation(async () =>
    structuredClone(savedState),
  );
  vi.mocked(api.saveRecipe).mockImplementation(
    async (_job, _asset, recipe, generation, reason) => {
      expect(generation).toBe(savedState.recipe_state!.generation);
      savedState = {
        ...savedState,
        ...developmentStateFixture(
          {
            ...savedState.recipe_state,
            recipe,
            recipe_hash: (generation + 1).toString(16).padStart(64, "0"),
            generation: generation + 1,
            current_revision:
              savedState.recipe_state.current_revision + (reason ? 1 : 0),
            modified: !reason,
            error: null,
          },
          {
            revision: savedState.revision + 1,
            export_path: savedState.export_path,
          },
        ),
      };
      return structuredClone(savedState);
    },
  );
  vi.mocked(api.recipeHistory).mockResolvedValue([
    {
      revision_id: "revision-1",
      revision_number: 1,
      recipe_hash: "a".repeat(64),
      origin: "manual",
      reason: "initial",
      created_at: "2026-09-04T00:00:00Z",
    },
  ]);
  vi.mocked(api.recipeDiff).mockResolvedValue([
    { control: "Basic / Exposure (EV)", before: 0, after: 1.2 },
  ]);
  vi.mocked(api.recipeJson).mockResolvedValue(
    JSON.stringify(state.recipe_state!.recipe),
  );
  vi.mocked(api.exportRecipe).mockResolvedValue("C:/Output/photo.recipe.json");
  vi.mocked(api.chooseRecipe).mockResolvedValue("C:/Output/photo.recipe.json");
  vi.mocked(api.importRecipe).mockImplementation(
    async (_job, _asset, _path, generation) => {
      expect(generation).toBe(savedState.recipe_state.generation);
      const imported = recipeStateFixture(state.adjustments);
      imported.recipe.provenance.origin = "imported";
      imported.recipe.provenance.source_recipe_id = "imported-source-recipe";
      savedState = developmentStateFixture({
        ...imported,
        generation: generation + 1,
        current_revision: savedState.recipe_state.current_revision + 1,
      });
      return structuredClone(savedState);
    },
  );
  vi.mocked(api.restoreRecipe).mockImplementation(
    async (_job, _asset, _revision, generation) => {
      expect(generation).toBe(savedState.recipe_state.generation);
      savedState = developmentStateFixture({
        ...recipeStateFixture(),
        generation: generation + 1,
        current_revision: savedState.recipe_state.current_revision + 1,
      });
      return structuredClone(savedState);
    },
  );
  vi.mocked(api.thumbnail).mockResolvedValue(
    "data:image/jpeg;base64,b3JpZ2luYWw=",
  );
  vi.mocked(api.cancelDevelopment).mockResolvedValue();
  vi.mocked(api.recipeMask).mockImplementation(async (request) => ({
    diagnostic: {
      status: "ready",
      reference: "a".repeat(64),
      model_version: "fixture",
      cache_path: "mask.png",
      width: 512,
      height: 512,
      confidence: null,
      warnings: [],
    },
    overlay_data: request.layer_id
      ? "data:image/png;base64,b3ZlcmxheQ=="
      : null,
  }));
  vi.mocked(api.renderRecipe).mockImplementation(async (r) => {
    expect(r.expected_generation).toBe(savedState.recipe_state.generation);
    savedState = {
      ...savedState,
      state: r.preview ? "preview_rendered" : "exported",
      revision: savedState.revision + 1,
      recipe_state: {
        ...savedState.recipe_state,
        generation: r.expected_generation + 1,
        current_revision:
          savedState.recipe_state.current_revision +
          (r.commit || !r.preview ? 1 : 0),
        modified:
          r.commit || !r.preview ? false : savedState.recipe_state.modified,
      },
      export_path: r.preview
        ? savedState.export_path
        : "C:/Output/photo-edited.jpg",
    };
    return {
      state: structuredClone(savedState),
      preview_data: r.preview ? "data:image/jpeg;base64,ZWRpdGVk" : null,
      width: r.preview ? 1600 : 6000,
      height: r.preview ? 1067 : 4000,
    };
  });
});
it("saves nested toolkit controls and keeps global, subject and background resets independent", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.change(screen.getByLabelText("Green saturation"), {
    target: { value: "-18" },
  });
  fireEvent.change(screen.getByLabelText("Texture"), {
    target: { value: "12" },
  });
  fireEvent.click(screen.getByText("Add subject layer"));
  fireEvent.click(screen.getByText("Add background layer"));
  fireEvent.change(screen.getByLabelText("Subject Exposure (EV)"), {
    target: { value: "0.7" },
  });
  fireEvent.change(screen.getByLabelText("Background Exposure (EV)"), {
    target: { value: "-0.4" },
  });
  await waitFor(() =>
    expect(api.saveRecipe).toHaveBeenLastCalledWith(
      "job-1",
      "photo-1",
      expect.objectContaining({
        global: expect.objectContaining({
          presence: expect.objectContaining({ texture: 12 }),
        }),
        local_layers: expect.arrayContaining([
          expect.objectContaining({
            mask_type: "subject",
            adjustments: expect.objectContaining({ exposure_ev: 0.7 }),
          }),
        ]),
      }),
      expect.any(Number),
      null,
    ),
  );
  fireEvent.click(screen.getByText("Reset Global"));
  expect(screen.getByLabelText("Texture")).toHaveValue(0);
  expect(screen.getByLabelText("Subject Exposure (EV)")).toHaveValue(0.7);
  fireEvent.click(screen.getByText("Reset Subject"));
  expect(screen.getByLabelText("Subject Exposure (EV)")).toHaveValue(0);
  expect(screen.getByLabelText("Background Exposure (EV)")).toHaveValue(-0.4);
});
it("generates masks, aligns overlays, hides stale overlays and never sends overlay state to export", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.click(screen.getByText("Add subject layer"));
  fireEvent.click(screen.getByText("Generate Subject / Background masks"));
  await screen.findByText(/Mask ready\. Update Preview/);
  expect(api.recipeMask).toHaveBeenCalledWith(
    expect.objectContaining({ generate: true }),
  );
  fireEvent.click(screen.getByText("Update Preview"));
  await screen.findByAltText("Rendered edit preview");
  fireEvent.click(screen.getByText("Show Subject Mask"));
  await screen.findByAltText("Local mask overlay");
  fireEvent.click(screen.getByText("Show original/source preview"));
  expect(screen.queryByAltText("Local mask overlay")).not.toBeInTheDocument();
  fireEvent.click(screen.getByText("Show edited preview"));
  expect(screen.getByAltText("Local mask overlay")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Subject Exposure (EV)"), {
    target: { value: "0.5" },
  });
  expect(screen.queryByAltText("Local mask overlay")).not.toBeInTheDocument();
  fireEvent.click(screen.getByText("Export full resolution"));
  await screen.findByText(/Export written/);
  const request = vi.mocked(api.renderRecipe).mock.calls.at(-1)![0];
  expect(request).not.toHaveProperty("overlay");
  expect(request).not.toHaveProperty("adjustments");
  expect(
    savedState.recipe_state!.recipe.local_layers[0].adjustments.exposure_ev,
  ).toBe(0.5);
});
it("a failed mask remains nonfatal and leaves global export available", async () => {
  const diagnostic: MaskDiagnostic = {
    status: "failed",
    reference: null,
    model_version: "fixture",
    cache_path: null,
    width: 0,
    height: 0,
    confidence: null,
    warnings: ["Portrait model failed"],
  };
  vi.mocked(api.recipeMask).mockResolvedValue({
    diagnostic,
    overlay_data: null,
  });
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.click(screen.getByText("Generate Subject / Background masks"));
  await screen.findByText("Portrait model failed");
  expect(screen.getByText("Export full resolution")).toBeEnabled();
});
it("reloads and saves typed adjustments, previews, toggles before/after and exports full resolution", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  const exposure = await screen.findByLabelText("Exposure (EV)");
  await waitFor(() => expect(exposure).toHaveValue(1.2));
  fireEvent.change(exposure, { target: { value: "2" } });
  await waitFor(() =>
    expect(api.saveRecipe).toHaveBeenCalledWith(
      "job-1",
      "photo-1",
      expect.objectContaining({
        global: expect.objectContaining({
          basic: expect.objectContaining({ exposure_ev: 2 }),
        }),
      }),
      expect.any(Number),
      null,
    ),
  );
  fireEvent.click(screen.getByText("Update Preview"));
  await screen.findByAltText("Rendered edit preview");
  expect(api.renderRecipe).toHaveBeenCalledWith(
    expect.objectContaining({
      preview: true,
      expected_generation: expect.any(Number),
      commit: true,
    }),
  );
  expect(savedState.recipe_state.recipe.global.basic.exposure_ev).toBe(2);
  expect(savedState.adjustments.exposure_ev).toBe(2);
  fireEvent.click(screen.getByText("Show original/source preview"));
  expect(
    screen.getByAltText("Original embedded/source preview"),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByText("Export full resolution"));
  await screen.findByText(/Export written · 6000 × 4000/);
  expect(api.renderRecipe).toHaveBeenLastCalledWith(
    expect.objectContaining({
      preview: false,
      jpeg_quality: 95,
      output_format: "jpeg",
    }),
  );
  fireEvent.click(screen.getByText("Reset All"));
  expect(exposure).toHaveValue(0);
});
it("reports processing failures and requests cancellation without removing the photo", async () => {
  let reject: (e: Error) => void = () => {};
  vi.mocked(api.renderRecipe).mockImplementation(
    () =>
      new Promise((_resolve, rejectPromise) => {
        reject = rejectPromise;
      }),
  );
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.click(screen.getByText("Update Preview"));
  await waitFor(() => expect(api.renderRecipe).toHaveBeenCalled());
  fireEvent.click(screen.getByText("Cancel render"));
  expect(api.cancelDevelopment).toHaveBeenCalled();
  reject(new Error("Rendering cancelled"));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Rendering cancelled",
  );
  expect(screen.getByText("Develop · photo-1.nef")).toBeInTheDocument();
});
it("keeps HEIC development unavailable while retaining the source preview", async () => {
  render(<DevelopmentPanel asset={{ ...asset(), file_type: "heic" }} />);
  await screen.findByText(/HEIC\/HEIF editing is not available/);
  expect(screen.getByText("Update Preview")).toBeDisabled();
  fireEvent.click(screen.getByText("Show original/source preview"));
  expect(
    await screen.findByAltText("Original embedded/source preview"),
  ).toBeInTheDocument();
});
it("debounces automatic preview updates and does not loop after a render failure", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  vi.mocked(api.renderRecipe).mockRejectedValue(
    new Error("Preview unavailable"),
  );
  fireEvent.click(screen.getByLabelText("Auto preview · 350 ms debounce"));
  fireEvent.change(screen.getByLabelText("Exposure (EV)"), {
    target: { value: "0.5" },
  });
  fireEvent.change(screen.getByLabelText("Exposure (EV)"), {
    target: { value: "0.8" },
  });
  expect(api.renderRecipe).not.toHaveBeenCalled();
  await screen.findByText("Preview unavailable");
  expect(api.renderRecipe).toHaveBeenCalledTimes(1);
  expect(api.renderRecipe).toHaveBeenCalledWith(
    expect.objectContaining({
      expected_generation: expect.any(Number),
      commit: false,
    }),
  );
  expect(savedState.recipe_state.recipe.global.basic.exposure_ev).toBe(0.8);
});

it("shows recipe identity, exports JSON and restores a persisted revision", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.click(screen.getByText("Recipe Inspector"));
  expect(screen.getByText("recipe-1")).toBeInTheDocument();
  fireEvent.click(screen.getByText("Save Snapshot"));
  await waitFor(() =>
    expect(api.saveRecipe).toHaveBeenLastCalledWith(
      "job-1",
      "photo-1",
      expect.any(Object),
      expect.any(Number),
      "snapshot",
    ),
  );
  await waitFor(() =>
    expect(screen.getByText("Export Recipe JSON")).toBeEnabled(),
  );
  fireEvent.click(screen.getByText("Export Recipe JSON"));
  await screen.findByText(/Recipe JSON written/);
  fireEvent.click(screen.getByText("Load Revision History"));
  await screen.findByText("Restore revision 1");
  fireEvent.click(screen.getByText("Compare revision 1"));
  await screen.findByText("Basic / Exposure (EV)");
  fireEvent.click(screen.getByText("Restore revision 1"));
  await waitFor(() =>
    expect(screen.getByLabelText("Exposure (EV)")).toHaveValue(0),
  );
  expect(api.restoreRecipe).toHaveBeenCalledWith(
    "job-1",
    "photo-1",
    "revision-1",
    expect.any(Number),
  );
  fireEvent.click(screen.getByText("Update Preview"));
  await screen.findByAltText("Rendered edit preview");
  expect(savedState.recipe_state.recipe.global.basic.exposure_ev).toBe(0);
  expect(savedState.adjustments.exposure_ev).toBe(0);
});
it("imports through the validated recipe API and makes preview stale", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.click(screen.getByText("Update Preview"));
  await screen.findByAltText("Rendered edit preview");
  fireEvent.click(screen.getByText("Import Recipe JSON"));
  await screen.findByText(/Recipe imported/);
  expect(api.importRecipe).toHaveBeenCalledWith(
    "job-1",
    "photo-1",
    "C:/Output/photo.recipe.json",
    expect.any(Number),
  );
  expect(screen.getByText(/Preview is out of date/)).toBeInTheDocument();
  fireEvent.click(screen.getByText("Update Preview"));
  await screen.findByText(/Preview ready/);
  expect(savedState.recipe_state.recipe.provenance.origin).toBe("imported");
  expect(savedState.recipe_state.recipe.global.basic.exposure_ev).toBe(1.2);
  expect(savedState.adjustments.exposure_ev).toBe(1.2);
});
it("section resets create reset checkpoints and rapid saves use successive generations", async () => {
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.change(screen.getByLabelText("Green saturation"), {
    target: { value: "-20" },
  });
  fireEvent.change(screen.getByLabelText("Exposure (EV)"), {
    target: { value: "0.7" },
  });
  fireEvent.click(screen.getByText("Reset Mixer"));
  await waitFor(() =>
    expect(api.saveRecipe).toHaveBeenLastCalledWith(
      "job-1",
      "photo-1",
      expect.any(Object),
      expect.any(Number),
      "reset",
    ),
  );
  expect(
    savedState.recipe_state!.recipe.global.color_mixer.green.saturation,
  ).toBe(0);
  expect(savedState.recipe_state!.recipe.global.basic.exposure_ev).toBe(0.7);
});
it("displays corrupt-recipe recovery without dropping the asset", async () => {
  const damaged = structuredClone(state);
  damaged.recipe_state!.error = {
    code: "corrupt_stored_recipe",
    message: "Original payload retained. Reset All to recover.",
  };
  vi.mocked(api.development).mockResolvedValue(damaged);
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Reset All")).toBeEnabled());
  expect(
    screen.getAllByText(/Original payload retained/).length,
  ).toBeGreaterThan(0);
  expect(screen.getByText("Export Recipe JSON")).toBeDisabled();
  fireEvent.click(screen.getByText("Reset All"));
  await waitFor(() =>
    expect(api.saveRecipe).toHaveBeenLastCalledWith(
      "job-1",
      "photo-1",
      expect.any(Object),
      expect.any(Number),
      "reset",
    ),
  );
});

it("builds complete, independent Phase 3 development fixtures", () => {
  const initial = recipeStateFixture({
    ...neutralAdjustments(),
    exposure_ev: 1.2,
  });
  const first = developmentStateFixture(initial);
  const second = developmentStateFixture(initial);
  expect(first.recipe_state.recipe.schema_version).toBe(1);
  expect(first.recipe_state.recipe.asset_id).toBe(asset().id);
  expect(first.recipe_state.recipe.global.basic.exposure_ev).toBe(1.2);
  expect(first.recipe_state.recipe_hash).toMatch(/^[a-f0-9]{64}$/);
  expect(first.adjustments).toEqual(recipeControls(first.recipe_state.recipe));
  expect(first.diagnostics.mask.status).toBe("unavailable");
  expect(first.unresolved_masks).toEqual([]);

  first.recipe_state.recipe.global.color_mixer.green.saturation = -18;
  first.recipe_state.recipe.local_layers.push({
    id: "subject",
    mask_type: "subject",
    enabled: true,
    strength: 1,
    invert: false,
    confidence: null,
    mask_reference: null,
    adjustments: neutralLocal(),
  });
  expect(second).toEqual(developmentStateFixture(initial));
  expect(initial.recipe.local_layers).toEqual([]);
  expect(initial.recipe.global.color_mixer.green.saturation).toBe(0);
});

it("loads nested recipe settings and reloads saved local edits without changing other layers", async () => {
  const persistedRecipe = recipeStateFixture(state.adjustments);
  persistedRecipe.recipe.global.color_mixer.green.saturation = -18;
  persistedRecipe.recipe.global.presence.texture = 12;
  persistedRecipe.recipe.global.detail.sharpening.masking = 45;
  persistedRecipe.recipe.global.geometry.rotation_degrees = 1.5;
  persistedRecipe.recipe.local_layers = [
    {
      id: "subject",
      mask_type: "subject",
      enabled: true,
      strength: 1,
      invert: false,
      confidence: null,
      mask_reference: null,
      adjustments: { ...neutralLocal(), exposure_ev: 0.7 },
    },
    {
      id: "background",
      mask_type: "background",
      enabled: true,
      strength: 1,
      invert: false,
      confidence: null,
      mask_reference: null,
      adjustments: { ...neutralLocal(), exposure_ev: -0.4 },
    },
  ];
  savedState = developmentStateFixture(persistedRecipe);
  const panel = render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  expect(screen.getByLabelText("Exposure (EV)")).toHaveValue(1.2);
  expect(screen.getByLabelText("Green saturation")).toHaveValue(-18);
  expect(screen.getByLabelText("Texture")).toHaveValue(12);
  expect(screen.getByLabelText("Sharpening masking")).toHaveValue(45);
  expect(screen.getByLabelText("Rotation (degrees)")).toHaveValue(1.5);
  expect(screen.getByLabelText("Subject Exposure (EV)")).toHaveValue(0.7);
  expect(screen.getByLabelText("Background Exposure (EV)")).toHaveValue(-0.4);
  expect(
    screen.queryByText("Loading saved adjustments…"),
  ).not.toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Subject Exposure (EV)"), {
    target: { value: "0.9" },
  });
  await waitFor(() =>
    expect(
      savedState.recipe_state.recipe.local_layers[0].adjustments.exposure_ev,
    ).toBe(0.9),
  );
  panel.unmount();
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  expect(screen.getByLabelText("Subject Exposure (EV)")).toHaveValue(0.9);
  expect(screen.getByLabelText("Background Exposure (EV)")).toHaveValue(-0.4);
  expect(screen.getByLabelText("Exposure (EV)")).toHaveValue(1.2);
  expect(screen.getByLabelText("Green saturation")).toHaveValue(-18);
  expect(api.development).toHaveBeenCalledTimes(2);
});

it("rejects a missing recipe response instead of silently accepting legacy adjustments", async () => {
  // Deliberately invalid response: verify the production contract guard, not compatibility.
  vi.mocked(api.development).mockResolvedValue({
    ...state,
    recipe_state: null,
  });
  render(<DevelopmentPanel asset={asset()} />);
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "This desktop version did not return an edit recipe.",
  );
  expect(screen.getByText("Update Preview")).toBeDisabled();
  expect(screen.getByLabelText("Exposure (EV)")).toBeDisabled();
  expect(api.saveRecipe).not.toHaveBeenCalled();
  expect(api.renderRecipe).not.toHaveBeenCalled();
});
