import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { DevelopmentPanel } from "../components/DevelopmentPanel";
import { api } from "../api";
import { asset } from "./fixtures";
import type { DevelopmentState } from "../types";
import { neutralAdjustments } from "../toolkit";
import type { MaskDiagnostic } from "../toolkit";
vi.mock("../api", () => ({
  errorMessage: (e: { message: string }) => e.message,
  api: {
    development: vi.fn(),
    saveDevelopment: vi.fn(),
    thumbnail: vi.fn(),
    renderDevelopment: vi.fn(),
    cancelDevelopment: vi.fn(),
    developmentMask: vi.fn(),
  },
}));
const state: DevelopmentState = {
  adjustments: {
    ...neutralAdjustments(),
    exposure_ev: 1.2,
    temperature: 6500,
    tint: 0,
    contrast: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    saturation: 0,
    vibrance: 0,
    rotation_degrees: 0,
    crop: { x: 0, y: 0, width: 1, height: 1 },
    sharpening: 0,
    noise_reduction: 0,
  },
  revision: 2,
  state: "source_ready",
  source_identity: null,
  preview_path: null,
  export_path: null,
  error: null,
  warnings: [],
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.development).mockResolvedValue(structuredClone(state));
  vi.mocked(api.saveDevelopment).mockImplementation(
    async (_job, _asset, adjustments) => ({ ...state, adjustments }),
  );
  vi.mocked(api.thumbnail).mockResolvedValue(
    "data:image/jpeg;base64,b3JpZ2luYWw=",
  );
  vi.mocked(api.cancelDevelopment).mockResolvedValue();
  vi.mocked(api.developmentMask).mockImplementation(async (request) => ({
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
  vi.mocked(api.renderDevelopment).mockImplementation(async (r) => ({
    state: {
      ...state,
      adjustments: r.adjustments,
      export_path: r.preview ? null : "C:/Output/photo-edited.jpg",
    },
    preview_data: r.preview ? "data:image/jpeg;base64,ZWRpdGVk" : null,
    width: r.preview ? 1600 : 6000,
    height: r.preview ? 1067 : 4000,
  }));
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
    expect(api.saveDevelopment).toHaveBeenLastCalledWith(
      "job-1",
      "photo-1",
      expect.objectContaining({
        presence: expect.objectContaining({ texture: 12 }),
        local_layers: expect.arrayContaining([
          expect.objectContaining({
            mask_type: "subject",
            adjustments: expect.objectContaining({ exposure_ev: 0.7 }),
          }),
        ]),
      }),
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
  expect(api.developmentMask).toHaveBeenCalledWith(
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
  const request = vi.mocked(api.renderDevelopment).mock.calls.at(-1)![0];
  expect(request).not.toHaveProperty("overlay");
  expect(request.adjustments).not.toHaveProperty("overlay");
  expect(request.adjustments.local_layers[0].adjustments.exposure_ev).toBe(0.5);
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
  vi.mocked(api.developmentMask).mockResolvedValue({
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
    expect(api.saveDevelopment).toHaveBeenCalledWith(
      "job-1",
      "photo-1",
      expect.objectContaining({ exposure_ev: 2 }),
    ),
  );
  fireEvent.click(screen.getByText("Update Preview"));
  await screen.findByAltText("Rendered edit preview");
  expect(api.renderDevelopment).toHaveBeenCalledWith(
    expect.objectContaining({
      preview: true,
      adjustments: expect.objectContaining({ exposure_ev: 2 }),
    }),
  );
  fireEvent.click(screen.getByText("Show original/source preview"));
  expect(
    screen.getByAltText("Original embedded/source preview"),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByText("Export full resolution"));
  await screen.findByText(/Export written · 6000 × 4000/);
  expect(api.renderDevelopment).toHaveBeenLastCalledWith(
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
  vi.mocked(api.renderDevelopment).mockImplementation(
    () =>
      new Promise((_resolve, rejectPromise) => {
        reject = rejectPromise;
      }),
  );
  render(<DevelopmentPanel asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Update Preview")).toBeEnabled());
  fireEvent.click(screen.getByText("Update Preview"));
  await waitFor(() => expect(api.renderDevelopment).toHaveBeenCalled());
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
  vi.mocked(api.renderDevelopment).mockRejectedValue(
    new Error("Preview unavailable"),
  );
  fireEvent.click(screen.getByLabelText("Auto preview · 350 ms debounce"));
  fireEvent.change(screen.getByLabelText("Exposure (EV)"), {
    target: { value: "0.5" },
  });
  fireEvent.change(screen.getByLabelText("Exposure (EV)"), {
    target: { value: "0.8" },
  });
  expect(api.renderDevelopment).not.toHaveBeenCalled();
  await screen.findByText("Preview unavailable");
  expect(api.renderDevelopment).toHaveBeenCalledTimes(1);
  expect(api.renderDevelopment).toHaveBeenCalledWith(
    expect.objectContaining({
      adjustments: expect.objectContaining({ exposure_ev: 0.8 }),
    }),
  );
});
