import { useEffect, useRef, useState } from "react";
import type { PhotoType } from "../analysis";
import { api, errorMessage } from "../api";
import {
  DEFAULT_TRAINING_CONFIG,
  type TrainingDataset,
  type TrainingPair,
  type TrainingPreviewSet,
  type TrainingRun,
  type MatchingProgress,
  type ValidationFeedback,
} from "../training";

function filename(path: string | null | undefined) {
  if (!path) return "—";
  return path.split(/[\\/]/).at(-1) ?? path;
}

function label(value: string) {
  return value.replaceAll("_", " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export function StandaloneTrainingStudioScreen({
  onClose,
  onViewPresets,
}: {
  onClose: () => void;
  onViewPresets: () => void;
}) {
  const [datasets, setDatasets] = useState<TrainingDataset[]>([]);
  const [dataset, setDataset] = useState<TrainingDataset | null>(null);
  const [styleName, setStyleName] = useState("");
  const [photoType, setPhotoType] = useState<PhotoType>("portrait");
  const [run, setRun] = useState<TrainingRun | null>(null);
  const [matching, setMatching] = useState<TrainingDataset["alignment"]>(null);
  const [previewPair, setPreviewPair] = useState<TrainingPair | null>(null);
  const [previews, setPreviews] = useState<TrainingPreviewSet | null>(null);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [matchProgress, setMatchProgress] = useState<MatchingProgress | null>(
    null,
  );
  const [matchFailure, setMatchFailure] = useState<string | null>(null);
  const matchingRequest = useRef<string | null>(null);
  const cancelMatchRequested = useRef(false);
  const operation = useRef(false);
  const [manualBeforePath, setManualBeforePath] = useState("");
  const [manualAfterPath, setManualAfterPath] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const activeRun = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .trainingDatasets()
      .then((saved) => {
        if (cancelled) return;
        setDatasets(saved);
        setDataset(saved[0] ?? null);
        setMatching(saved[0]?.alignment ?? null);
      })
      .catch((cause) => {
        if (!cancelled) setError(errorMessage(cause));
      });
    return () => {
      cancelled = true;
      const requestId = activeRun.current;
      if (requestId) void api.cancelTraining(requestId).catch(() => {});
      if (matchingRequest.current)
        void api
          .cancelTrainingMatching(matchingRequest.current)
          .catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!dataset || !activeRun.current) return;
    const datasetId = dataset.dataset_id;
    const timer = window.setInterval(() => {
      void api
        .trainingProgress(datasetId)
        .then((progress) => {
          if (progress?.run_id === activeRun.current) setRun(progress);
        })
        .catch(() => {});
    }, 250);
    return () => window.clearInterval(timer);
  }, [dataset, busy]);

  const beforeCount = dataset?.before_files.length ?? 0;
  const afterCount = dataset?.after_files.length ?? 0;
  const readyPairs =
    dataset?.pairs.filter(
      (pair) =>
        !pair.excluded &&
        dataset.dataset_fingerprint !== null &&
        (pair.validation.status === "ready" ||
          pair.validation.status === "needs_review"),
    ).length ?? 0;
  const pairedBefore = new Set(
    dataset?.pairs.map((pair) => pair.source_path) ?? [],
  );
  const pairedAfter = new Set(
    dataset?.pairs.map((pair) => pair.reference_path) ?? [],
  );
  const availableBefore =
    dataset?.before_files.filter((path) => !pairedBefore.has(path)) ?? [];
  const availableAfter =
    dataset?.after_files.filter((path) => !pairedAfter.has(path)) ?? [];

  function update(next: TrainingDataset) {
    setDataset(next);
    setMatching(next.alignment);
    setDatasets((current) => [
      next,
      ...current.filter((item) => item.dataset_id !== next.dataset_id),
    ]);
  }

  async function perform(name: string, action: () => Promise<void>) {
    if (operation.current) return;
    operation.current = true;
    setBusy(name);
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      operation.current = false;
      setBusy(null);
    }
  }

  async function createDataset() {
    await perform("create", async () => {
      const next = await api.createTrainingDataset(styleName, photoType);
      update(next);
      setStyleName("");
    });
  }

  async function addFiles(role: "before" | "after") {
    if (!dataset) return;
    await perform(role, async () => {
      const paths = await api.chooseTrainingFiles(
        role === "before"
          ? "Choose original before images"
          : "Choose finished after edits",
        role,
      );
      if (!paths.length) return;
      update(
        role === "before"
          ? await api.addTrainingBeforeFiles(dataset.dataset_id, paths)
          : await api.addTrainingAfterFiles(dataset.dataset_id, paths),
      );
    });
  }

  async function addFolder(role: "before" | "after") {
    if (!dataset) return;
    await perform(`${role}-folder`, async () => {
      const folder = await api.chooseFolder(
        role === "before"
          ? "Choose a folder of original photos"
          : "Choose a folder of finished edits",
      );
      if (!folder) return;
      update(
        role === "before"
          ? await api.addTrainingBeforeFolder(dataset.dataset_id, folder)
          : await api.addTrainingAfterFolder(dataset.dataset_id, folder),
      );
    });
  }

  async function matchAndValidate() {
    if (!dataset || operation.current || matchingRequest.current) return;
    const requestId = crypto.randomUUID();
    matchingRequest.current = requestId;
    cancelMatchRequested.current = false;
    setMatchFailure(null);
    setMatchProgress({
      request_id: requestId,
      dataset_id: dataset.dataset_id,
      status: "running",
      stage: "scanning_before",
      processed: 0,
      total: beforeCount,
      error: null,
    });
    let done = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      try {
        if (cancelMatchRequested.current)
          await api.cancelTrainingMatching(requestId);
        const progress = await api.trainingMatchingProgress(requestId);
        if (!done && progress?.request_id === matchingRequest.current)
          setMatchProgress(progress);
      } catch {
        /* The main request reports failures; polling is best effort. */
      }
      if (!done) timer = setTimeout(() => void poll(), 250);
    };
    await perform("validate", async () => {
      void poll();
      try {
        const result = await api.matchValidateTrainingDataset(
          dataset.dataset_id,
          requestId,
        );
        update(result);
        setReviewOpen(false);
        setMatchProgress(null);
      } catch (cause) {
        if (cancelMatchRequested.current)
          setNotice("Matching cancelled. Previous dataset preserved.");
        else setMatchFailure(errorMessage(cause));
        setMatchProgress(null);
      } finally {
        done = true;
        clearTimeout(timer);
        matchingRequest.current = null;
      }
    });
  }

  async function train() {
    if (!dataset || operation.current) return;
    const requestId = crypto.randomUUID();
    activeRun.current = requestId;
    setRun({
      schema_version: 1,
      run_id: requestId,
      dataset_id: dataset.dataset_id,
      style_id: null,
      style_name: dataset.style_name,
      style_version: null,
      status: "queued",
      stage: "queued",
      completed: 0,
      total: dataset.pairs.length,
      config: DEFAULT_TRAINING_CONFIG,
      metrics: null,
      artifact_path: null,
      started_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      duration_ms: 0,
      error: null,
    });
    await perform("train", async () => {
      const completed = await api.runTraining(
        dataset.dataset_id,
        requestId,
        DEFAULT_TRAINING_CONFIG,
      );
      setRun(completed);
      update(await api.trainingDataset(dataset.dataset_id));
    });
    activeRun.current = null;
  }

  async function inspect(pair: TrainingPair) {
    if (!dataset) return;
    setPreviewPair(pair);
    setPreviews(null);
    await perform("preview", async () => {
      setPreviews(
        await api.trainingPairPreviews(dataset.dataset_id, pair.pair_id),
      );
    });
  }

  async function feedback(pairId: string, value: ValidationFeedback) {
    if (!dataset) return;
    await perform("feedback", async () => {
      update(await api.trainingFeedback(dataset.dataset_id, pairId, value));
    });
  }

  async function toggleExcluded(pair: TrainingPair) {
    if (!dataset) return;
    await perform("exclude", async () => {
      update(
        await api.setTrainingPairExcluded(
          dataset.dataset_id,
          pair.pair_id,
          !pair.excluded,
        ),
      );
    });
  }

  return (
    <section className="screen training-studio">
      <header className="job-header">
        <div>
          <div className="eyebrow">LOCAL AUTHORING / REUSABLE STYLE</div>
          <h1>Training Studio</h1>
          <p className="subtitle">
            Teach a reusable adaptive preset from original photographs and
            finished edits. Training is independent from Jobs, Culling, and
            Editing.
          </p>
        </div>
        <button onClick={onClose}>Back to home</button>
      </header>
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      {notice && (
        <div className="notice" role="status">
          {notice}
        </div>
      )}

      <div className="training-layout">
        <aside className="training-sidebar panel">
          <h2>Create Style</h2>
          <label>
            Style name
            <input
              value={styleName}
              onChange={(event) => setStyleName(event.target.value)}
              placeholder="My Portrait Style"
            />
          </label>
          <label>
            Photo type
            <select
              value={photoType}
              onChange={(event) =>
                setPhotoType(event.target.value as PhotoType)
              }
            >
              <option value="portrait">Portrait</option>
              <option value="real_estate">Real Estate</option>
              <option value="landscape">Landscape</option>
            </select>
          </label>
          <button
            className="primary"
            disabled={!styleName.trim() || busy !== null}
            onClick={() => void createDataset()}
          >
            Create Style
          </button>
          {datasets.length > 0 && (
            <label>
              Saved training datasets
              <select
                aria-label="Saved training datasets"
                disabled={busy !== null}
                value={dataset?.dataset_id ?? ""}
                onChange={(event) => {
                  const next =
                    datasets.find(
                      (item) => item.dataset_id === event.target.value,
                    ) ?? null;
                  setDataset(next);
                  setMatching(next?.alignment ?? null);
                  setRun(null);
                  setMatchProgress(null);
                  setMatchFailure(null);
                  setPreviews(null);
                  setManualBeforePath("");
                  setManualAfterPath("");
                }}
              >
                {datasets.map((item) => (
                  <option key={item.dataset_id} value={item.dataset_id}>
                    {item.style_name}
                  </option>
                ))}
              </select>
            </label>
          )}
          <button onClick={onViewPresets}>View Presets</button>
        </aside>

        <div className="training-main">
          {!dataset ? (
            <div className="empty-state">
              <h2>Create a style to begin</h2>
              <p>Select before and after files; no Job is required.</p>
            </div>
          ) : (
            <>
              <section className="panel training-summary">
                <div className="section-heading">
                  <div>
                    <span className="eyebrow">TRAINING DATASET</span>
                    <h2>{dataset.style_name}</h2>
                    <p className="muted">
                      {label(dataset.photo_type)} · {readyPairs} ready pairs
                    </p>
                  </div>
                  <button
                    className="primary"
                    disabled={readyPairs === 0 || busy !== null}
                    onClick={() => void train()}
                  >
                    Train Style
                  </button>
                </div>
                <div className="training-input-columns">
                  <div className="training-input-group">
                    <h3>
                      Before <span>{beforeCount} images</span>
                    </h3>
                    <button
                      disabled={busy !== null}
                      onClick={() => void addFiles("before")}
                    >
                      Add Before Image
                    </button>
                    <button
                      disabled={busy !== null}
                      onClick={() => void addFolder("before")}
                    >
                      Add Before Folder
                    </button>
                    {beforeCount > 0 && (
                      <p className="muted">
                        {filename(dataset.before_files[0])} →{" "}
                        {filename(dataset.before_files.at(-1))}
                      </p>
                    )}
                  </div>
                  <div className="training-input-group">
                    <h3>
                      After <span>{afterCount} images</span>
                    </h3>
                    <button
                      disabled={busy !== null}
                      onClick={() => void addFiles("after")}
                    >
                      Add After Image
                    </button>
                    <button
                      disabled={busy !== null}
                      onClick={() => void addFolder("after")}
                    >
                      Add After Folder
                    </button>
                    {afterCount > 0 && (
                      <p className="muted">
                        {filename(dataset.after_files[0])} →{" "}
                        {filename(dataset.after_files.at(-1))}
                      </p>
                    )}
                  </div>
                </div>
                <button
                  className="primary wide-action"
                  disabled={!beforeCount || !afterCount || busy !== null}
                  onClick={() => void matchAndValidate()}
                >
                  {busy === "validate"
                    ? "Matching dataset…"
                    : "Match / Validate Dataset"}
                </button>
                {matchProgress && (
                  <div className="panel" role="status" aria-live="polite">
                    <h3>Matching dataset</h3>
                    <p>{label(matchProgress.stage)}</p>
                    <progress
                      aria-label="Dataset matching progress"
                      value={matchProgress.processed}
                      max={Math.max(1, matchProgress.total)}
                    />
                    <p>
                      {matchProgress.processed} / {matchProgress.total} ·{" "}
                      {matchProgress.total
                        ? Math.floor(
                            (100 * matchProgress.processed) /
                              matchProgress.total,
                          )
                        : 0}
                      % of this stage
                    </p>
                    <button
                      onClick={() => {
                        cancelMatchRequested.current = true;
                        const id = matchingRequest.current;
                        if (id)
                          void api.cancelTrainingMatching(id).catch(() => {});
                      }}
                    >
                      Cancel matching
                    </button>
                  </div>
                )}
                {matchFailure && (
                  <div className="error" role="alert">
                    <h3>Dataset matching failed</h3>
                    <p>{matchFailure}</p>
                    <button
                      disabled={busy !== null}
                      onClick={() => void matchAndValidate()}
                    >
                      Try Again
                    </button>
                  </div>
                )}
                {matching && !matchProgress && !matchFailure && (
                  <div className="panel" role="status">
                    <h3>
                      {readyPairs > 0
                        ? "Dataset Ready"
                        : "Dataset needs review"}
                    </h3>
                    <p>
                      {matching.matched_count} matched ·{" "}
                      {matching.ambiguous_count} ambiguous ·{" "}
                      {matching.unmatched_before.length +
                        matching.unmatched_after.length}{" "}
                      unmatched
                    </p>
                    <p>
                      {readyPairs > 0
                        ? "This dataset is ready for training. Review any flagged pairs below."
                        : "Review the unmatched or rejected images before training."}
                    </p>
                    <button
                      className="primary"
                      disabled={!readyPairs || busy !== null}
                      onClick={() => void train()}
                    >
                      Train Style
                    </button>
                  </div>
                )}
                {dataset.warnings.map((warning) => (
                  <div className="notice" key={warning}>
                    {warning}
                  </div>
                ))}
              </section>

              {matching && (
                <section className="panel training-alignment">
                  <div className="section-heading">
                    <h2>Dataset summary</h2>
                    <span>{matching.matched_count} matched</span>
                  </div>
                  <div className="metric-grid">
                    <div>
                      <span>Before</span>
                      <strong>{matching.before_count}</strong>
                    </div>
                    <div>
                      <span>After</span>
                      <strong>{matching.after_count}</strong>
                    </div>
                    <div>
                      <span>Ambiguous</span>
                      <strong>{matching.ambiguous_count}</strong>
                    </div>
                    <div>
                      <span>Unmatched before</span>
                      <strong>{matching.unmatched_before.length}</strong>
                    </div>
                    <div>
                      <span>Unmatched after</span>
                      <strong>{matching.unmatched_after.length}</strong>
                    </div>
                    <div>
                      <span>Order fallback</span>
                      <strong>
                        {matching.order_fallback_used ? "Review" : "No"}
                      </strong>
                    </div>
                  </div>
                  <p>
                    Start: {filename(matching.first_before)} →{" "}
                    {filename(matching.first_after)}{" "}
                    {matching.start_aligned ? "✓ aligned" : "· check"}
                  </p>
                  <p>
                    End: {filename(matching.last_before)} →{" "}
                    {filename(matching.last_after)}{" "}
                    {matching.end_aligned ? "✓ aligned" : "· check"}
                  </p>
                  {matching.diagnostics.map((item) => (
                    <div className="notice" key={item}>
                      {item}
                    </div>
                  ))}
                </section>
              )}

              {run && (
                <section className="panel training-run" aria-live="polite">
                  <div className="section-heading">
                    <h2>
                      {run.status === "complete"
                        ? "Training Complete"
                        : `Training · ${label(run.stage)}`}
                    </h2>
                    <span>
                      {run.completed}/{run.total}
                    </span>
                  </div>
                  <progress
                    value={run.completed}
                    max={Math.max(1, run.total)}
                  />
                  {activeRun.current && (
                    <button
                      onClick={() => {
                        const id = activeRun.current;
                        if (id) void api.cancelTraining(id);
                      }}
                    >
                      Cancel training
                    </button>
                  )}
                  {run.error && <div className="error">{run.error}</div>}
                  {run.status === "complete" && (
                    <>
                      <p>
                        <strong>{run.style_name}</strong> · version{" "}
                        {run.style_version}
                      </p>
                      <p className="notice">
                        Preset saved successfully and is available in Presets.
                      </p>
                      <p>
                        Training pairs:{" "}
                        {
                          dataset.pairs.filter((p) => p.split === "train")
                            .length
                        }{" "}
                        · Validation pairs:{" "}
                        {
                          dataset.pairs.filter((p) => p.split === "validation")
                            .length
                        }
                      </p>
                      <div className="training-actions">
                        <button className="primary" onClick={onViewPresets}>
                          View Preset
                        </button>
                        <button onClick={() => setRun(null)}>
                          Train New Version
                        </button>
                      </div>
                    </>
                  )}
                  {run.metrics && (
                    <div className="metric-grid">
                      <div>
                        <span>Validation recipe error</span>
                        <strong>
                          {run.metrics.validation.mean_recipe_mae.toFixed(4)}
                        </strong>
                      </div>
                      <div>
                        <span>Model rendered loss</span>
                        <strong>
                          {run.metrics.validation.rendered_loss?.toFixed(4) ??
                            "—"}
                        </strong>
                      </div>
                      <div>
                        <span>Mean baseline loss</span>
                        <strong>
                          {run.metrics.mean_baseline.rendered_loss?.toFixed(
                            4,
                          ) ?? "—"}
                        </strong>
                      </div>
                      <div>
                        <span>Beats mean baseline</span>
                        <strong>
                          {run.metrics.beats_mean_baseline ? "Yes" : "No"}
                        </strong>
                      </div>
                    </div>
                  )}
                  {run.metrics?.overfitting_warning && (
                    <div className="notice">
                      {run.metrics.overfitting_warning}
                    </div>
                  )}
                  {run.metrics?.warnings.map((warning) => (
                    <div className="notice" key={warning}>
                      {warning}
                    </div>
                  ))}
                </section>
              )}

              <section className="training-pairs">
                <div className="section-heading">
                  <div>
                    <span className="eyebrow">MATCH REVIEW</span>
                    <h2>Review Matches</h2>
                    <p className="muted">
                      Inspect each before/after pair, exclude unsafe matches, or
                      leave validation feedback.
                    </p>
                  </div>
                  <button onClick={() => setReviewOpen((open) => !open)}>
                    {reviewOpen ? "Hide matches" : "Review all matches"}
                  </button>
                </div>
                {availableBefore.length > 0 && availableAfter.length > 0 && (
                  <div className="training-manual-pair panel">
                    <strong>Correct a match manually</strong>
                    <p className="muted">
                      Use this when a filename is ambiguous or an ordered
                      candidate needs a different pairing. Structural validation
                      still decides whether it is ready.
                    </p>
                    <div className="training-input-columns">
                      <label>
                        Before
                        <select
                          aria-label="Manual before file"
                          value={manualBeforePath}
                          onChange={(event) =>
                            setManualBeforePath(event.target.value)
                          }
                        >
                          <option value="">Choose a before file</option>
                          {availableBefore.map((path) => (
                            <option key={path} value={path}>
                              {filename(path)}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        After
                        <select
                          aria-label="Manual after file"
                          value={manualAfterPath}
                          onChange={(event) =>
                            setManualAfterPath(event.target.value)
                          }
                        >
                          <option value="">Choose an after file</option>
                          {availableAfter.map((path) => (
                            <option key={path} value={path}>
                              {filename(path)}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>
                    <button
                      disabled={
                        !manualBeforePath || !manualAfterPath || busy !== null
                      }
                      onClick={() =>
                        void perform("manual-pair", async () => {
                          update(
                            await api.addTrainingPathPair(
                              dataset.dataset_id,
                              manualBeforePath,
                              manualAfterPath,
                            ),
                          );
                          setManualBeforePath("");
                          setManualAfterPath("");
                        })
                      }
                    >
                      Add manual pair
                    </button>
                  </div>
                )}
                {dataset.pairs
                  .filter(
                    (pair) => reviewOpen || pair.validation.status !== "ready",
                  )
                  .map((pair) => (
                    <article
                      className={`panel training-pair ${pair.excluded ? "excluded" : ""}`}
                      key={pair.pair_id}
                    >
                      <div className="section-heading">
                        <div>
                          <h3>{filename(pair.source_path)}</h3>
                          <p className="muted">
                            {filename(pair.reference_path)}
                          </p>
                        </div>
                        <span className={`pill ${pair.validation.status}`}>
                          {label(pair.validation.status)}
                        </span>
                      </div>
                      <dl>
                        <div>
                          <dt>Before</dt>
                          <dd>{filename(pair.source_path)}</dd>
                        </div>
                        <div>
                          <dt>After</dt>
                          <dd>{filename(pair.reference_path)}</dd>
                        </div>
                        <div>
                          <dt>Geometry</dt>
                          <dd>{label(pair.validation.geometry)}</dd>
                        </div>
                        <div>
                          <dt>Split</dt>
                          <dd>{label(pair.split)}</dd>
                        </div>
                        <div>
                          <dt>Target fit</dt>
                          <dd>
                            {pair.target
                              ? label(pair.target.confidence)
                              : "Not estimated"}
                          </dd>
                        </div>
                      </dl>
                      {pair.validation.diagnostics
                        .concat(pair.diagnostics)
                        .map((item) => (
                          <p className="muted" key={item}>
                            {item}
                          </p>
                        ))}
                      <div className="training-actions">
                        <button
                          disabled={busy !== null}
                          onClick={() => void inspect(pair)}
                        >
                          Review pair
                        </button>
                        <button
                          disabled={busy !== null}
                          onClick={() => void toggleExcluded(pair)}
                        >
                          {pair.excluded ? "Include pair" : "Exclude pair"}
                        </button>
                      </div>
                      {pair.split === "validation" && (
                        <div
                          className="feedback-actions"
                          aria-label="Validation feedback"
                        >
                          {(
                            ["accept", "needs_adjustment", "reject"] as const
                          ).map((value) => (
                            <button
                              className={
                                pair.feedback === value ? "active" : ""
                              }
                              key={value}
                              disabled={busy !== null}
                              onClick={() => void feedback(pair.pair_id, value)}
                            >
                              {label(value)}
                            </button>
                          ))}
                        </div>
                      )}
                    </article>
                  ))}
              </section>
            </>
          )}
        </div>
      </div>

      {previewPair && (
        <div
          className="training-preview"
          role="dialog"
          aria-modal="true"
          aria-label="Training pair visual review"
        >
          <div className="panel">
            <div className="section-heading">
              <h2>{filename(previewPair.source_path)}</h2>
              <button
                onClick={() => {
                  setPreviewPair(null);
                  setPreviews(null);
                }}
              >
                Close review
              </button>
            </div>
            {!previews ? (
              <div className="empty-state">Preparing comparable previews…</div>
            ) : (
              <div className="comparison-grid">
                <figure>
                  <figcaption>Source</figcaption>
                  <img src={previews.source_data} alt="Unedited source" />
                </figure>
                {previews.ai_data && (
                  <figure>
                    <figcaption>AI edit</figcaption>
                    <img src={previews.ai_data} alt="Applied AI edit" />
                  </figure>
                )}
                {previews.target_data && (
                  <figure>
                    <figcaption>Target recipe render</figcaption>
                    <img
                      src={previews.target_data}
                      alt="Estimated target recipe render"
                    />
                  </figure>
                )}
                <figure>
                  <figcaption>Reference</figcaption>
                  <img
                    src={previews.reference_data}
                    alt="Photographer reference edit"
                  />
                </figure>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
