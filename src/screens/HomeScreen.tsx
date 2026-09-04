import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import { formatDate } from "../format";
import { SystemInfo } from "../components/SystemInfo";
import type { Job, MachineResources, Page } from "../types";

export function HomeScreen({
  onNew,
  onOpen,
  desktop,
}: {
  onNew: () => void;
  onOpen: (id: string) => void;
  desktop: boolean;
}) {
  const [jobs, setJobs] = useState<Page<Job> | null>(null);
  const [resources, setResources] = useState<MachineResources | null>(null);
  const [offset, setOffset] = useState(0);
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(desktop);
  const [error, setError] = useState<string | null>(null);
  const recent = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!desktop) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    void api
      .listJobs(offset)
      .then((page) => {
        if (!cancelled) setJobs(page);
      })
      .catch((error: unknown) => {
        if (!cancelled) setError(errorMessage(error));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktop, offset, revision]);

  useEffect(() => {
    if (!desktop) return;
    let cancelled = false;
    void api
      .resources()
      .then((value) => {
        if (!cancelled) setResources(value);
      })
      .catch(() => {
        /* Resource detection is optional, not a blocker. */
      });
    return () => {
      cancelled = true;
    };
  }, [desktop]);

  return (
    <section className="screen home-screen">
      <header className="topline">
        <span className="eyebrow">YOUR LOCAL PHOTO WORKSPACE</span>
        <span className="pill">Foundation preview</span>
      </header>
      <div className="welcome">
        <h1>
          Good work starts
          <br />
          with a little order.
        </h1>
        <p className="subtitle">
          Bring a shoot together. Browse your originals.
          <br />
          Keep every detail close at hand.
        </p>
      </div>
      <div className="home-actions">
        <button
          className="action-card new-job"
          onClick={onNew}
          disabled={!desktop}
        >
          <span className="action-symbol" aria-hidden="true">
            ＋
          </span>
          <strong>New Job</strong>
          <span>Choose folders and discover your photos</span>
          <span className="card-arrow" aria-hidden="true">
            ↗
          </span>
        </button>
        <button
          className="action-card"
          onClick={() => {
            recent.current?.scrollIntoView({ behavior: "smooth" });
            recent.current?.focus();
          }}
          disabled={!desktop}
        >
          <span className="action-symbol" aria-hidden="true">
            ▤
          </span>
          <strong>Open Existing Job</strong>
          <span>Return to a job saved on this computer</span>
          <span className="card-arrow" aria-hidden="true">
            ↗
          </span>
        </button>
      </div>
      <section
        className="recent-jobs"
        ref={recent}
        tabIndex={-1}
        aria-label="Saved jobs"
      >
        <div className="section-heading">
          <h2>
            Saved jobs <span>{jobs?.total ?? 0}</span>
          </h2>
          <button
            className="quiet"
            onClick={() => setRevision((value) => value + 1)}
            disabled={!desktop || loading}
          >
            Refresh
          </button>
        </div>
        {error && (
          <div role="alert" className="error">
            {error}
          </div>
        )}
        {loading ? (
          <div className="empty-state" role="status">
            Loading local jobs…
          </div>
        ) : !jobs?.items.length ? (
          <div className="empty-state">
            <span className="empty-icon" aria-hidden="true">
              ▧
            </span>
            <h3>Your workspace is ready.</h3>
            <p>
              Create your first job to start a contact sheet.
              <br />
              No uploads. No changes to your originals.
            </p>
          </div>
        ) : (
          <div className="job-list">
            {jobs.items.map((job) => (
              <button
                className="job-row"
                key={job.id}
                onClick={() => onOpen(job.id)}
              >
                <span className="job-icon" aria-hidden="true">
                  ▧
                </span>
                <span className="job-description">
                  <strong>{job.name}</strong>
                  <small>
                    {job.asset_count.toLocaleString()} photos ·{" "}
                    {formatDate(job.updated_at)}
                  </small>
                </span>
                <span className={`pill ${job.status}`}>{job.status}</span>
                <span aria-hidden="true">→</span>
              </button>
            ))}
          </div>
        )}
        {!!jobs && jobs.total > jobs.limit && (
          <div className="pagination">
            <button
              disabled={offset === 0 || loading}
              onClick={() => setOffset(Math.max(0, offset - jobs.limit))}
            >
              Previous
            </button>
            <span>Page {Math.floor(offset / jobs.limit) + 1}</span>
            <button
              disabled={offset + jobs.limit >= jobs.total || loading}
              onClick={() => setOffset(offset + jobs.limit)}
            >
              Next
            </button>
          </div>
        )}
      </section>
      <footer className="resource-bar">
        <span>
          <span className="status-dot" /> Local-first foundation
        </span>
        {resources ? (
          <SystemInfo resources={resources} />
        ) : (
          <span>Windows 11 x64 & macOS Apple Silicon</span>
        )}
      </footer>
    </section>
  );
}
