import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../api";
import { BatchContextInspector } from "../components/BatchContextInspector";
import { asset, job } from "./fixtures";
import {
  batchContextFixture,
  batchContextStateFixture,
} from "./batch-context-fixture";

vi.mock("../api", () => ({
  api: {
    batchContextState: vi.fn(),
    runBatchContext: vi.fn(),
    batchContextProgress: vi.fn(),
    cancelBatchContext: vi.fn(),
  },
  errorMessage: (error: Error) => error.message,
}));

const assets = [asset("photo-1"), asset("photo-2")];

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.batchContextState).mockResolvedValue(
    batchContextStateFixture(),
  );
  vi.mocked(api.batchContextProgress).mockResolvedValue(null);
  vi.mocked(api.cancelBatchContext).mockResolvedValue();
  vi.mocked(api.runBatchContext).mockResolvedValue(batchContextStateFixture());
});

function open(selectedAssetId = "photo-1", onSelectAsset = vi.fn()) {
  render(
    <BatchContextInspector
      jobId={job.id}
      photoType="portrait"
      assets={assets}
      selectedAssetId={selectedAssetId}
      onSelectAsset={onSelectAsset}
    />,
  );
  return onSelectAsset;
}

it("shows cached group, reference, relative exposure and color context", async () => {
  const select = open();
  expect(await screen.findByText("Selected: 2")).toBeInTheDocument();
  expect(screen.getByText("Scene groups: 1")).toBeInTheDocument();
  expect(screen.getByText("Lighting groups: 1")).toBeInTheDocument();
  expect(screen.getByText("References: 2")).toBeInTheDocument();
  expect(screen.getAllByText("photo-1.nef").length).toBeGreaterThan(0);
  expect(screen.getByText("Reference", { selector: "dd" })).toBeInTheDocument();
  expect(screen.getByText("Near group median")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "photo-2.nef" }));
  expect(select).toHaveBeenCalledWith(assets[1]);
});

it("reports a changed selection and builds its exact current context", async () => {
  const stale = batchContextStateFixture(null, true);
  vi.mocked(api.batchContextState).mockResolvedValue(stale);
  let finish!: (value: ReturnType<typeof batchContextStateFixture>) => void;
  vi.mocked(api.runBatchContext).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  open();
  expect(
    await screen.findByText(/editing selection or source evidence changed/i),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Build Context" }));
  expect(
    screen.getByText(/Building batch context… 0 \/ 2/),
  ).toBeInTheDocument();
  expect(api.runBatchContext).toHaveBeenCalledWith(
    expect.objectContaining({
      job_id: job.id,
      photo_type: "portrait",
      force: false,
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Cancel Context" }));
  await waitFor(() => expect(api.cancelBatchContext).toHaveBeenCalled());
  finish(batchContextStateFixture(batchContextFixture()));
  expect(await screen.findByText("Selected: 2")).toBeInTheDocument();
});

it("keeps unavailable source analysis nonfatal", async () => {
  const context = batchContextFixture();
  context.asset_contexts[1] = {
    asset_id: "photo-2",
    availability: "unavailable",
    scene_group_id: null,
    lighting_group_id: null,
    sequence_group_id: null,
    reference_asset_id: null,
    exposure_delta_from_group: null,
    wb_delta_from_group: null,
    group_confidence: 0,
    consistency_notes: [
      { code: "analysis_unavailable", message: "Source unavailable" },
    ],
  };
  context.diagnostics.available_assets = 1;
  context.diagnostics.unavailable_assets = 1;
  vi.mocked(api.batchContextState).mockResolvedValue(
    batchContextStateFixture(context),
  );
  open("photo-2");
  expect(await screen.findAllByText("Unavailable")).toHaveLength(4);
  expect(screen.getByText("No reliable reference")).toBeInTheDocument();
});
