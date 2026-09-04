import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { NewJobForm } from "../components/NewJobForm";
import { MetadataPanel } from "../components/MetadataPanel";
import { JobScreen } from "../screens/JobScreen";
import { IngestionWarnings } from "../components/IngestionWarnings";
import { SystemInfo } from "../components/SystemInfo";
import { api, desktopAvailable } from "../api";
import { asset, job } from "./fixtures";

vi.mock("../api", () => ({
  desktopAvailable: vi.fn(() => true),
  errorMessage: (error: { message?: string }) =>
    error.message ?? "Unexpected error",
  api: {
    listJobs: vi.fn(),
    getJob: vi.fn(),
    createJob: vi.fn(),
    resumeJob: vi.fn(),
    listAssets: vi.fn(),
    thumbnail: vi.fn(),
    resources: vi.fn(),
    formats: vi.fn(),
    warnings: vi.fn(),
    chooseFolder: vi.fn(),
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(desktopAvailable).mockReturnValue(true);
  vi.mocked(api.listJobs).mockResolvedValue({
    items: [job],
    offset: 0,
    limit: 12,
    total: 1,
  });
  vi.mocked(api.resources).mockRejectedValue(new Error("optional"));
  vi.mocked(api.formats).mockResolvedValue([]);
  vi.mocked(api.warnings).mockResolvedValue({
    items: [],
    offset: 0,
    limit: 100,
    total: 0,
  });
  vi.mocked(api.getJob).mockResolvedValue(job);
  vi.mocked(api.listAssets).mockResolvedValue({
    items: [asset()],
    offset: 0,
    limit: 60,
    total: 1,
  });
});

describe("desktop UI", () => {
  it("keeps metadata and preview diagnostics separate and loads details on demand", async () => {
    const warningJob = {
      ...job,
      warning_count: 166,
      warnings: { ...job.warnings, metadata: 83, preview: 83 },
    };
    vi.mocked(api.warnings).mockImplementation(
      async (_jobId, offset = 0, limit = 100) => ({
        items: [
          {
            category: "preview",
            code: "preview_capability",
            message: offset
              ? "Second page diagnostic"
              : "HEIF decoder unavailable",
            path: "C:\\Photos\\one.heic",
          },
        ],
        offset,
        limit,
        total: 166,
      }),
    );
    render(<IngestionWarnings job={warningJob} />);
    expect(screen.getAllByText("83")).toHaveLength(2);
    expect(api.warnings).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "View details" }));
    expect(
      await screen.findByText("HEIF decoder unavailable"),
    ).toBeInTheDocument();
    expect(api.warnings).toHaveBeenCalledWith(job.id, 0, 100);
    fireEvent.click(screen.getByRole("button", { name: "Next warnings" }));
    expect(
      await screen.findByText("Second page diagnostic"),
    ).toBeInTheDocument();
    expect(api.warnings).toHaveBeenLastCalledWith(job.id, 100, 100);
  });

  it("shows Apple unified memory without claiming dedicated VRAM", () => {
    render(
      <SystemInfo
        resources={{
          logical_cpu_count: 10,
          total_ram_bytes: 32 * 1024 ** 3,
          available_ram_bytes: 16 * 1024 ** 3,
          gpu_name: "Apple M4",
          gpu_memory_bytes: null,
          available_disk_bytes: null,
          os: "macOS",
          architecture: "aarch64",
          gpu_detection: "detected",
          gpus: [
            {
              vendor: "Apple",
              model: "Apple M4",
              device_type: "integrated",
              memory_model: "unified",
              dedicated_vram_bytes: null,
              shared_memory_budget_bytes: 24 * 1024 ** 3,
              graphics_api: "Metal",
              compute_capability: null,
              detection_source: "Metal device API",
            },
          ],
        }}
      />,
    );
    expect(
      screen.getByText(/Unified memory \(shared with system RAM\)/),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Dedicated VRAM: not applicable / unavailable"),
    ).toBeInTheDocument();
    expect(screen.getByText(/macOS · aarch64/)).toBeInTheDocument();
  });

  it("renders supported formats from the native capability registry", async () => {
    vi.mocked(api.formats).mockResolvedValue([
      {
        file_type: "heic",
        extension: "heic",
        family: "heif",
        discoverable: true,
        metadata_supported: "bundled_exiftool",
        preview_supported: "partial",
        editable_future: true,
        develop_supported: "unavailable",
      },
    ]);
    render(<NewJobForm onCancel={vi.fn()} onCreated={vi.fn()} />);
    expect(
      await screen.findByText(/Still-photo formats: HEIC/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Output may be inside input/)).toBeInTheDocument();
  });
  it("clearly disables local workflows in an ordinary browser", () => {
    vi.mocked(desktopAvailable).mockReturnValue(false);
    render(<App />);
    expect(screen.getByText(/Browser preview/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /New Job/ })).toBeDisabled();
    expect(api.listJobs).not.toHaveBeenCalled();
  });

  it("opens a persisted job and inspects nullable metadata", async () => {
    render(<App />);
    fireEvent.click(
      await screen.findByRole("button", { name: /Studio portraits/ }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Select photo-1.nef" }),
    );
    expect(screen.getByText("Nikon")).toBeInTheDocument();
    expect(screen.getAllByText("Not available").length).toBeGreaterThan(3);
    expect(screen.getByText("No embedded preview")).toBeInTheDocument();
    expect(api.thumbnail).not.toHaveBeenCalled();
  });

  it("requires both folders and creates a trimmed job through the backend", async () => {
    const onCreated = vi.fn();
    vi.mocked(api.chooseFolder)
      .mockResolvedValueOnce(job.input_path)
      .mockResolvedValueOnce(job.output_path);
    vi.mocked(api.createJob).mockResolvedValue(job);
    render(<NewJobForm onCancel={vi.fn()} onCreated={onCreated} />);
    const submit = screen.getByRole("button", { name: "Create & scan job" });
    expect(submit).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Job name"), {
      target: { value: "  Studio portraits  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Browse input" }));
    await waitFor(() =>
      expect(screen.getByLabelText("Input folder")).toHaveValue(job.input_path),
    );
    fireEvent.click(screen.getByRole("button", { name: "Browse output" }));
    await waitFor(() => expect(submit).toBeEnabled());
    fireEvent.click(submit);
    await waitFor(() => expect(onCreated).toHaveBeenCalledWith(job));
    expect(api.createJob).toHaveBeenCalledWith({
      name: job.name,
      input_path: job.input_path,
      output_path: job.output_path,
    });
  });

  it("handles cancelled folder dialogs without starting a job", async () => {
    vi.mocked(api.chooseFolder).mockResolvedValue(null);
    render(<NewJobForm onCancel={vi.fn()} onCreated={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Browse input" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Browse input" }),
      ).toBeEnabled(),
    );
    expect(screen.getByLabelText("Input folder")).toHaveValue("");
    expect(api.createJob).not.toHaveBeenCalled();
  });

  it("paginates a 3,001-photo job, with only 60 cards mounted", async () => {
    vi.mocked(api.getJob).mockResolvedValue({ ...job, asset_count: 3001 });
    vi.mocked(api.listAssets).mockImplementation(
      async (_id, offset, limit) => ({
        items: Array.from({ length: 60 }, (_, i) =>
          asset(`photo-${offset + i}`),
        ),
        total: 3001,
        offset,
        limit,
      }),
    );
    render(<JobScreen jobId={job.id} />);
    await screen.findByRole("button", { name: "Select photo-0.nef" });
    expect(
      screen.getAllByRole("button", { name: /^Select photo-/ }),
    ).toHaveLength(60);
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByRole("button", { name: "Select photo-60.nef" });
    expect(api.listAssets).toHaveBeenLastCalledWith(job.id, 60, 60);
    expect(
      screen.queryByRole("button", { name: "Select photo-0.nef" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: /^Select photo-/ }),
    ).toHaveLength(60);
  });

  it("surfaces IPC errors with a retry control", async () => {
    vi.mocked(api.getJob).mockRejectedValue({
      code: "database",
      message: "Database unavailable",
    });
    render(<JobScreen jobId={job.id} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Database unavailable",
    );
    expect(screen.getByRole("button", { name: "Refresh" })).toBeEnabled();
  });

  it("resumes an interrupted job through the backend", async () => {
    vi.mocked(api.getJob).mockResolvedValue({ ...job, status: "interrupted" });
    vi.mocked(api.resumeJob).mockResolvedValue({ ...job, status: "scanning" });
    render(<JobScreen jobId={job.id} />);
    fireEvent.click(await screen.findByRole("button", { name: "Resume scan" }));
    await waitFor(() => expect(api.resumeJob).toHaveBeenCalledWith(job.id));
  });

  it("renders paths as plain text and preserves camera-local timestamps", () => {
    const photo = asset();
    photo.metadata.capture_timestamp = "2026:09:03 12:34:56";
    photo.original_path = "<script>alert('no')</script>.nef";
    render(<MetadataPanel asset={photo} />);
    expect(screen.getByText(photo.original_path)).toBeInTheDocument();
    expect(screen.getByText("2026:09:03 12:34:56")).toBeInTheDocument();
    expect(document.querySelector("script")).toBeNull();
  });
});
