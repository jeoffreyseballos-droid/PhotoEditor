import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../api";
import {
  AnalysisContent,
  AnalysisInspector,
} from "../components/AnalysisInspector";
import { asset } from "./fixtures";
import fixture from "./analysis-fixture.json";
import type { AnalysisState, PhotoAnalysis } from "../analysis";

vi.mock("../api", () => ({
  api: {
    getAnalysis: vi.fn(),
    analyzeAsset: vi.fn(),
    cancelAnalysis: vi.fn(),
    invalidateAnalysis: vi.fn(),
    exportAnalysis: vi.fn(),
  },
  errorMessage: (e: Error) => e.message,
}));
const none: AnalysisState = {
  status: "not_analyzed",
  analysis: null,
  cached: false,
  error: null,
};
// Produced by the real Rust synthetic pipeline, and validated by Rust on every test run.
const analyzed = (): AnalysisState => ({
  status: "warning",
  analysis: structuredClone(fixture) as PhotoAnalysis,
  cached: false,
  error: null,
});
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getAnalysis).mockResolvedValue(none);
  vi.mocked(api.analyzeAsset).mockResolvedValue(analyzed());
  vi.mocked(api.cancelAnalysis).mockResolvedValue();
  vi.mocked(api.invalidateAnalysis).mockResolvedValue();
  vi.mocked(api.exportAnalysis).mockResolvedValue("output/photo.analysis.json");
});
it("loads lazily and never analyzes automatically", async () => {
  render(<AnalysisInspector asset={asset()} />);
  expect(api.getAnalysis).not.toHaveBeenCalled();
  expect(api.analyzeAsset).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Photo analysis · source measurements"));
  await waitFor(() =>
    expect(api.getAnalysis).toHaveBeenCalledWith(
      "job-1",
      "photo-1",
      "portrait",
    ),
  );
  expect(api.analyzeAsset).not.toHaveBeenCalled();
});
it("shows measurements, unavailable faces, JSON and optional source geometry without edit requests", async () => {
  render(<AnalysisContent asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Analyze source")).toBeEnabled());
  fireEvent.click(screen.getByText("Analyze source"));
  await screen.findByText("Analysis saved. Source and recipe unchanged.");
  expect(api.analyzeAsset).toHaveBeenCalledWith(
    expect.objectContaining({
      job_id: "job-1",
      asset_id: "photo-1",
      photo_type: "portrait",
    }),
  );
  const sent = vi.mocked(api.analyzeAsset).mock.calls[0][0];
  expect(Object.keys(sent).sort()).toEqual([
    "asset_id",
    "job_id",
    "photo_type",
    "request_id",
  ]);
  expect(screen.getByText("0.2588 / 0.0613")).toBeInTheDocument();
  expect(
    screen.getByText(/unavailable: Face detector not installed/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("img", { name: "Source geometry debug diagram" }),
  ).not.toBeInTheDocument();
  fireEvent.click(
    screen.getByLabelText("Show source-coordinate geometry diagram"),
  );
  expect(
    screen.getByRole("img", { name: "Source geometry debug diagram" }),
  ).toBeInTheDocument();
  expect(screen.getByText("View PhotoAnalysis JSON")).toBeInTheDocument();
  fireEvent.click(screen.getByText("Export analysis JSON"));
  await screen.findByText("Analysis JSON saved: output/photo.analysis.json");
  expect(api.exportAnalysis).toHaveBeenCalledWith(
    "job-1",
    "photo-1",
    "portrait",
  );
});
it("selects all photo types and reuses saved analysis", async () => {
  const realEstate = analyzed();
  const a = realEstate.analysis!;
  a.photo_type = "real_estate";
  const skipped = {
    status: "not_applicable",
    reason: "Portrait alpha skipped for this photo type",
  } as const;
  a.subjects = {
    subject_present: skipped,
    measurements: skipped,
    subject_count: skipped,
    faces: skipped,
  };
  const unavailable = {
    status: "unavailable",
    reason: "Subject unavailable",
  } as const;
  a.lighting = {
    ...a.lighting,
    subject_light_level: unavailable,
    background_light_level: unavailable,
    subject_background_ev_difference: unavailable,
    backlighting_tendency: unavailable,
  };
  a.type_specific = {
    photo_type: "real_estate",
    measurements: {
      interior_exterior: {
        status: "unavailable",
        reason: "No semantic interior/exterior model configured",
      },
      bright_region_fraction: a.common.exposure.near_highlight_clip_fraction,
      shadow_depth: a.common.exposure.percentiles.p05,
      mixed_lighting: a.lighting.mixed_lighting_tendency,
      estimated_roll: a.common.composition.horizontal_line,
    },
  };
  a.diagnostics.providers = [];
  a.diagnostics.analyzers[1].status = "not_applicable";
  vi.mocked(api.analyzeAsset).mockResolvedValue({
    ...realEstate,
    cached: true,
  });
  render(<AnalysisContent asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Analyze source")).toBeEnabled());
  for (const type of ["landscape", "real_estate"]) {
    fireEvent.change(screen.getByLabelText("Analysis photo type"), {
      target: { value: type },
    });
    await waitFor(() =>
      expect(api.getAnalysis).toHaveBeenLastCalledWith(
        "job-1",
        "photo-1",
        type,
      ),
    );
    await waitFor(() =>
      expect(screen.getByText("Analyze source")).toBeEnabled(),
    );
  }
  fireEvent.click(screen.getByText("Analyze source"));
  await screen.findByText("Reused saved source analysis.");
  expect(api.analyzeAsset).toHaveBeenCalledWith(
    expect.objectContaining({ photo_type: "real_estate" }),
  );
});
it("invalidates disposable analyses and allows rerun", async () => {
  vi.mocked(api.getAnalysis).mockResolvedValue(analyzed());
  render(<AnalysisContent asset={asset()} />);
  await waitFor(() =>
    expect(screen.getByText("Invalidate analysis")).toBeEnabled(),
  );
  fireEvent.click(screen.getByText("Invalidate analysis"));
  await screen.findByText(/Discarded disposable analysis/);
  expect(api.invalidateAnalysis).toHaveBeenCalledWith("job-1", "photo-1");
  expect(screen.getByText("Export analysis JSON")).toBeDisabled();
  expect(screen.getByText("Analyze source")).toBeEnabled();
});
it("cancels long work and can retry a failed run", async () => {
  let reject!: (e: Error) => void;
  vi.mocked(api.analyzeAsset).mockImplementationOnce(
    () =>
      new Promise((_, r) => {
        reject = r;
      }),
  );
  render(<AnalysisContent asset={asset()} />);
  await waitFor(() => expect(screen.getByText("Analyze source")).toBeEnabled());
  fireEvent.click(screen.getByText("Analyze source"));
  expect(screen.getByLabelText("Analysis photo type")).toBeDisabled();
  fireEvent.click(screen.getByText("Cancel analysis"));
  expect(api.cancelAnalysis).toHaveBeenCalledWith(
    vi.mocked(api.analyzeAsset).mock.calls[0][0].request_id,
  );
  await act(async () => {
    reject(new Error("Analysis cancelled"));
  });
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Analysis cancelled",
  );
  expect(screen.getByText("Analyze source")).toBeEnabled();
  fireEvent.click(screen.getByText("Analyze source"));
  await screen.findByText("Analysis saved. Source and recipe unchanged.");
});
it("cancels on close and ignores late load results", async () => {
  let resolve!: (s: AnalysisState) => void;
  vi.mocked(api.getAnalysis).mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const view = render(<AnalysisContent asset={asset()} />);
  fireEvent.change(screen.getByLabelText("Analysis photo type"), {
    target: { value: "landscape" },
  });
  await waitFor(() =>
    expect(api.getAnalysis).toHaveBeenLastCalledWith(
      "job-1",
      "photo-1",
      "landscape",
    ),
  );
  await act(async () => resolve(analyzed()));
  expect(screen.queryByText("View PhotoAnalysis JSON")).not.toBeInTheDocument();
  vi.mocked(api.analyzeAsset).mockImplementation(() => new Promise(() => {}));
  fireEvent.click(screen.getByText("Analyze source"));
  view.unmount();
  expect(api.cancelAnalysis).toHaveBeenCalledWith(
    vi.mocked(api.analyzeAsset).mock.calls[0][0].request_id,
  );
});
