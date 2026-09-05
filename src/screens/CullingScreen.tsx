import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import type { PhotoType } from "../analysis";
import {
  filterItems,
  relationshipReason,
  relationshipBadge,
  relationshipLabels,
  type DuplicateVisibility,
  type RelationshipFilter,
  starText,
  starValues,
  type Stars,
  type CullingOverview,
  type CullingState,
  type CullingProgress,
  type CullingItem,
} from "../culling";
import { Thumbnail } from "../components/Thumbnail";

export function CullingScreen({
  jobId,
  onClose,
  onRunEditing,
}: {
  jobId: string;
  onClose: () => void;
  onRunEditing: (photoType: PhotoType, selectedAssetIds: string[]) => void;
}) {
  const [kind, setKind] = useState<PhotoType>("portrait");
  const [ready, setReady] = useState(false);
  const [overview, setOverview] = useState<CullingOverview | null>(null);
  const [progress, setProgress] = useState<CullingProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [pending, setPending] = useState(false);
  const [selectionSyncing, setSelectionSyncing] = useState(false);
  const [running, setRunning] = useState(false);
  const [ratings, setRatings] = useState<Stars[]>([]);
  const [relationship, setRelationship] = useState<RelationshipFilter>("all");
  const [selectedOnly, setSelectedOnly] = useState(false);
  const [duplicates, setDuplicates] = useState<DuplicateVisibility>("show");
  const [hideBlurry, setHideBlurry] = useState(true);
  const [hideClosedEyes, setHideClosedEyes] = useState(true);
  const [sort, setSort] = useState("rating");
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<CullingState | null>(null);
  const mounted = useRef(true);
  const activeRequest = useRef<string | null>(null);
  const selectionQueue = useRef<Promise<void>>(Promise.resolve());
  const selectionOperation = useRef(0);
  useEffect(() => {
    mounted.current = true;
    let cancelled = false;
    void api
      .cullingProgress(jobId)
      .then((p) => {
        if (!cancelled) {
          if (p) setKind(p.photo_type);
          activeRequest.current = p?.request_id ?? null;
          setProgress(p);
          setRunning(!!p && ["queued", "running"].includes(p.status));
          setReady(true);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(errorMessage(e));
          setReady(true);
        }
      });
    return () => {
      cancelled = true;
      mounted.current = false;
    };
  }, [jobId]);
  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    setOverview(null);
    void api
      .cullingOverview(jobId, kind)
      .then((o) => {
        if (!cancelled) setOverview(o);
      })
      .catch((e) => {
        if (!cancelled) setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [jobId, kind, revision, ready]);
  useEffect(() => {
    if (!running) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const p = await api.cullingProgress(jobId);
        if (cancelled) return;
        if (p && p.request_id === activeRequest.current) {
          setProgress(p);
          if (!["queued", "running"].includes(p.status)) {
            if (p.error) setError(p.error);
            setRunning(false);
            setRevision((v) => v + 1);
            return;
          }
        }
      } catch (e) {
        if (!cancelled) setError(errorMessage(e));
      }
      if (!cancelled) timer = setTimeout(() => void poll(), 1000);
    }
    timer = setTimeout(() => void poll(), 500);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [running, jobId]);
  useEffect(() => {
    setDetail(null);
    if (!selected) return;
    let cancelled = false;
    void api
      .cullingDetail(jobId, selected, kind)
      .then((s) => {
        if (!cancelled) setDetail(s);
      })
      .catch((e) => {
        if (!cancelled) setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selected, kind, jobId, revision]);
  async function mutate(work: () => Promise<void>) {
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      await work();
      if (mounted.current) setRevision((v) => v + 1);
    } catch (e) {
      if (mounted.current) setError(errorMessage(e));
    } finally {
      if (mounted.current) setPending(false);
    }
  }
  async function run(force: boolean) {
    setError(null);
    setRunning(true);
    const request_id = crypto.randomUUID();
    activeRequest.current = request_id;
    setProgress({
      job_id: jobId,
      request_id,
      photo_type: kind,
      status: "queued",
      stage: "Queued",
      completed: 0,
      total: overview?.items.length ?? 0,
      failed: 0,
      cached: 0,
      duration_ms: 0,
      error: null,
      hash_bytes: 0,
      hash_cached: 0,
      hash_duration_ms: 0,
      hash_failures: 0,
    });
    try {
      const p = await api.runCulling({
        job_id: jobId,
        photo_type: kind,
        request_id,
        force,
      });
      if (mounted.current && activeRequest.current === request_id) {
        setProgress(p);
        if (p.error) setError(p.error);
      }
    } catch (e) {
      if (mounted.current && activeRequest.current === request_id)
        setError(errorMessage(e));
    } finally {
      if (mounted.current && activeRequest.current === request_id) {
        setRunning(false);
        setRevision((v) => v + 1);
      }
    }
  }
  const eyeStateAvailable = overview?.issue_availability.closed_eyes ?? false;
  const items = filterItems(
    overview?.items ?? [],
    ratings,
    selectedOnly,
    sort,
    relationship,
    {
      duplicates,
      hideBlurry,
      hideClosedEyes: eyeStateAvailable && hideClosedEyes,
    },
  );
  const pageCount = Math.max(1, Math.ceil(items.length / 60));
  const currentPage = Math.min(page, pageCount - 1);
  const shown = items.slice(currentPage * 60, (currentPage + 1) * 60);
  const selectedItem = overview?.items.find((i) => i.asset.id === selected);
  const group = selectedItem?.group_id
    ? (overview?.items.filter((i) => i.group_id === selectedItem.group_id) ??
      [])
    : [];
  const busy = pending || running;
  const assessment = detail?.assessment;
  const people = assessment?.features?.people;
  const technical = assessment?.features?.technical;
  const severeFocusGate = assessment?.reasons.find(
    (reason) => reason.code === "severe_subject_softness",
  );
  const groupFocus = assessment?.reasons.find(
    (reason) => reason.code === "group_focus_reference",
  )?.measurement;
  const hasStrongFace = assessment?.reasons.some(
    (reason) => reason.code === "face_sharp",
  );
  const focusSeverity = severeFocusGate
    ? "Severe — technically unusable rating cap fired"
    : hasStrongFace
      ? "Strong measured face detail"
      : "Review — no severe focus gate fired";
  const exact = selectedItem?.similarity?.exact;
  const exactMembers = exact
    ? (overview?.items.filter(
        (i) => i.similarity?.exact?.group_id === exact.group_id,
      ) ?? [])
    : [];
  const canonical = exactMembers.find(
    (i) => i.asset.id === exact?.canonical_asset_id,
  );
  function replaceSelection(assetIds: string[], targetKind = kind) {
    const ids = [...new Set(assetIds)];
    const selectedIds = new Set(ids);
    setOverview((current) =>
      current
        ? {
            ...current,
            selected_count: ids.length,
            items: current.items.map((item) => ({
              ...item,
              selected_for_editing: selectedIds.has(item.asset.id),
            })),
          }
        : current,
    );
    const operation = ++selectionOperation.current;
    setSelectionSyncing(true);
    setError(null);
    const persisted = selectionQueue.current
      .catch(() => {})
      .then(() => api.cullingSelectAssets(jobId, targetKind, ids));
    selectionQueue.current = persisted;
    void persisted
      .catch((cause) => {
        if (mounted.current && selectionOperation.current === operation) {
          setError(errorMessage(cause));
          setOverview(null);
          setRevision((value) => value + 1);
        }
      })
      .finally(() => {
        if (mounted.current && selectionOperation.current === operation)
          setSelectionSyncing(false);
      });
  }
  function changeFilters(
    next: Partial<{
      ratings: Stars[];
      relationship: RelationshipFilter;
      selectedOnly: boolean;
      duplicates: DuplicateVisibility;
      hideBlurry: boolean;
      hideClosedEyes: boolean;
    }>,
  ) {
    const nextRatings = next.ratings ?? ratings;
    const nextRelationship = next.relationship ?? relationship;
    const nextSelectedOnly = next.selectedOnly ?? selectedOnly;
    const nextDuplicates = next.duplicates ?? duplicates;
    const nextHideBlurry = next.hideBlurry ?? hideBlurry;
    const nextHideClosedEyes = next.hideClosedEyes ?? hideClosedEyes;
    if (next.ratings !== undefined) setRatings(nextRatings);
    if (next.relationship !== undefined) setRelationship(nextRelationship);
    if (next.selectedOnly !== undefined) setSelectedOnly(nextSelectedOnly);
    if (next.duplicates !== undefined) setDuplicates(nextDuplicates);
    if (next.hideBlurry !== undefined) setHideBlurry(nextHideBlurry);
    if (next.hideClosedEyes !== undefined)
      setHideClosedEyes(nextHideClosedEyes);
    setPage(0);
    if (!overview) return;
    replaceSelection(
      filterItems(
        overview.items,
        nextRatings,
        nextSelectedOnly,
        sort,
        nextRelationship,
        {
          duplicates: nextDuplicates,
          hideBlurry: nextHideBlurry,
          hideClosedEyes: eyeStateAvailable && nextHideClosedEyes,
        },
      ).map((item) => item.asset.id),
    );
  }
  function setAssetSelection(assetId: string, isSelected: boolean) {
    if (!overview) return;
    replaceSelection(
      overview.items
        .filter((item) =>
          item.asset.id === assetId ? isSelected : item.selected_for_editing,
        )
        .map((item) => item.asset.id),
    );
  }
  function showAll() {
    changeFilters({
      ratings: [],
      relationship: "all",
      selectedOnly: false,
      duplicates: "show",
      hideBlurry: false,
      hideClosedEyes: false,
    });
  }
  return (
    <section className="screen culling-screen">
      <div className="eyebrow">SOURCE SELECTION / LOCAL AI CULLING</div>
      <header className="job-header">
        <div>
          <h1>Choose the photographs worth editing.</h1>
          <p className="subtitle">
            Recommendations, not deletions. Originals and editing recipes stay
            untouched.
          </p>
        </div>
        <button onClick={onClose}>Back to contact sheet</button>
      </header>
      <div className="culling-toolbar">
        <label>
          Photo type{" "}
          <select
            aria-label="Culling photo type"
            value={kind}
            disabled={busy || !ready}
            onChange={(e) => {
              setKind(e.target.value as PhotoType);
              setSelected(null);
              setPage(0);
            }}
          >
            <option value="portrait">Portrait</option>
            <option value="real_estate">Real estate</option>
            <option value="landscape">Landscape</option>
          </select>
        </label>
        <button disabled={busy || !overview} onClick={() => void run(false)}>
          Run / resume culling
        </button>
        <button disabled={busy || !overview} onClick={() => void run(true)}>
          Re-cull all
        </button>
        {running && (
          <button
            disabled={pending || !progress}
            onClick={() =>
              void mutate(() => api.cancelCulling(progress!.request_id))
            }
          >
            Cancel culling
          </button>
        )}
        <button disabled={pending} onClick={() => setRevision((v) => v + 1)}>
          Refresh ratings
        </button>
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {progress && (
        <div role="status" className="notice">
          {running ? "Culling photographs" : progress.stage} ·{" "}
          {progress.completed} of {progress.total}
        </div>
      )}
      <div className="culling-filters" aria-label="Photo filters">
        <fieldset>
          <legend>Rating</legend>
          <div className="culling-filter-buttons">
            <button
              aria-pressed={!ratings.length}
              onClick={() => {
                changeFilters({ ratings: [] });
              }}
            >
              All
            </button>
            <button
              aria-pressed={ratings.length === 1 && ratings[0] === 5}
              onClick={() => {
                changeFilters({ ratings: [5] });
              }}
            >
              5★
            </button>
            <button
              aria-pressed={
                ratings.length === 2 &&
                ratings.includes(4) &&
                ratings.includes(5)
              }
              onClick={() => {
                changeFilters({ ratings: [4, 5] });
              }}
            >
              4★+
            </button>
            <button
              aria-pressed={
                ratings.length === 3 &&
                ratings.includes(3) &&
                ratings.includes(4) &&
                ratings.includes(5)
              }
              onClick={() => {
                changeFilters({ ratings: [3, 4, 5] });
              }}
            >
              3★+
            </button>
          </div>
        </fieldset>
        <fieldset>
          <legend>Duplicates</legend>
          <div className="culling-filter-buttons">
            <button
              aria-pressed={duplicates === "show"}
              onClick={() => {
                changeFilters({ duplicates: "show" });
              }}
            >
              Show
            </button>
            <button
              aria-pressed={duplicates === "hide"}
              onClick={() => {
                changeFilters({ duplicates: "hide" });
              }}
            >
              Hide
            </button>
          </div>
        </fieldset>
        <fieldset>
          <legend>Issues</legend>
          <label className="culling-issue-toggle">
            <input
              type="checkbox"
              aria-label="Hide blurry photographs"
              checked={hideBlurry}
              onChange={(e) => {
                changeFilters({ hideBlurry: e.target.checked });
              }}
            />
            Hide Blurry
          </label>
          <label className="culling-issue-toggle">
            <input
              type="checkbox"
              aria-label="Hide closed-eye photographs"
              disabled={!eyeStateAvailable}
              checked={eyeStateAvailable && hideClosedEyes}
              onChange={(e) => {
                changeFilters({ hideClosedEyes: e.target.checked });
              }}
            />
            Hide Closed Eyes
            {!eyeStateAvailable && <small>Unavailable</small>}
          </label>
        </fieldset>
        <button className="culling-show-all" onClick={showAll}>
          Show All
        </button>
      </div>
      <div className="culling-selection">
        <div className="culling-selection-counts">
          <strong>{overview?.selected_count ?? 0} selected</strong>
          <span>
            Showing {items.length} of {overview?.items.length ?? 0}
          </span>
        </div>
        <button
          disabled={busy || !overview}
          onClick={() => replaceSelection([])}
        >
          Clear Selection
        </button>
        <button
          className="primary"
          disabled={busy || selectionSyncing || !overview?.selected_count}
          onClick={() =>
            onRunEditing(
              kind,
              (overview?.items ?? [])
                .filter((item) => item.selected_for_editing)
                .map((item) => item.asset.id),
            )
          }
        >
          Run for Editing
        </button>
      </div>
      <div className="job-content">
        <div>
          <p className="culling-page-count">
            Page {currentPage + 1} of {pageCount}
          </p>
          {!overview ? (
            <p role="status">Loading saved ratings…</p>
          ) : (
            <div className="photo-grid culling-grid">
              {shown.map((i) => (
                <article
                  className="culling-card"
                  key={i.asset.id}
                  onKeyDown={(e) => {
                    const target = e.target as HTMLElement;
                    if (
                      busy ||
                      !(target instanceof HTMLButtonElement) ||
                      !target.classList.contains("photo-card") ||
                      e.altKey ||
                      e.ctrlKey ||
                      e.metaKey ||
                      !/[1-5]/.test(e.key) ||
                      e.key.length !== 1
                    )
                      return;
                    e.preventDefault();
                    void mutate(() =>
                      api.cullingRating(
                        jobId,
                        i.asset.id,
                        kind,
                        Number(e.key) as Stars,
                      ),
                    );
                  }}
                >
                  <Thumbnail
                    asset={i.asset}
                    selected={selected === i.asset.id}
                    onSelect={() => setSelected(i.asset.id)}
                  />
                  <div className="culling-card-controls">
                    <div className="culling-card-badges">
                      {relationshipBadge(i) && (
                        <span
                          className={`culling-relationship ${i.relationship_kind === "exact" ? "exact" : ""}`}
                          aria-label={`Status ${i.asset.filename}`}
                        >
                          {relationshipBadge(i)}
                        </span>
                      )}
                      {i.issues.includes("blurry") && (
                        <span className="culling-issue">BLURRY</span>
                      )}
                      {i.issues.includes("closed_eyes") && (
                        <span className="culling-issue">CLOSED EYES</span>
                      )}
                    </div>
                    <strong aria-label={`Effective rating ${i.asset.filename}`}>
                      {starText(i.effective_rating)}
                    </strong>
                    <label>
                      <input
                        type="checkbox"
                        aria-label={`Include ${i.asset.filename} for editing`}
                        disabled={busy}
                        checked={i.selected_for_editing}
                        onChange={(e) =>
                          setAssetSelection(i.asset.id, e.target.checked)
                        }
                      />
                      Select
                    </label>
                  </div>
                </article>
              ))}
            </div>
          )}
          <div className="pagination">
            <button
              disabled={!currentPage}
              onClick={() => setPage(currentPage - 1)}
            >
              Previous culling page
            </button>
            <button
              disabled={currentPage + 1 >= pageCount}
              onClick={() => setPage(currentPage + 1)}
            >
              Next culling page
            </button>
          </div>
        </div>
        <aside className="culling-inspector">
          <h2>Culling inspector</h2>
          {!selected ? (
            <p>Select a photo to inspect reasons and compare similar frames.</p>
          ) : !detail ? (
            <p>Loading assessment…</p>
          ) : (
            <>
              <h3>{selectedItem?.asset.filename}</h3>
              <p>
                AI rating: {starText(assessment?.ai_rating ?? null)}
                {detail.stale ? " (stale)" : ""}
              </p>
              <p>Your rating: {starText(detail.user_rating)}</p>
              <p>Effective: {starText(detail.effective_rating)}</p>
              <label>
                Your rating{" "}
                <select
                  aria-label={`Your rating ${selectedItem?.asset.filename}`}
                  disabled={busy}
                  value={detail.user_rating ?? ""}
                  onChange={(e) =>
                    void mutate(() =>
                      api.cullingRating(
                        jobId,
                        selectedItem!.asset.id,
                        kind,
                        e.target.value
                          ? (Number(e.target.value) as Stars)
                          : null,
                      ),
                    )
                  }
                >
                  <option value="">Use AI / clear override</option>
                  {starValues.map((n) => (
                    <option key={n} value={n}>
                      {n}★
                    </option>
                  ))}
                </select>
              </label>
              <p>
                Relationship:{" "}
                {selectedItem?.relationship_kind
                  ? relationshipLabels[selectedItem.relationship_kind]
                  : "Unclassified / needs complete cull"}
              </p>
              {detail.stale && (
                <p className="notice">
                  Stored evidence below is stale. Resume culling to refresh
                  relationships; your rating and editing selection are
                  preserved.
                </p>
              )}
              {exact && (
                <section aria-label="Exact duplicate relationship">
                  <h3>Exact copies — {exact.group_size} photographs</h3>
                  <p>
                    Identical complete file bytes, including metadata. Preferred
                    copy:{" "}
                    <button
                      title={canonical?.asset.original_path}
                      onClick={() => setSelected(exact.canonical_asset_id)}
                    >
                      {canonical?.asset.filename ?? exact.canonical_asset_id}
                    </button>
                  </p>
                  <p>
                    Redundant copies receive AI 1★. Your rating and explicit
                    inclusion take precedence.
                  </p>
                  <RelatedPhotos
                    key={exact.group_id}
                    items={exactMembers}
                    selected={selected}
                    onSelect={setSelected}
                    canonical={exact.canonical_asset_id}
                  />
                </section>
              )}
              {assessment && (
                <>
                  <p>
                    Evidence confidence:{" "}
                    {Math.round(assessment.confidence * 100)}% (not a calibrated
                    probability)
                  </p>
                  <section aria-label="Focus diagnostics">
                    <h3>Focus diagnostics</h3>
                    <p>
                      Internal score before stars: {assessment.final_score} ·
                      Absolute score before group adjustment:{" "}
                      {assessment.absolute_score}
                    </p>
                    <p>
                      Global sharpness: {technical?.global_sharpness ?? "N/A"}{" "}
                      laplacian_rms
                    </p>
                    <p>
                      Subject sharpness:{" "}
                      {technical?.subject_sharpness.status === "available"
                        ? `${technical.subject_sharpness.value} edge_strength (${Math.round(technical.subject_sharpness.confidence * 100)}% evidence)`
                        : (technical?.subject_sharpness.status ?? "N/A")}
                    </p>
                    <p>
                      Face sharpness:{" "}
                      {people?.faces.status === "available"
                        ? people.faces.value
                            .filter((face) => face.relevant)
                            .map((face) =>
                              face.sharpness.status === "available"
                                ? `Person ${face.index + 1}: ${face.sharpness.value} (${Math.round(face.sharpness.confidence * 100)}%)`
                                : `Person ${face.index + 1}: ${face.sharpness.status}`,
                            )
                            .join(" · ") || "No important face measurement"
                        : (people?.faces.status ?? "N/A")}
                    </p>
                    <p>
                      Group median face sharpness:{" "}
                      {groupFocus?.reference ?? "N/A"}
                      {groupFocus &&
                      groupFocus.reference !== null &&
                      groupFocus.reference > 0
                        ? ` · selected/group ratio ${(
                            groupFocus.value / groupFocus.reference
                          ).toFixed(2)} · outlier ${
                            groupFocus.value < groupFocus.reference * 0.7
                              ? "yes"
                              : "no"
                          }`
                        : ""}
                    </p>
                    <p>Blur/focus severity: {focusSeverity}</p>
                    <p>
                      Severe-defect gate:{" "}
                      {severeFocusGate
                        ? `1★ cap at normalized face detail below ${severeFocusGate.measurement?.reference ?? "configured threshold"} (${Math.round(severeFocusGate.confidence * 100)}% evidence)`
                        : "none"}
                    </p>
                  </section>
                  <ul>
                    {assessment.reasons.map((r, i) => (
                      <li key={i} className={`reason-${r.severity}`}>
                        {r.subject_index !== null
                          ? `Person ${r.subject_index + 1}: `
                          : ""}
                        {relationshipReason(
                          r,
                          assessment,
                          overview?.items ?? [],
                        )}{" "}
                        <small>
                          ({Math.round(r.confidence * 100)}% evidence)
                        </small>
                        {r.measurement && (
                          <small>
                            {" "}
                            · {Number(r.measurement.value.toPrecision(3))}{" "}
                            {r.measurement.unit}
                          </small>
                        )}
                      </li>
                    ))}
                  </ul>
                  {people?.faces.status === "available" ? (
                    <>
                      <h3>Detected people ({people.faces.value.length})</h3>
                      {people.faces.value.map((f) => (
                        <p key={f.index}>
                          Person {f.index + 1}
                          {!f.relevant ? " (too small / low confidence)" : ""} ·
                          Eyes:{" "}
                          {f.eyes.status === "available"
                            ? f.eyes.value
                            : f.eyes.status}{" "}
                          · Detail:{" "}
                          {f.sharpness.status === "available"
                            ? f.sharpness.value.toFixed(2)
                            : f.sharpness.status}{" "}
                          · Face visible: {Math.round(f.visible_fraction * 100)}
                          %
                        </p>
                      ))}
                    </>
                  ) : (
                    people && <p>Faces: {people.faces.status}</p>
                  )}
                  <details>
                    <summary>Engine and source provenance</summary>
                    <p>
                      {assessment.culling_engine_version} · Schema{" "}
                      {assessment.schema_version}
                    </p>
                    <p>
                      Analysis {assessment.source_analysis_id ?? "unavailable"}
                    </p>
                    {assessment.model_versions.map((m) => (
                      <p key={m.provider}>
                        {m.provider}: {m.model} · {m.version}
                      </p>
                    ))}
                  </details>
                </>
              )}
              {group.length > 0 && (
                <>
                  <h3>Similar photos — {group.length}</h3>
                  <p>
                    {selectedItem?.similarity &&
                      relationshipLabels[selectedItem.similarity.kind]}{" "}
                    · visual similarity{" "}
                    {Math.round(
                      (selectedItem?.similarity?.similarity_score ?? 0) * 100,
                    )}
                    % (heuristic, not a probability).
                  </p>
                  <p>
                    Preferred technical frames:{" "}
                    {group
                      .filter((i) => i.similarity?.preferred)
                      .map((i) => i.asset.filename)
                      .join(", ")}
                    . Expression and intent still need photographer review.
                  </p>
                  <RelatedPhotos
                    key={selectedItem?.group_id}
                    items={group}
                    selected={selected}
                    onSelect={setSelected}
                  />
                </>
              )}
            </>
          )}
          <details className="culling-development-details">
            <summary>Development details</summary>
            <h3>Advanced view</h3>
            <label>
              Relationships{" "}
              <select
                aria-label="Duplicate filter"
                value={relationship}
                onChange={(e) => {
                  changeFilters({
                    relationship: e.target.value as RelationshipFilter,
                  });
                }}
              >
                <option value="all">All relationships</option>
                <option value="exact">Exact duplicates</option>
                <option value="near_similar">
                  Near duplicates / burst / similar
                </option>
                <option value="preferred">Preferred frames / copies</option>
                <option value="unique">Unique images</option>
              </select>
            </label>
            <div aria-label="Advanced exact rating filters">
              {starValues.map((n) => (
                <label key={n}>
                  <input
                    type="checkbox"
                    aria-label={`Filter ${n} stars`}
                    checked={ratings.includes(n)}
                    onChange={(e) => {
                      changeFilters({
                        ratings: e.target.checked
                          ? [...ratings, n]
                          : ratings.filter((rating) => rating !== n),
                      });
                    }}
                  />
                  {n}★
                </label>
              ))}
            </div>
            <label>
              <input
                type="checkbox"
                aria-label="Selected only"
                checked={selectedOnly}
                onChange={(e) => {
                  setSelectedOnly(e.target.checked);
                  setPage(0);
                }}
              />
              Selected only
            </label>
            <label>
              Sort{" "}
              <select value={sort} onChange={(e) => setSort(e.target.value)}>
                <option value="rating">Effective rating</option>
                <option value="filename">Filename</option>
              </select>
            </label>
            <h3>Saved analysis</h3>
            <p aria-label="Effective rating counts">
              {[5, 4, 3, 2, 1, 0]
                .map(
                  (n) =>
                    `${n ? `${n}★` : "Not rated"}: ${overview?.counts[n] ?? 0}`,
                )
                .join(" · ")}
            </p>
            <div aria-label="Duplicate and relationship counts">
              <p>
                Exact duplicate copies: {overview?.duplicates.exact_copies ?? 0}
                ; exact groups: {overview?.duplicates.exact_groups ?? 0}
              </p>
              <p>
                Near groups: {overview?.duplicates.near_groups ?? 0}; burst
                groups: {overview?.duplicates.burst_groups ?? 0}; similar
                groups: {overview?.duplicates.similar_groups ?? 0}
              </p>
              <p>
                Unique: {overview?.duplicates.unique_images ?? 0}; unclassified:{" "}
                {overview?.duplicates.unclassified_images ?? 0}
              </p>
            </div>
            <p>
              Closed-eye provider:{" "}
              {eyeStateAvailable ? "available" : "unavailable"}
            </p>
            {progress && (
              <div aria-label="Processing diagnostics">
                <p>
                  {progress.status}: {progress.stage} · {progress.completed}/
                  {progress.total} measured · {progress.failed} source-analysis
                  failures · {progress.cached} reused
                </p>
                <p>
                  Full-file hashing:{" "}
                  {(progress.hash_bytes / 1048576).toFixed(1)} MiB read ·{" "}
                  {progress.hash_cached} cached identities ·{" "}
                  {progress.hash_duration_ms} ms · {progress.hash_failures}{" "}
                  identity failures
                </p>
              </div>
            )}
          </details>
        </aside>
      </div>
    </section>
  );
}
function RelatedPhotos({
  items,
  selected,
  onSelect,
  canonical,
}: {
  items: CullingItem[];
  selected: string;
  onSelect: (id: string) => void;
  canonical?: string;
}) {
  const [page, setPage] = useState(0);
  const pages = Math.ceil(items.length / 24);
  const current = Math.min(page, Math.max(0, pages - 1));
  const label = canonical ? "exact copies" : "similar photos";
  return (
    <>
      <div className="culling-similar">
        {items.slice(current * 24, (current + 1) * 24).map((i) => (
          <div key={i.asset.id} title={i.asset.original_path}>
            <Thumbnail
              asset={i.asset}
              selected={selected === i.asset.id}
              onSelect={() => onSelect(i.asset.id)}
            />
            <span>
              {starText(i.effective_rating)} ·{" "}
              {canonical
                ? i.asset.id === canonical
                  ? "Preferred copy"
                  : "Exact duplicate"
                : i.similarity?.preferred
                  ? "Preferred"
                  : "Alternative"}
            </span>
          </div>
        ))}
      </div>
      {pages > 1 && (
        <div className="pagination">
          <button disabled={!current} onClick={() => setPage(current - 1)}>
            Previous {label}
          </button>
          <span>
            {current + 1} / {pages}
          </span>
          <button
            disabled={current + 1 >= pages}
            onClick={() => setPage(current + 1)}
          >
            Next {label}
          </button>
        </div>
      )}
    </>
  );
}
