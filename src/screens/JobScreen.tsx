import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { pageRange } from "../format";
import { MetadataPanel } from "../components/MetadataPanel";
import { Thumbnail } from "../components/Thumbnail";
import { FormatHint } from "../components/FormatHint";
import { IngestionWarnings } from "../components/IngestionWarnings";
import { DevelopmentPanel } from "../components/DevelopmentPanel";
import type { Asset, Job, Page } from "../types";
import type { PhotoType } from "../analysis";
import { CullingScreen } from "./CullingScreen";
import { PresetEditingScreen } from "./PresetEditingScreen";

const PAGE_SIZE = 60;

export function JobScreen({ jobId }: { jobId: string }) {
  const [job, setJob] = useState<Job | null>(null);
  const [page, setPage] = useState<Page<Asset> | null>(null);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<Asset | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [resuming, setResuming] = useState(false);
  const [revision, setRevision] = useState(0);
  const [developing, setDeveloping] = useState(false);
  const [culling, setCulling] = useState(false);
  const [editing, setEditing] = useState<{
    photoType: PhotoType;
    selectedAssetIds: string[];
  } | null>(null);
  const generation = useRef(0);

  // One in-flight polling chain. Generation + cleanup prevent stale page/job responses.
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const request = ++generation.current;
    setLoading(true);
    setError(null);
    async function load() {
      try {
        const nextPage = await api.listAssets(jobId, offset, PAGE_SIZE);
        // Page repair can change diagnostics; read the summary after those writes.
        const nextJob = await api.getJob(jobId);
        if (cancelled || generation.current !== request) return;
        setJob(nextJob);
        setPage(nextPage);
        setSelected((current) =>
          current
            ? (nextPage.items.find((asset) => asset.id === current.id) ?? null)
            : null,
        );
        setLoading(false);
        if (nextJob.status === "scanning")
          timer = setTimeout(() => void load(), 1200);
      } catch (error) {
        if (!cancelled && generation.current === request) {
          setError(errorMessage(error));
          setLoading(false);
        }
      }
    }
    void load();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [jobId, offset, revision]);

  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  async function resume() {
    setResuming(true);
    setError(null);
    try {
      const next = await api.resumeJob(jobId);
      setJob(next);
      setOffset(0);
      refresh();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setResuming(false);
    }
  }

  if (editing)
    return (
      <PresetEditingScreen
        key={jobId}
        jobId={jobId}
        photoType={editing.photoType}
        initialSelectedAssetIds={editing.selectedAssetIds}
        onBack={() => setEditing(null)}
      />
    );
  if (culling)
    return (
      <CullingScreen
        key={jobId}
        jobId={jobId}
        onClose={() => setCulling(false)}
        onRunEditing={(photoType, selectedAssetIds) =>
          setEditing({ photoType, selectedAssetIds })
        }
      />
    );
  return (
    <section className="screen job-screen">
      <div className="eyebrow">LOCAL WORKSPACE / JOB</div>
      <header className="job-header">
        <div>
          <h1>{job?.name ?? "Opening job…"}</h1>
          <p className="subtitle">
            {job?.asset_count.toLocaleString() ?? "—"} photos discovered{" "}
            {job && (
              <span className={`pill ${job.status}`}>
                {job.status === "ready" ? "Ready to browse" : job.status}
              </span>
            )}
          </p>
        </div>
        <div className="header-actions">
          <button
            disabled={!job || job.status === "scanning"}
            onClick={() => setCulling(true)}
          >
            AI Culling
          </button>
          <button onClick={refresh} disabled={loading || resuming}>
            Refresh
          </button>
          <button
            onClick={() => void resume()}
            disabled={!job || job.status === "scanning" || resuming}
          >
            {resuming
              ? "Starting…"
              : job?.status === "interrupted" || job?.status === "failed"
                ? "Resume scan"
                : "Rescan folders"}
          </button>
        </div>
      </header>
      {job && (
        <div className="job-paths">
          <div>
            <span>INPUT FOLDER</span>
            <p title={job.input_path}>{job.input_path}</p>
          </div>
          <div>
            <span>OUTPUT FOLDER · EXPORTS</span>
            <p title={job.output_path}>{job.output_path}</p>
          </div>
        </div>
      )}
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      {job?.last_error && <div className="notice">{job.last_error}</div>}
      {job?.status === "scanning" && (
        <div className="scan-status" role="status">
          <span className="spinner" /> Discovering photos and preparing previews
          in the background. You can leave this screen; progress is saved in
          batches.
        </div>
      )}
      {job && <IngestionWarnings job={job} />}
      {selected && (
        <button onClick={() => setDeveloping((v) => !v)}>
          {developing ? "Close development panel" : "Develop selected photo"}
        </button>
      )}
      {selected && developing && (
        <DevelopmentPanel
          key={`${selected.job_id}-${selected.id}`}
          asset={selected}
        />
      )}
      <div className="job-content">
        <div className="contact-sheet">
          <div className="section-heading">
            <h2>Contact sheet</h2>
            <span className="muted">
              {page
                ? pageRange(page.offset, page.limit, page.total)
                : "Loading…"}
            </span>
          </div>
          {loading ? (
            <div className="empty-state" role="status">
              Loading this page and restoring any missing cached previews…
            </div>
          ) : page?.items.length ? (
            <div className="photo-grid">
              {page.items.map((asset) => (
                <Thumbnail
                  key={`${asset.id}-${asset.fingerprint}`}
                  asset={asset}
                  selected={selected?.id === asset.id}
                  onSelect={() => setSelected(asset)}
                />
              ))}
            </div>
          ) : (
            <div className="empty-state">
              <span className="empty-icon" aria-hidden="true">
                ▧
              </span>
              <h3>
                {job?.status === "scanning"
                  ? "Your contact sheet is on its way."
                  : "No photos to show."}
              </h3>
              <p>
                {job?.status === "scanning" ? (
                  "The first photos appear after a small batch is saved."
                ) : (
                  <>
                    <FormatHint /> Check your input folder and rescan.
                  </>
                )}
              </p>
            </div>
          )}
          {page && (
            <div className="pagination">
              <button
                disabled={offset === 0 || loading}
                onClick={() => {
                  setOffset(Math.max(0, offset - PAGE_SIZE));
                  setSelected(null);
                }}
              >
                Previous
              </button>
              <span>
                Page {Math.floor(offset / PAGE_SIZE) + 1} of{" "}
                {Math.max(1, Math.ceil(page.total / PAGE_SIZE))}
              </span>
              <button
                disabled={offset + PAGE_SIZE >= page.total || loading}
                onClick={() => {
                  setOffset(offset + PAGE_SIZE);
                  setSelected(null);
                }}
              >
                Next
              </button>
            </div>
          )}
        </div>
        <MetadataPanel asset={selected} />
      </div>
    </section>
  );
}
