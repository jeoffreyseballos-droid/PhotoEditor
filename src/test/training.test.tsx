import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../api";
import { TrainingStudioScreen } from "../screens/TrainingStudioScreen";
import {
  trainingDatasetFixture,
  trainingPairFixture,
  trainingRunFixture,
} from "./training-fixture";
import type { TrainingDataset, MatchingProgress } from "../training";

vi.mock("../api", () => ({
  api: {
    trainingDatasets: vi.fn(),
    createTrainingDataset: vi.fn(),
    trainingDataset: vi.fn(),
    addTrainingBeforeFiles: vi.fn(),
    addTrainingAfterFiles: vi.fn(),
    addTrainingBeforeFolder: vi.fn(),
    addTrainingAfterFolder: vi.fn(),
    addTrainingPathPair: vi.fn(),
    matchTrainingDataset: vi.fn(),
    matchValidateTrainingDataset: vi.fn(),
    trainingMatchingProgress: vi.fn(),
    cancelTrainingMatching: vi.fn(),
    setTrainingPairExcluded: vi.fn(),
    validateTrainingDataset: vi.fn(),
    runTraining: vi.fn(),
    trainingProgress: vi.fn(),
    cancelTraining: vi.fn(),
    trainingPairPreviews: vi.fn(),
    trainingFeedback: vi.fn(),
    chooseTrainingFiles: vi.fn(),
    chooseFolder: vi.fn(),
  },
  errorMessage: (error: unknown) =>
    error instanceof Error ? error.message : "Training failed",
}));

beforeEach(() => {
  vi.resetAllMocks();
  const dataset = trainingDatasetFixture();
  vi.mocked(api.trainingDatasets).mockResolvedValue([dataset]);
  vi.mocked(api.trainingDataset).mockResolvedValue(dataset);
  vi.mocked(api.trainingProgress).mockResolvedValue(null);
  vi.mocked(api.cancelTraining).mockResolvedValue();
  vi.mocked(api.trainingMatchingProgress).mockResolvedValue(null);
  vi.mocked(api.cancelTrainingMatching).mockResolvedValue();
});

