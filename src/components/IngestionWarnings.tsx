import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { IngestionWarning, Job, Page, WarningCategory } from "../types";

const labels: Record<WarningCategory, string> = {
  metadata: "Metadata",
  preview: "Preview",
  unreadable: "Unreadable files",
  access: "Access",
  traversal: "Traversal",
};
export function IngestionWarnings({ job }: { job: Job }) {
  const [open, setOpen] = useState(false);
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<Page<IngestionWarning> | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setPage(null);
    setError(null);
    void api
      .warnings(job.id, offset, 100)
      .then((value) => {
        if (!cancelled) setPage(value);
      })
      .catch((error) => {
        if (!cancelled) setError(errorMessage(error));
      });
    return () => {
      cancelled = true;
    };
  }, [open, offset, job.id, job.updated_at]);
  return (
    <section className="ingestion-warnings" aria-label="Ingestion diagnostics">
      <div className="warning-summary">
        {Object.entries(labels).map(([key, label]) => (
          <span key={key}>
            {label}:{" "}
            <strong>
              {job.warnings[key as WarningCategory].toLocaleString()}
            </strong>
          </span>
        ))}
        <button
          className="quiet"
          disabled={!job.warning_count && !open}
          aria-expanded={open}
          onClick={() => {
            setOpen((value) => !value);
            setOffset(0);
          }}
        >
          {open ? "Hide details" : "View details"}
        </button>
      </div>
      {open && (
        <div className="warning-details">
          <p className="field-hint">
            Diagnostics describe separate capabilities, not rejected photos.
            Ordinary folders and unsupported files do not produce warnings.
          </p>
          {error && (
            <p role="alert" className="error">
              {error}
            </p>
          )}
          {!page && !error && <p role="status">Loading warning details…</p>}
          {page?.items.map((warning, index) => (
            <div className="warning-entry" key={`${page.offset}-${index}`}>
              <span className="pill">{labels[warning.category]}</span>
              <p>{warning.message}</p>
              {warning.path && <code>{warning.path}</code>}
              <small>{warning.code}</small>
            </div>
          ))}
          {page && (
            <div className="pagination">
              <button
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - page.limit))}
              >
                Previous warnings
              </button>
              <span>
                {page.total.toLocaleString()} diagnostics · Page{" "}
                {Math.floor(offset / page.limit) + 1}
              </span>
              <button
                disabled={offset + page.limit >= page.total}
                onClick={() => setOffset(offset + page.limit)}
              >
                Next warnings
              </button>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
