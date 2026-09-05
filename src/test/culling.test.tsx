import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../api";
import { CullingScreen } from "../screens/CullingScreen";
import {
  exactSelectionEligible,
  filterItems,
  matchesRelationship,
  type SimilarityContext,
  type CullingAssessment,
  type CullingItem,
  type CullingOverview,
  type CullingProgress,
  type Stars,
} from "../culling";
import { asset, job } from "./fixtures";
import fixture from "./culling-fixture.json";
vi.mock("../api", () => ({
  api: {
    cullingOverview: vi.fn(),
    cullingDetail: vi.fn(),
    cullingProgress: vi.fn(),
    runCulling: vi.fn(),
    cancelCulling: vi.fn(),
    cullingRating: vi.fn(),
    cullingSelectAsset: vi.fn(),
    cullingSelectAssets: vi.fn(),
    cullingSelectRatings: vi.fn(),
    thumbnail: vi.fn(),
  },
  errorMessage: (e: Error) => e.message,
}));
let saved: CullingItem[];
let issueAvailability: CullingOverview["issue_availability"];
const neutralRelationship = (): SimilarityContext =>
  structuredClone(fixture.similarity) as SimilarityContext;
function visualRelationship(
  id: string,
  preferred: string[],
  size = 2,
  kind: SimilarityContext["kind"] = "near_duplicate",
): SimilarityContext {
  return {
    ...neutralRelationship(),
    group_id: "1".repeat(64),
    group_size: size,
    preferred: preferred.includes(id),
    preferred_assets: preferred,
    relative_score: preferred.includes(id) ? 0 : 5,
    confidence: 0.7,
    similarity_score: 0.98,
    kind,
  };
}
function overview(): CullingOverview {
  const counts = [0, 0, 0, 0, 0, 0];
  for (const i of saved) counts[i.effective_rating ?? 0]++;
  return {
    items: structuredClone(saved),
    counts,
    selected_count: saved.filter((i) => i.selected_for_editing).length,
    progress: null,
    issue_availability: issueAvailability,
    duplicates: {
      exact_copies: saved.filter(
        (i) =>
          i.similarity?.exact &&
          i.similarity.exact.canonical_asset_id !== i.asset.id,
      ).length,
      exact_groups: new Set(
        saved.flatMap((i) =>
          i.similarity?.exact ? [i.similarity.exact.group_id] : [],
        ),
      ).size,
      near_groups: new Set(
        saved
          .filter((i) => i.similarity?.kind === "near_duplicate")
          .map((i) => i.group_id),
      ).size,
      burst_groups: new Set(
        saved
          .filter((i) => i.similarity?.kind === "burst")
          .map((i) => i.group_id),
      ).size,
      similar_groups: new Set(
        saved
          .filter((i) => i.similarity?.kind === "similar")
          .map((i) => i.group_id),
      ).size,
      unique_images: saved.filter((i) => i.relationship_kind === "unique")
        .length,
      unclassified_images: saved.filter((i) => i.relationship_kind === null)
        .length,
    },
  };
}
const progress = (status = "complete"): CullingProgress => ({
  job_id: job.id,
  request_id: "run-1",
  photo_type: "portrait",
  status,
  stage: "Complete",
  completed: 6,
  total: 6,
  failed: 0,
  cached: 0,
  duration_ms: 20,
  error: null,
  hash_bytes: 12345,
  hash_cached: 0,
  hash_duration_ms: 2,
  hash_failures: 0,
});
beforeEach(() => {
  vi.resetAllMocks();
  issueAvailability = { blurry: true, closed_eyes: false };
  saved = ([5, 4, 3, 2, 1, null] as const).map((r, i) => ({
    asset: asset(`photo-${i + 1}`),
    ai_rating: r,
    user_rating: null,
    effective_rating: r,
    selected_for_editing: false,
    stale: false,
    group_id: i < 2 ? "1".repeat(64) : null,
    preferred: i === 0,
    review_count: i === 1 ? 1 : 0,
    relationship_kind: i < 2 ? "near_duplicate" : i === 5 ? null : "unique",
    similarity:
      i < 2
        ? visualRelationship(`photo-${i + 1}`, ["photo-1"])
        : i === 5
          ? null
          : neutralRelationship(),
    issues: [],
  }));
  vi.mocked(api.cullingOverview).mockImplementation(async () => overview());
  vi.mocked(api.cullingProgress).mockResolvedValue(null);
  vi.mocked(api.runCulling).mockResolvedValue(progress());
  vi.mocked(api.cancelCulling).mockResolvedValue();
  vi.mocked(api.cullingRating).mockImplementation(async (_, id, _kind, r) => {
    const i = saved.find((i) => i.asset.id === id)!;
    i.user_rating = r;
    i.effective_rating = r ?? i.ai_rating;
  });
  vi.mocked(api.cullingSelectAsset).mockImplementation(
    async (_, id, selected) => {
      saved.find((i) => i.asset.id === id)!.selected_for_editing = selected;
    },
  );
  vi.mocked(api.cullingSelectAssets).mockImplementation(
    async (_, _kind, assetIds) => {
      const selected = new Set(assetIds);
      saved.forEach((item) => {
        item.selected_for_editing = selected.has(item.asset.id);
      });
    },
  );
  vi.mocked(api.cullingSelectRatings).mockImplementation(
    async (
      _,
      _kind,
      ratings,
      relationship = "all",
      selectedOnly = false,
      excludeExactDuplicates = true,
    ) => {
      saved.forEach((i) => {
        i.selected_for_editing =
          i.effective_rating !== null &&
          ratings.includes(i.effective_rating) &&
          matchesRelationship(i, relationship) &&
          (!selectedOnly || i.selected_for_editing) &&
          exactSelectionEligible(i, excludeExactDuplicates);
      });
    },
  );
  vi.mocked(api.cullingDetail).mockImplementation(async (_, id) => {
    const i = saved.find((i) => i.asset.id === id)!;
    const assessment = {
      ...structuredClone(fixture),
      asset_id: id,
      features: { ...structuredClone(fixture.features), asset_id: id },
      ai_rating: i.ai_rating,
      similarity: i.similarity ?? neutralRelationship(),
      duplicate_content:
        i.similarity?.exact?.content ?? fixture.duplicate_content,
    } as CullingAssessment;
    if (i.similarity?.exact) {
      assessment.reasons.push({
        code:
          i.similarity.exact.canonical_asset_id === id
            ? "preferred_copy"
            : "exact_duplicate",
        severity:
          i.similarity.exact.canonical_asset_id === id ? "positive" : "major",
        confidence: 1,
        subject_index: null,
        measurement: null,
      });
    }
    if (i.similarity?.group_id) {
      assessment.reasons.push({
        code: "group_focus_reference",
        severity: "info",
        confidence: 0.8,
        subject_index: null,
        measurement: {
          value: 0.2,
          unit: "normalized_detail",
          reference: 0.25,
        },
      });
    }
    return {
      assessment,
      user_rating: i.user_rating,
      effective_rating: i.effective_rating,
      selected_for_editing: i.selected_for_editing,
      stale: i.stale,
      updated_at: null,
    };
  });
});
async function open(onRunEditing = () => {}) {
  render(
    <CullingScreen
      jobId={job.id}
      onClose={() => {}}
      onRunEditing={onRunEditing}
    />,
  );
  await screen.findByRole("button", { name: "Select photo-1.nef" });
}
const cards = () => document.querySelectorAll(".culling-grid .culling-card");
function withDuplicates() {
  const exact = {
    group_id: "a".repeat(64),
    group_size: 2,
    canonical_asset_id: "photo-1",
    content: fixture.duplicate_content,
  };
  for (const index of [0, 1, 4]) {
    const i = saved[index];
    i.group_id = "1".repeat(64);
    i.similarity = visualRelationship(i.asset.id, ["photo-1"], 3);
    if (index !== 1) {
      i.relationship_kind = "exact";
      i.similarity.exact = structuredClone(exact);
    }
  }
}
it("moves diagnostic counts out of the primary workflow and keeps cards quiet", async () => {
  withDuplicates();
  await open();
  const filters = screen.getByLabelText("Photo filters");
  expect(filters).not.toHaveTextContent("Exact groups");
  expect(filters).not.toHaveTextContent("source-analysis failures");
  const counts = screen.getByLabelText("Duplicate and relationship counts");
  expect(counts).toHaveTextContent(
    "Exact duplicate copies: 1; exact groups: 1",
  );
  expect(counts).toHaveTextContent("Near groups: 1");
  expect(counts).toHaveTextContent("Unique: 2; unclassified: 1");
  expect(screen.getByLabelText("Status photo-1.nef")).toHaveTextContent("BEST");
  expect(screen.getByLabelText("Status photo-5.nef")).toHaveTextContent(
    "DUPLICATE",
  );
  const firstCard = screen
    .getByRole("button", { name: "Select photo-1.nef" })
    .closest("article")!;
  expect(firstCard).not.toHaveTextContent("AI 5★");
  expect(firstCard).not.toHaveTextContent("review signals");
  expect(
    within(firstCard).queryByLabelText("Your rating photo-1.nef"),
  ).toBeNull();
});
it("shows all duplicate alternatives and hides only exact, near and burst alternatives", async () => {
  withDuplicates();
  for (const i of saved.slice(2, 4)) {
    i.relationship_kind = "similar";
    i.similarity = visualRelationship(i.asset.id, ["photo-3"], 2, "similar");
    i.similarity.group_id = "2".repeat(64);
    i.group_id = i.similarity.group_id;
  }
  await open();
  expect(cards()).toHaveLength(6);
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  expect(cards()).toHaveLength(4);
  expect(screen.getByText("4 selected")).toBeInTheDocument();
  for (const checkbox of screen.getAllByRole("checkbox", {
    name: /Include .* for editing/,
  }))
    expect(checkbox).toBeChecked();
  await waitFor(() => {
    expect(saved[0].selected_for_editing).toBe(true);
    expect(saved[1].selected_for_editing).toBe(false);
    expect(saved[4].selected_for_editing).toBe(false);
  });
  expect(
    screen.queryByRole("button", { name: "Select photo-2.nef" }),
  ).toBeNull();
  expect(
    screen.queryByRole("button", { name: "Select photo-5.nef" }),
  ).toBeNull();
  expect(
    screen.getByRole("button", { name: "Select photo-3.nef" }),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Select photo-4.nef" }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Show" }));
  expect(cards()).toHaveLength(6);
});
it("uses the overview's single display representative when scoring retains a tie", async () => {
  saved[1].similarity!.preferred = true;
  saved[1].similarity!.preferred_assets = ["photo-1", "photo-2"];
  expect(saved[1].preferred).toBe(false);
  await open();
  expect(screen.getByLabelText("Status photo-1.nef")).toHaveTextContent("BEST");
  expect(screen.getByLabelText("Status photo-2.nef")).toHaveTextContent(
    "DUPLICATE",
  );
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  expect(
    screen.queryByRole("button", { name: "Select photo-2.nef" }),
  ).toBeNull();
});
it("Show All resets every user-facing filter and selects every photograph", async () => {
  saved[4].issues = ["blurry"];
  saved[2].selected_for_editing = true;
  await open();
  expect(cards()).toHaveLength(5);
  fireEvent.click(screen.getByRole("button", { name: "4★+" }));
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  fireEvent.click(screen.getByRole("button", { name: "Show All" }));
  expect(cards()).toHaveLength(6);
  expect(screen.getByLabelText("Hide blurry photographs")).not.toBeChecked();
  expect(screen.getByRole("button", { name: "All" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(screen.getByRole("button", { name: "Show" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(screen.getByText("6 selected")).toBeInTheDocument();
  expect(screen.getByText("Showing 6 of 6")).toBeInTheDocument();
  for (const item of saved)
    expect(
      screen.getByLabelText(`Include ${item.asset.filename} for editing`),
    ).toBeChecked();
  await waitFor(() =>
    expect(saved.every((item) => item.selected_for_editing)).toBe(true),
  );
});
it("Hide Blurry automatically removes and deselects confidently blurry photographs", async () => {
  saved[4].issues = ["blurry"];
  await open();
  expect(screen.getByLabelText("Hide blurry photographs")).toBeChecked();
  expect(
    screen.queryByRole("button", { name: "Select photo-5.nef" }),
  ).toBeNull();
  fireEvent.click(screen.getByLabelText("Hide blurry photographs"));
  const blurry = screen.getByRole("button", { name: "Select photo-5.nef" });
  expect(blurry.closest("article")).toHaveTextContent("BLURRY");
  expect(screen.getByText("6 selected")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-5.nef for editing"),
  ).toBeChecked();
  fireEvent.click(screen.getByLabelText("Hide blurry photographs"));
  expect(
    screen.queryByRole("button", { name: "Select photo-5.nef" }),
  ).toBeNull();
  expect(screen.getByText("5 selected")).toBeInTheDocument();
  await waitFor(() => expect(saved[4].selected_for_editing).toBe(false));
});
it("clearly disables closed-eye filtering when no detector is installed", async () => {
  await open();
  expect(screen.getByLabelText("Hide closed-eye photographs")).toBeDisabled();
  expect(screen.getByText("Unavailable")).toBeInTheDocument();
  expect(screen.getByText(/Closed-eye provider:/)).toHaveTextContent(
    "unavailable",
  );
});
it("defaults closed-eye hiding on when a detector is available and retains the issue label", async () => {
  issueAvailability.closed_eyes = true;
  saved[3].issues = ["closed_eyes"];
  await open();
  expect(screen.getByLabelText("Hide closed-eye photographs")).toBeEnabled();
  expect(screen.getByLabelText("Hide closed-eye photographs")).toBeChecked();
  expect(
    screen.queryByRole("button", { name: "Select photo-4.nef" }),
  ).toBeNull();
  fireEvent.click(screen.getByLabelText("Hide closed-eye photographs"));
  expect(
    screen
      .getByRole("button", { name: "Select photo-4.nef" })
      .closest("article"),
  ).toHaveTextContent("CLOSED EYES");
  expect(screen.getByText("6 selected")).toBeInTheDocument();
  fireEvent.click(screen.getByLabelText("Hide closed-eye photographs"));
  expect(screen.getByText("5 selected")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Select photo-4.nef" }),
  ).toBeNull();
  await waitFor(() => expect(saved[3].selected_for_editing).toBe(false));
});
it("inspects exact canonical relationships and navigates related thumbnails outside the active filter", async () => {
  withDuplicates();
  await open();
  fireEvent.click(screen.getByLabelText("Filter 1 stars"));
  fireEvent.click(screen.getByRole("button", { name: "Select photo-5.nef" }));
  await screen.findByText("Exact duplicate of photo-1.nef", { exact: false });
  const relation = screen.getByRole("region", {
    name: "Exact duplicate relationship",
  });
  expect(relation).toHaveTextContent("Exact copies — 2 photographs");
  expect(screen.getByText("AI rating: ★☆☆☆☆")).toBeInTheDocument();
  fireEvent.click(
    within(relation).getByRole("button", { name: "Select photo-1.nef" }),
  );
  await screen.findByText("AI rating: ★★★★★");
  expect(cards()).toHaveLength(1);
  expect(api.cullingDetail).toHaveBeenLastCalledWith(
    job.id,
    "photo-1",
    "portrait",
  );
});
it("keeps a manually selected duplicate and its relationship through override, recull and reopen", async () => {
  withDuplicates();
  const view = render(
    <CullingScreen jobId={job.id} onClose={() => {}} onRunEditing={() => {}} />,
  );
  await screen.findByRole("button", { name: "Select photo-5.nef" });
  fireEvent.click(screen.getByLabelText("Include photo-5.nef for editing"));
  await screen.findByText("1 selected");
  fireEvent.click(screen.getByRole("button", { name: "Select photo-5.nef" }));
  fireEvent.change(await screen.findByLabelText("Your rating photo-5.nef"), {
    target: { value: "5" },
  });
  await waitFor(() =>
    expect(
      screen.getByLabelText("Effective rating photo-5.nef"),
    ).toHaveTextContent("★★★★★"),
  );
  expect(saved[4].ai_rating).toBe(1);
  expect(screen.getByText("1 selected")).toBeInTheDocument();
  fireEvent.click(screen.getByText("Re-cull all"));
  await waitFor(() => expect(screen.getByText("Re-cull all")).toBeEnabled());
  expect(saved[4].selected_for_editing).toBe(true);
  expect(saved[4].user_rating).toBe(5);
  expect(saved[4].relationship_kind).toBe("exact");
  view.unmount();
  await open();
  expect(
    screen.getByLabelText("Include photo-5.nef for editing"),
  ).toBeChecked();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-5.nef" }));
  expect(await screen.findByLabelText("Your rating photo-5.nef")).toHaveValue(
    "5",
  );
  expect(screen.getByLabelText("Status photo-5.nef")).toHaveTextContent(
    "DUPLICATE",
  );
  fireEvent.change(screen.getByLabelText("Your rating photo-5.nef"), {
    target: { value: "" },
  });
  await waitFor(() =>
    expect(
      screen.getByLabelText("Effective rating photo-5.nef"),
    ).toHaveTextContent("★☆☆☆☆"),
  );
  expect(
    screen.getByLabelText("Include photo-5.nef for editing"),
  ).toBeChecked();
});
it("labels burst and similar-composition relationships while keeping their rating filters independent", async () => {
  for (const i of saved.slice(0, 2)) {
    i.relationship_kind = "burst";
    i.similarity!.kind = "burst";
  }
  for (const i of saved.slice(2, 4)) {
    i.relationship_kind = "similar";
    i.similarity = visualRelationship(i.asset.id, ["photo-3"], 2, "similar");
    i.similarity.group_id = "2".repeat(64);
    i.group_id = i.similarity.group_id;
    i.preferred = i.asset.id === "photo-3";
  }
  await open();
  expect(screen.getByLabelText("Status photo-1.nef")).toHaveTextContent("BEST");
  expect(screen.getByLabelText("Status photo-2.nef")).toHaveTextContent(
    "DUPLICATE",
  );
  expect(screen.getByLabelText("Status photo-4.nef")).toHaveTextContent(
    "SIMILAR",
  );
  expect(
    screen.getByLabelText("Duplicate and relationship counts"),
  ).toHaveTextContent("burst groups: 1");
  expect(
    screen.getByLabelText("Duplicate and relationship counts"),
  ).toHaveTextContent("similar groups: 1");
  fireEvent.change(screen.getByLabelText("Duplicate filter"), {
    target: { value: "near_similar" },
  });
  expect(cards()).toHaveLength(4);
  fireEvent.click(screen.getByRole("button", { name: "4★+" }));
  expect(cards()).toHaveLength(2);
});
it("bounds related thumbnail rendering and allows paging large exact groups", async () => {
  const base = structuredClone(saved[0]);
  saved = Array.from({ length: 30 }, (_, i) => ({
    ...structuredClone(base),
    asset: asset(`photo-${i + 1}`),
    group_id: null,
    similarity: {
      ...neutralRelationship(),
      exact: {
        group_id: "a".repeat(64),
        group_size: 30,
        canonical_asset_id: "photo-1",
        content: fixture.duplicate_content,
      },
    },
    relationship_kind: "exact",
    preferred: i === 0,
    ai_rating: i === 0 ? 5 : 1,
    effective_rating: i === 0 ? 5 : 1,
  }));
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-1.nef" }));
  const region = await screen.findByRole("region", {
    name: "Exact duplicate relationship",
  });
  expect(
    within(region).getAllByRole("button", { name: /^Select photo-/ }),
  ).toHaveLength(24);
  fireEvent.click(within(region).getByText("Next exact copies"));
  expect(
    within(region).getAllByRole("button", { name: /^Select photo-/ }),
  ).toHaveLength(6);
});
it("uses effective ratings for the simplified All, 5, 4+ and 3+ filters", async () => {
  await open();
  expect(cards()).toHaveLength(6);
  expect(api.runCulling).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Effective rating counts")).toHaveTextContent(
    "5★: 1",
  );
  fireEvent.click(screen.getByRole("button", { name: "5★" }));
  expect(cards()).toHaveLength(1);
  expect(screen.getByText("1 selected")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "4★+" }));
  expect(cards()).toHaveLength(2);
  expect(screen.getByText("2 selected")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "3★+" }));
  expect(cards()).toHaveLength(3);
  expect(screen.getByText("3 selected")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "All" }));
  fireEvent.click(screen.getByLabelText("Filter 1 stars"));
  fireEvent.click(screen.getByLabelText("Filter 5 stars"));
  expect(cards()).toHaveLength(2);
  expect(
    screen.getByRole("button", { name: "Select photo-5.nef" }),
  ).toBeInTheDocument();
});
it("preserves AI while overriding effective rating, counts update, and clear restores AI", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-4.nef" }));
  fireEvent.change(await screen.findByLabelText("Your rating photo-4.nef"), {
    target: { value: "5" },
  });
  await waitFor(() =>
    expect(screen.getByLabelText("Effective rating counts")).toHaveTextContent(
      "5★: 2",
    ),
  );
  expect(saved[3].ai_rating).toBe(2);
  await screen.findByText("Your rating: ★★★★★");
  expect(screen.getByText("AI rating: ★★☆☆☆")).toBeInTheDocument();
  fireEvent.change(screen.getByLabelText("Your rating photo-4.nef"), {
    target: { value: "" },
  });
  await waitFor(() =>
    expect(screen.getByLabelText("Effective rating counts")).toHaveTextContent(
      "5★: 1",
    ),
  );
  expect(saved[3].effective_rating).toBe(2);
});
it("auto-selects the filtered snapshot, allows manual overrides, and runs only that selection", async () => {
  const onRunEditing = vi.fn();
  const view = render(
    <CullingScreen
      jobId={job.id}
      onClose={() => {}}
      onRunEditing={onRunEditing}
    />,
  );
  await screen.findByRole("button", { name: "Select photo-1.nef" });
  expect(
    screen.getByRole("button", { name: "Run for Editing" }),
  ).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "4★+" }));
  expect(screen.getByText("2 selected")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-1.nef for editing"),
  ).toBeChecked();
  expect(
    screen.getByLabelText("Include photo-2.nef for editing"),
  ).toBeChecked();
  await waitFor(() => {
    expect(saved[0].selected_for_editing).toBe(true);
    expect(saved[1].selected_for_editing).toBe(true);
  });
  fireEvent.click(screen.getByLabelText("Include photo-2.nef for editing"));
  expect(screen.getByText("1 selected")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-2.nef for editing"),
  ).not.toBeChecked();
  await waitFor(() => expect(saved[1].selected_for_editing).toBe(false));
  view.unmount();
  await open(onRunEditing);
  expect(screen.getByText("1 selected")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Run for Editing" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Run for Editing" }));
  expect(onRunEditing).toHaveBeenCalledWith("portrait", ["photo-1"]);
  expect(api.runCulling).not.toHaveBeenCalled();
});
it("updates filter and manual checkbox selection immediately while persistence runs in the background", async () => {
  let releaseAuto!: () => void;
  vi.mocked(api.cullingSelectAssets).mockImplementationOnce(
    (_, _kind, assetIds) =>
      new Promise((resolve) => {
        releaseAuto = () => {
          const selected = new Set(assetIds);
          saved.forEach((item) => {
            item.selected_for_editing = selected.has(item.asset.id);
          });
          resolve();
        };
      }),
  );
  await open();
  expect(api.cullingOverview).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", { name: "5★" }));
  expect(screen.getByText("1 selected")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-1.nef for editing"),
  ).toBeChecked();
  expect(saved.every((item) => !item.selected_for_editing)).toBe(true);
  expect(api.cullingOverview).toHaveBeenCalledTimes(1);
  await waitFor(() => expect(api.cullingSelectAssets).toHaveBeenCalledOnce());
  await act(async () => releaseAuto());
  await waitFor(() => expect(saved[0].selected_for_editing).toBe(true));

  let releaseManual!: () => void;
  vi.mocked(api.cullingSelectAssets).mockImplementationOnce(
    (_, _kind, assetIds) =>
      new Promise((resolve) => {
        releaseManual = () => {
          const selected = new Set(assetIds);
          saved.forEach((item) => {
            item.selected_for_editing = selected.has(item.asset.id);
          });
          resolve();
        };
      }),
  );
  fireEvent.click(screen.getByLabelText("Include photo-1.nef for editing"));
  expect(screen.getByText("0 selected")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-1.nef for editing"),
  ).not.toBeChecked();
  expect(
    screen.getByRole("button", { name: "Run for Editing" }),
  ).toBeDisabled();
  expect(saved[0].selected_for_editing).toBe(true);
  expect(api.cullingOverview).toHaveBeenCalledTimes(1);
  await waitFor(() => expect(api.cullingSelectAssets).toHaveBeenCalledTimes(2));
  await act(async () => releaseManual());
  await waitFor(() => expect(saved[0].selected_for_editing).toBe(false));
});
it("keeps Clear Selection at zero until another filter change recomputes the snapshot", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "5★" }));
  expect(screen.getByText("1 selected")).toBeInTheDocument();
  await waitFor(() => expect(saved[0].selected_for_editing).toBe(true));
  fireEvent.click(screen.getByRole("button", { name: "Clear Selection" }));
  expect(screen.getByText("0 selected")).toBeInTheDocument();
  expect(screen.getByText("Showing 1 of 6")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-1.nef for editing"),
  ).not.toBeChecked();
  expect(
    screen.getByRole("button", { name: "Run for Editing" }),
  ).toBeDisabled();
  await waitFor(() =>
    expect(saved.every((item) => !item.selected_for_editing)).toBe(true),
  );
  fireEvent.click(screen.getByRole("button", { name: "4★+" }));
  expect(screen.getByText("2 selected")).toBeInTheDocument();
  expect(
    screen.getByLabelText("Include photo-1.nef for editing"),
  ).toBeChecked();
  expect(
    screen.getByLabelText("Include photo-2.nef for editing"),
  ).toBeChecked();
  await waitFor(() =>
    expect(saved.filter((item) => item.selected_for_editing)).toHaveLength(2),
  );
});
it("replaces 45 stale selections automatically when the five-star filter matches five", async () => {
  const onRunEditing = vi.fn();
  const base = structuredClone(saved[0]);
  saved = Array.from({ length: 52 }, (_, index) => ({
    ...structuredClone(base),
    asset: asset(`photo-${index + 1}`),
    ai_rating: (index < 5 ? 5 : 4) as Stars,
    effective_rating: (index < 5 ? 5 : 4) as Stars,
    selected_for_editing: index < 45,
    group_id: null,
    relationship_kind: "unique" as const,
    similarity: neutralRelationship(),
    issues: [],
  }));
  render(
    <CullingScreen
      jobId={job.id}
      onClose={() => {}}
      onRunEditing={onRunEditing}
    />,
  );
  await screen.findByText("45 selected");
  fireEvent.click(screen.getByRole("button", { name: "5★" }));
  expect(screen.getByText("Showing 5 of 52")).toBeInTheDocument();
  expect(screen.getByText("5 selected")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Select Shown" }),
  ).not.toBeInTheDocument();
  for (const item of saved.slice(0, 5))
    expect(
      screen.getByLabelText(`Include ${item.asset.filename} for editing`),
    ).toBeChecked();
  await waitFor(() =>
    expect(saved.filter((item) => item.selected_for_editing)).toHaveLength(5),
  );
  expect(screen.getByText("Showing 5 of 52")).toBeInTheDocument();
  await waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Run for Editing" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Run for Editing" }));
  expect(onRunEditing).toHaveBeenCalledWith(
    "portrait",
    saved.slice(0, 5).map((item) => item.asset.id),
  );
});
it("duplicate hiding selects only visible representatives and Clear Selection stays authoritative", async () => {
  withDuplicates();
  await open();
  fireEvent.click(screen.getByLabelText("Include photo-5.nef for editing"));
  await screen.findByText("1 selected");
  fireEvent.click(screen.getByRole("button", { name: "Hide" }));
  expect(
    screen.queryByRole("button", { name: "Select photo-5.nef" }),
  ).toBeNull();
  expect(screen.getByText("4 selected")).toBeInTheDocument();
  await waitFor(() => {
    expect(saved[0].selected_for_editing).toBe(true);
    expect(saved[1].selected_for_editing).toBe(false);
    expect(saved[4].selected_for_editing).toBe(false);
  });
  fireEvent.click(screen.getByRole("button", { name: "Clear Selection" }));
  expect(screen.getByText("0 selected")).toBeInTheDocument();
  expect(screen.getByText("Showing 4 of 6")).toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Run for Editing" }),
  ).toBeDisabled();
  await waitFor(() =>
    expect(saved.every((item) => !item.selected_for_editing)).toBe(true),
  );
});
it("rating changes and rerun do not change saved selection", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "4★+" }));
  expect(screen.getByText("2 selected")).toBeInTheDocument();
  await waitFor(() =>
    expect(saved.filter((item) => item.selected_for_editing)).toHaveLength(2),
  );
  fireEvent.click(screen.getByRole("button", { name: "Select photo-1.nef" }));
  fireEvent.change(await screen.findByLabelText("Your rating photo-1.nef"), {
    target: { value: "1" },
  });
  await waitFor(() =>
    expect(screen.getByLabelText("Your rating photo-1.nef")).toHaveValue("1"),
  );
  expect(saved[0].selected_for_editing).toBe(true);
  fireEvent.click(screen.getByText("Re-cull all"));
  await waitFor(() =>
    expect(api.runCulling).toHaveBeenCalledWith(
      expect.objectContaining({ force: true, photo_type: "portrait" }),
    ),
  );
  await waitFor(() => expect(screen.getByText("Re-cull all")).toBeEnabled());
  expect(screen.getByText("2 selected")).toBeInTheDocument();
  expect(saved[0].user_rating).toBe(1);
});
it("supports cancellation and shows preserved progress", async () => {
  let resolve!: (p: CullingProgress) => void;
  vi.mocked(api.runCulling).mockImplementation(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  await open();
  fireEvent.click(screen.getByText("Run / resume culling"));
  await screen.findByText("Cancel culling");
  fireEvent.click(screen.getByText("Cancel culling"));
  await waitFor(() =>
    expect(api.cancelCulling).toHaveBeenCalledWith(
      vi.mocked(api.runCulling).mock.calls[0][0].request_id,
    ),
  );
  await act(async () =>
    resolve({
      ...progress("cancelled"),
      completed: 2,
      stage: "Stopped; completed ratings preserved",
    }),
  );
  await screen.findByText(/cancelled: Stopped; completed ratings preserved/);
  expect(screen.getByText("Run / resume culling")).toBeEnabled();
});
it("reopens a running batch, polls status and can cancel the original request", async () => {
  vi.mocked(api.cullingProgress).mockResolvedValue({
    ...progress("running"),
    photo_type: "landscape",
  });
  await open();
  expect(screen.getByLabelText("Culling photo type")).toHaveValue("landscape");
  fireEvent.click(screen.getByText("Cancel culling"));
  await waitFor(() => expect(api.cancelCulling).toHaveBeenCalledWith("run-1"));
  expect(api.runCulling).not.toHaveBeenCalled();
});
it("shows structured per-person reasons and practical similar group thumbnails", async () => {
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-1.nef" }));
  await screen.findByText("Detected people (5)");
  expect(screen.getByText(/Person 5: Eyes reported open/)).toBeInTheDocument();
  expect(screen.getByText("Similar photos — 2")).toBeInTheDocument();
  expect(screen.getByLabelText("Hide closed-eye photographs")).toBeDisabled();
  expect(
    document.querySelectorAll(".culling-similar .photo-card"),
  ).toHaveLength(2);
  const diagnostics = screen.getByRole("region", {
    name: "Focus diagnostics",
  });
  expect(diagnostics).toHaveTextContent("Internal score before stars");
  expect(diagnostics).toHaveTextContent("Global sharpness");
  expect(diagnostics).toHaveTextContent("Subject sharpness");
  expect(diagnostics).toHaveTextContent("Group median face sharpness: 0.25");
  expect(diagnostics).toHaveTextContent("outlier no");
  expect(diagnostics).toHaveTextContent("Severe-defect gate: none");
});
it("shows the severe face-focus gate and its one-star cap", async () => {
  const original = vi.mocked(api.cullingDetail).getMockImplementation()!;
  saved[0].ai_rating = 1;
  saved[0].effective_rating = 1;
  vi.mocked(api.cullingDetail).mockImplementation(async (jobId, id, kind) => {
    const state = await original(jobId, id, kind);
    if (id !== "photo-1" || !state.assessment?.features) return state;
    state.assessment.ai_rating = 1;
    state.assessment.absolute_score = 19;
    state.assessment.final_score = 19;
    const faces = state.assessment.features.people.faces;
    if (faces.status === "available") {
      faces.value[0].sharpness = {
        status: "available",
        value: 0.154,
        confidence: 0.8,
      };
    }
    state.assessment.reasons.push({
      code: "severe_subject_softness",
      severity: "major",
      confidence: 0.8,
      subject_index: 0,
      measurement: {
        value: 0.154,
        unit: "normalized_detail",
        reference: 0.2,
      },
    });
    return state;
  });
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-1.nef" }));
  const diagnostics = await screen.findByRole("region", {
    name: "Focus diagnostics",
  });
  expect(diagnostics).toHaveTextContent(
    "Severe — technically unusable rating cap fired",
  );
  expect(diagnostics).toHaveTextContent(
    "1★ cap at normalized face detail below 0.2",
  );
});
it("rating keys work on focused thumbnails, never on selects or modified shortcuts", async () => {
  await open();
  const thumbnail = screen.getByRole("button", { name: "Select photo-3.nef" });
  fireEvent.click(thumbnail);
  fireEvent.keyDown(await screen.findByLabelText("Your rating photo-3.nef"), {
    key: "5",
  });
  fireEvent.keyDown(thumbnail, { key: "5", ctrlKey: true });
  expect(api.cullingRating).not.toHaveBeenCalled();
  fireEvent.keyDown(thumbnail, { key: "5" });
  await waitFor(() =>
    expect(api.cullingRating).toHaveBeenCalledWith(
      job.id,
      "photo-3",
      "portrait",
      5,
    ),
  );
});
it("mutation errors remain visible without optimistic rating loss", async () => {
  vi.mocked(api.cullingRating).mockRejectedValue(
    new Error("Database unavailable"),
  );
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-4.nef" }));
  fireEvent.change(await screen.findByLabelText("Your rating photo-4.nef"), {
    target: { value: "5" },
  });
  await screen.findByRole("alert");
  expect(screen.getByRole("alert")).toHaveTextContent("Database unavailable");
  expect(saved[3].effective_rating).toBe(2);
});
it("filters and sorts derive from effective ratings, including unrated and selected subset", () => {
  const items = ([2, 5, null] as const).map((r, i) => ({
    ...saved[i],
    ai_rating: 1 as Stars,
    effective_rating: r,
    selected_for_editing: i === 0,
  }));
  expect(
    filterItems(items, [], false, "rating").map((i) => i.effective_rating),
  ).toEqual([5, 2, null]);
  expect(filterItems(items, [2, 5], true, "rating")).toHaveLength(1);
  expect(filterItems(items, [5], false, "rating")).toHaveLength(1);
  expect(
    filterItems(items, [], false, "filename").map((i) => i.asset.id),
  ).toEqual(items.map((i) => i.asset.id));
});
it("excludes only redundant exact copies from bulk-selection eligibility", () => {
  const exactItems = structuredClone(saved);
  const exact = {
    group_id: "a".repeat(64),
    group_size: 2,
    canonical_asset_id: exactItems[0].asset.id,
    content: fixture.duplicate_content,
  };
  exactItems[0].similarity!.exact = structuredClone(exact);
  exactItems[1].similarity!.exact = structuredClone(exact);
  expect(exactSelectionEligible(exactItems[0], true)).toBe(true);
  expect(exactSelectionEligible(exactItems[1], true)).toBe(false);

  const paired = structuredClone(saved.slice(0, 2));
  expect(paired.every((item) => exactSelectionEligible(item, true))).toBe(true);
  for (const item of paired) item.similarity!.kind = "burst";
  expect(paired.every((item) => exactSelectionEligible(item, true))).toBe(true);
  expect(exactSelectionEligible(exactItems[1], false)).toBe(true);
});
it("changing photo type requests separate AI results without starting analysis", async () => {
  await open();
  fireEvent.change(screen.getByLabelText("Culling photo type"), {
    target: { value: "real_estate" },
  });
  await waitFor(() =>
    expect(api.cullingOverview).toHaveBeenLastCalledWith(job.id, "real_estate"),
  );
  expect(api.runCulling).not.toHaveBeenCalled();
  expect(screen.getByLabelText("Effective rating counts")).toHaveTextContent(
    "Not rated: 1",
  );
});

it("ignores an old inspection response after another asset is selected", async () => {
  const original = vi.mocked(api.cullingDetail).getMockImplementation()!;
  let release!: (value: Awaited<ReturnType<typeof api.cullingDetail>>) => void;
  vi.mocked(api.cullingDetail).mockImplementation((j, id, k) =>
    id === "photo-1"
      ? new Promise((resolve) => {
          release = resolve;
        })
      : original(j, id, k),
  );
  await open();
  fireEvent.click(screen.getByRole("button", { name: "Select photo-1.nef" }));
  fireEvent.click(screen.getByRole("button", { name: "Select photo-4.nef" }));
  await screen.findByText("AI rating: ★★☆☆☆");
  await act(async () => release(await original(job.id, "photo-1", "portrait")));
  expect(screen.getByText("AI rating: ★★☆☆☆")).toBeInTheDocument();
  expect(screen.queryByText("AI rating: ★★★★★")).not.toBeInTheDocument();
});

it("auto-selection includes every filtered asset across pagination", async () => {
  saved = Array.from({ length: 75 }, (_, n) => ({
    ...saved[0],
    asset: asset(`photo-${n + 1}`),
    ai_rating: n < 65 ? 5 : 2,
    effective_rating: n < 65 ? 5 : 2,
    group_id: null,
    relationship_kind: "unique",
    similarity: neutralRelationship(),
  }));
  await open();
  fireEvent.click(screen.getByRole("button", { name: "5★" }));
  expect(cards()).toHaveLength(60);
  expect(screen.getByText("65 selected")).toBeInTheDocument();
  expect(screen.getByText("Showing 65 of 75")).toBeInTheDocument();
  await waitFor(() =>
    expect(saved.slice(0, 65).every((i) => i.selected_for_editing)).toBe(true),
  );
  expect(saved.slice(65).every((i) => !i.selected_for_editing)).toBe(true);
  fireEvent.click(screen.getByText("Next culling page"));
  expect(cards()).toHaveLength(5);
  for (const checkbox of screen.getAllByRole("checkbox", {
    name: /Include .* for editing/,
  }))
    expect(checkbox).toBeChecked();
  expect(api.cullingSelectAssets).toHaveBeenLastCalledWith(
    job.id,
    "portrait",
    expect.arrayContaining(saved.slice(0, 65).map((item) => item.asset.id)),
  );
  fireEvent.click(screen.getByText("Previous culling page"));
  expect(cards()).toHaveLength(60);
});