it("opens without a Job and uses only explicit before/after inputs", async () => {
  render(<TrainingStudioScreen onClose={vi.fn()} onViewPresets={vi.fn()} />);
  expect(
    await screen.findByRole("heading", { name: "Jeoffrey Portrait" }),
  ).toBeInTheDocument();
  expect(screen.queryByLabelText("Source photo")).not.toBeInTheDocument();
  expect(screen.queryByText("Add Pair")).not.toBeInTheDocument();
  expect(
    screen.getByRole("heading", { name: "Before 1 images" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("heading", { name: "After 1 images" }),
  ).toBeInTheDocument();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function fortySeven() {
  const pairs = Array.from({ length: 47 }, (_, i) =>
    trainingPairFixture({
      pair_id: `pair-${i}`,
      source_path: `C:\\before\\2H1A${3375 + i}.CR3`,
      reference_path: `C:\\after\\Sheila (${i + 1} of 47).jpg`,
    }),
  );
  return trainingDatasetFixture({
    pairs,
    before_files: pairs.map((p) => p.source_path),
    after_files: pairs.map((p) => p.reference_path),
    alignment: {
      before_count: 47,
      after_count: 47,
      matched_count: 47,
      ambiguous_count: 0,
      unmatched_before: [],
      unmatched_after: [],
      first_before: pairs[0].source_path,
      first_after: pairs[0].reference_path,
      last_before: pairs[46].source_path,
      last_after: pairs[46].reference_path,
      start_aligned: true,
      end_aligned: true,
      order_fallback_used: true,
      diagnostics: [],
    },
  });
}

it("shows immediate real stage progress, locks controls, and completes 47 pairs", async () => {
  const result = deferred<TrainingDataset>();
  const dataset = fortySeven();
  vi.mocked(api.trainingDatasets).mockResolvedValue([
    { ...dataset, alignment: null, pairs: [] },
  ]);
  vi.mocked(api.matchValidateTrainingDataset).mockReturnValue(result.promise);
  let latest: MatchingProgress | null = null;
  vi.mocked(api.trainingMatchingProgress).mockImplementation(
    async () => latest,
  );
  render(<TrainingStudioScreen onClose={vi.fn()} onViewPresets={vi.fn()} />);
  await screen.findByRole("heading", { name: "Before 47 images" });
  fireEvent.click(screen.getByText("Match / Validate Dataset"));
  expect(
    screen.getByRole("progressbar", { name: "Dataset matching progress" }),
  ).toHaveValue(0);
  for (const name of [
    "Add Before Image",
    "Add Before Folder",
    "Add After Image",
    "Add After Folder",
    "Train Style",
    "Matching dataset…",
  ])
    expect(screen.getByRole("button", { name })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Matching dataset…" }));
  expect(api.matchValidateTrainingDataset).toHaveBeenCalledTimes(1);
  const requestId = vi.mocked(api.matchValidateTrainingDataset).mock
    .calls[0][1];
  latest = {
    request_id: requestId,
    dataset_id: dataset.dataset_id,
    status: "running",
    stage: "building_pair_candidates",
    processed: 18,
    total: 47,
    error: null,
  };
  await screen.findByText("Building Pair Candidates");
  expect(screen.getByRole("progressbar")).toHaveValue(18);
  latest = { ...latest, stage: "structural_validation", processed: 32 };
  await screen.findByText("Structural Validation");
  expect(screen.getByRole("progressbar")).toHaveValue(32);
  latest = { ...latest, processed: 47 };
  await screen.findByText(/47 \/ 47/);
  await act(async () => result.resolve(dataset));
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  expect(screen.getByText("Dataset Ready")).toBeInTheDocument();
  expect(
    screen
      .getAllByRole("button", { name: "Train Style" })
      .every((button) => !button.hasAttribute("disabled")),
  ).toBe(true);
  expect(screen.queryByText("Review pair")).not.toBeInTheDocument();
});

it("failure releases controls and supports retry", async () => {
  vi.mocked(api.matchValidateTrainingDataset).mockRejectedValue(
    new Error("Folder unavailable"),
  );
  render(<TrainingStudioScreen onClose={vi.fn()} onViewPresets={vi.fn()} />);
  fireEvent.click(await screen.findByText("Match / Validate Dataset"));
  await screen.findByText("Dataset matching failed");
  expect(screen.getByText("Folder unavailable")).toBeInTheDocument();
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  expect(screen.getByText("Add Before Folder")).toBeEnabled();
  vi.mocked(api.matchValidateTrainingDataset).mockResolvedValue(fortySeven());
  fireEvent.click(screen.getByText("Try Again"));
  await screen.findByText("Dataset Ready");
});

it("cancellation awaits the worker and preserves the previous dataset", async () => {
  const result = deferred<TrainingDataset>();
  vi.mocked(api.matchValidateTrainingDataset).mockReturnValue(result.promise);
  render(<TrainingStudioScreen onClose={vi.fn()} onViewPresets={vi.fn()} />);
  fireEvent.click(await screen.findByText("Match / Validate Dataset"));
  fireEvent.click(screen.getByText("Cancel matching"));
  expect(api.cancelTrainingMatching).toHaveBeenCalled();
  expect(screen.getByText("Add Before Folder")).toBeDisabled();
  await act(async () => result.reject(new Error("Cancelled")));
  expect(
    screen.getByText("Matching cancelled. Previous dataset preserved."),
  ).toBeInTheDocument();
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  expect(
    screen.getByRole("heading", { name: "Before 1 images" }),
  ).toBeInTheDocument();
  expect(screen.getByText("Train Style")).toBeEnabled();
});

it("keeps folder matching and alignment review separate from training", async () => {
  const dataset = trainingDatasetFixture({
    before_files: [],
    after_files: [],
    pairs: [],
  });
  vi.mocked(api.trainingDatasets).mockResolvedValue([dataset]);
  const matched = trainingDatasetFixture({
    before_files: ["C:\\before\\IMG_1001.CR3", "C:\\before\\IMG_1002.CR3"],
    after_files: ["C:\\after\\IMG_1001_EDIT.JPG", "C:\\after\\IMG_1002.JPG"],
    alignment: {
      before_count: 2,
      after_count: 2,
      matched_count: 2,
      ambiguous_count: 0,
      unmatched_before: [],
      unmatched_after: [],
      first_before: "C:\\before\\IMG_1001.CR3",
      first_after: "C:\\after\\IMG_1001_EDIT.JPG",
      last_before: "C:\\before\\IMG_1002.CR3",
      last_after: "C:\\after\\IMG_1002.JPG",
      start_aligned: true,
      end_aligned: true,
      order_fallback_used: false,
      diagnostics: [],
    },
  });
  vi.mocked(api.chooseFolder)
    .mockResolvedValueOnce("C:\\before")
    .mockResolvedValueOnce("C:\\after");
  vi.mocked(api.addTrainingBeforeFolder).mockResolvedValue({
    ...dataset,
    before_files: matched.before_files,
  });
  vi.mocked(api.addTrainingAfterFolder).mockResolvedValue({
    ...dataset,
    before_files: matched.before_files,
    after_files: matched.after_files,
  });
  vi.mocked(api.matchTrainingDataset).mockResolvedValue({
    dataset: matched,
    matching: {
      matched: [],
      ambiguous_sources: [],
      unmatched_references: [],
      unmatched_sources: [],
      before_count: 2,
      after_count: 2,
      start_aligned: true,
      end_aligned: true,
      order_fallback_used: false,
      diagnostics: [],
    },
  });
  vi.mocked(api.validateTrainingDataset).mockResolvedValue(matched);
  vi.mocked(api.matchValidateTrainingDataset).mockResolvedValue(matched);

  render(<TrainingStudioScreen onClose={vi.fn()} onViewPresets={vi.fn()} />);
  await screen.findByRole("heading", { name: "Jeoffrey Portrait" });
  fireEvent.click(screen.getByText("Add Before Folder"));
  await screen.findByRole("heading", { name: "Before 2 images" });
  fireEvent.click(screen.getByText("Add After Folder"));
  await screen.findByRole("heading", { name: "After 2 images" });
  fireEvent.click(screen.getByText("Match / Validate Dataset"));
  await waitFor(() =>
    expect(api.matchValidateTrainingDataset).toHaveBeenCalledWith(
      "dataset-1",
      expect.any(String),
    ),
  );
  expect(await screen.findByText("2 matched")).toBeInTheDocument();
  expect(screen.getAllByText(/aligned/)).toHaveLength(2);
});

it("trains an independent dataset and exposes the saved trained preset", async () => {
  const completed = trainingRunFixture();
  const viewPresets = vi.fn();
  vi.mocked(api.runTraining).mockResolvedValue(completed);
  vi.mocked(api.trainingDataset).mockResolvedValue(trainingDatasetFixture());

  render(
    <TrainingStudioScreen onClose={vi.fn()} onViewPresets={viewPresets} />,
  );
  fireEvent.click(await screen.findByText("Train Style"));
  expect(await screen.findByText("Training Complete")).toBeInTheDocument();
  expect(
    screen.getByText("Preset saved successfully and is available in Presets."),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByText("View Preset"));
  expect(viewPresets).toHaveBeenCalled();
  expect(api.runTraining).toHaveBeenCalledWith(
    "dataset-1",
    expect.any(String),
    expect.any(Object),
  );
});

it("keeps pair review, exclusion, feedback, and cancellation available", async () => {
  const dataset = trainingDatasetFixture();
  vi.mocked(api.trainingPairPreviews).mockResolvedValue({
    source_data: "data:image/jpeg;base64,c291cmNl",
    ai_data: null,
    target_data: "data:image/jpeg;base64,dGFyZ2V0",
    reference_data: "data:image/jpeg;base64,cmVmZXJlbmNl",
  });
  vi.mocked(api.setTrainingPairExcluded).mockResolvedValue(
    trainingDatasetFixture({
      pairs: [{ ...dataset.pairs[0], excluded: true, split: "excluded" }],
    }),
  );
  vi.mocked(api.trainingFeedback).mockResolvedValue(
    trainingDatasetFixture({
      pairs: [{ ...dataset.pairs[0], feedback: "accept" }],
    }),
  );
  vi.mocked(api.runTraining).mockImplementation(() => new Promise(() => {}));

  render(<TrainingStudioScreen onClose={vi.fn()} onViewPresets={vi.fn()} />);
  await screen.findByRole("heading", { name: "Jeoffrey Portrait" });
  fireEvent.click(screen.getByText("Review all matches"));
  fireEvent.click(screen.getByText("Review pair"));
  expect(await screen.findByAltText("Unedited source")).toBeVisible();
  fireEvent.click(screen.getByText("Close review"));
  fireEvent.click(screen.getByText("Exclude pair"));
  await waitFor(() => expect(api.setTrainingPairExcluded).toHaveBeenCalled());
  vi.mocked(api.setTrainingPairExcluded).mockResolvedValue(dataset);
  fireEvent.click(await screen.findByText("Include pair"));
  await screen.findByText("Exclude pair");
  fireEvent.click(screen.getAllByText("Accept")[0]);
  await waitFor(() =>
    expect(api.trainingFeedback).toHaveBeenCalledWith(
      "dataset-1",
      "pair-1",
      "accept",
    ),
  );
  fireEvent.click(screen.getByText("Train Style"));
  fireEvent.click(await screen.findByText("Cancel training"));
  await waitFor(() => expect(api.cancelTraining).toHaveBeenCalled());
});
