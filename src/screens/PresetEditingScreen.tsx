import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import type { PhotoType } from "../analysis";
import { BatchContextInspector } from "../components/BatchContextInspector";
import { DevelopmentPanel } from "../components/DevelopmentPanel";
import { StyleInferenceInspector } from "../components/StyleInferenceInspector";
import { Thumbnail } from "../components/Thumbnail";
import type { BuiltInPreset, BuiltInPresetId } from "../presets";
import type {
  StyleApplyProgress,
  StyleAssetInference,
  StyleSummary,
} from "../trained-styles";
import type { Asset } from "../types";

const POP_SUBJECT_LAYER_ID = "built-in-pop-subject-v1";

interface BatchProgress {
  stage: "masks" | "previews";
  completed: number;
  total: number;
}

interface BatchOutcome {
  cancelled: boolean;
  attention: string[];
  maskFailures: string[];
  rendered: number;
}

interface ExportProgress {
  completed: number;
  total: number;
}

interface ExportSummary {
  exported: number;
  failed: number;
}

export function PresetEditingScreen({
  jobId,
  photoType,
  initialSelectedAssetIds,
  onBack,
}: {
  jobId: string;
  photoType: PhotoType;
  initialSelectedAssetIds: string[];
  onBack: () => void;
}) {
  const [presets, setPresets] = useState<BuiltInPreset[]>([]);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [selectedIds, setSelectedIds] = useState(initialSelectedAssetIds);
  const [choice, setChoice] = useState<BuiltInPresetId | null>(null);
  const [styleChoice, setStyleChoice] = useState<string | null>(null);
  const [trainedStyles, setTrainedStyles] = useState<StyleSummary[]>([]);
  const [applied, setApplied] = useState<BuiltInPresetId | null>(null);
  const [appliedStyle, setAppliedStyle] = useState<StyleSummary | null>(null);
  const [styleInferences, setStyleInferences] = useState<StyleAssetInference[]>(
    [],
  );
  const [styleProgress, setStyleProgress] = useState<StyleApplyProgress | null>(
    null,
  );
  const [appliedCount, setAppliedCount] = useState(0);
  const [updatedCount, setUpdatedCount] = useState(0);
  const [unchangedCount, setUnchangedCount] = useState(0);
  const [unresolved, setUnresolved] = useState<string[]>([]);
  const [inspected, setInspected] = useState<Asset | null>(null);
  const [loading, setLoading] = useState(true);
  const [editingReady, setEditingReady] = useState(false);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editedPreviews, setEditedPreviews] = useState<Record<string, string>>(
    {},
  );
  const [attention, setAttention] = useState<string[]>([]);
  const [batchProgress, setBatchProgress] = useState<BatchProgress | null>(
    null,
  );
  const [renderedCount, setRenderedCount] = useState(0);
  const [exportProgress, setExportProgress] = useState<ExportProgress | null>(
    null,
  );
  const [exportSummary, setExportSummary] = useState<ExportSummary | null>(
    null,
  );
  const batchVersion = useRef(0);
  const activeRequest = useRef<string | null>(null);
  const activeStyleRequest = useRef<string | null>(null);

  const processEditedPreviews = useCallback(
    async (
      prepareSubjectMasks: boolean,
      assetIds: string[],
      initialAttention: string[] = [],
    ): Promise<BatchOutcome> => {
      const version = ++batchVersion.current;
      const failed = new Set<string>(initialAttention);
      const maskFailures = new Set<string>();
      let rendered = 0;
      const current = () => batchVersion.current === version;
      const markFailed = (assetId: string, mask: boolean) => {
        failed.add(assetId);
        if (mask) maskFailures.add(assetId);
        if (current()) setAttention([...failed]);
      };

      setEditedPreviews({});
      setAttention(initialAttention);
      setRenderedCount(0);

      if (prepareSubjectMasks) {
        setBatchProgress({
          stage: "masks",
          completed: 0,
          total: assetIds.length,
        });
        for (let index = 0; index < assetIds.length; index += 1) {
          if (!current())
            return {
              cancelled: true,
              attention: [...failed],
              maskFailures: [...maskFailures],
              rendered,
            };
          const assetId = assetIds[index];
          let requestId: string | null = null;
          try {
            const development = await api.development(jobId, assetId);
            const generation = development.recipe_state?.generation;
            if (generation === undefined)
              throw new Error(
                "This photo does not have a current edit recipe.",
              );
            requestId = crypto.randomUUID();
            activeRequest.current = requestId;
            const mask = await api.recipeMask({
              job_id: jobId,
              asset_id: assetId,
              request_id: requestId,
              expected_generation: generation,
              layer_id: POP_SUBJECT_LAYER_ID,
              generate: true,
            });
            if (mask.diagnostic.status !== "ready") markFailed(assetId, true);
          } catch {
            if (!current())
              return {
                cancelled: true,
                attention: [...failed],
                maskFailures: [...maskFailures],
                rendered,
              };
            markFailed(assetId, true);
          } finally {
            if (activeRequest.current === requestId)
              activeRequest.current = null;
            if (current())
              setBatchProgress({
                stage: "masks",
                completed: index + 1,
                total: assetIds.length,
              });
          }
        }
      }

      if (!current())
        return {
          cancelled: true,
          attention: [...failed],
          maskFailures: [...maskFailures],
          rendered,
        };
      setBatchProgress({
        stage: "previews",
        completed: 0,
        total: assetIds.length,
      });
      for (let index = 0; index < assetIds.length; index += 1) {
        if (!current())
          return {
            cancelled: true,
            attention: [...failed],
            maskFailures: [...maskFailures],
            rendered,
          };
        const assetId = assetIds[index];
        let requestId: string | null = null;
        try {
          const development = await api.development(jobId, assetId);
          const generation = development.recipe_state?.generation;
          if (generation === undefined)
            throw new Error("This photo does not have a current edit recipe.");
          requestId = crypto.randomUUID();
          activeRequest.current = requestId;
          const result = await api.renderRecipe({
            job_id: jobId,
            asset_id: assetId,
            request_id: requestId,
            expected_generation: generation,
            preview: true,
            output_format: "jpeg",
            jpeg_quality: 90,
            commit: false,
          });
          if (!result.preview_data)
            throw new Error("The renderer did not return an edited preview.");
          rendered += 1;
          if (current()) {
            setEditedPreviews((previews) => ({
              ...previews,
              [assetId]: result.preview_data!,
            }));
            setRenderedCount(rendered);
          }
        } catch {
          if (!current())
            return {
              cancelled: true,
              attention: [...failed],
              maskFailures: [...maskFailures],
              rendered,
            };
          markFailed(assetId, false);
        } finally {
          if (activeRequest.current === requestId) activeRequest.current = null;
          if (current())
            setBatchProgress({
              stage: "previews",
              completed: index + 1,
              total: assetIds.length,
            });
        }
      }
      if (current()) setBatchProgress(null);
      return {
        cancelled: !current(),
        attention: [...failed],
        maskFailures: [...maskFailures],
        rendered,
      };
    },
    [jobId],
  );

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setEditingReady(false);
    setError(null);
    void Promise.all([
      api.builtinPresets(),
      api.presetEditingState(jobId),
      api.trainedStyleState(jobId, photoType),
      api.cullingOverview(jobId, photoType),
    ])
      .then(([definitions, state, trainedState, overview]) => {
        if (cancelled) return;
        const persisted = new Set(state.selected_asset_ids);
        setPresets(definitions);
        setTrainedStyles(trainedState.styles);
        setSelectedIds(state.selected_asset_ids);
        setAssets(
          overview.items
            .filter((item) => persisted.has(item.asset.id))
            .map((item) => item.asset),
        );
        setChoice(state.applied_preset);
        setApplied(state.applied_preset);
        setAppliedStyle(trainedState.applied_style);
        setStyleInferences(trainedState.inferences);
        setAttention(trainedState.needs_review);
        setAppliedCount(
          trainedState.applied_style
            ? trainedState.applied_count
            : state.applied_count,
        );
        setUnresolved(state.unresolved_subject_masks);
        setEditingReady(true);
        if (!state.selected_asset_ids.length) {
          setError("Return to culling and select at least one photograph.");
        } else if (state.applied_preset) {
          setSaving(true);
          void processEditedPreviews(
            state.applied_preset === "pop",
            state.selected_asset_ids,
          ).then((outcome) => {
            if (cancelled || outcome.cancelled) return;
            setUnresolved(outcome.maskFailures);
            setSaving(false);
          });
        } else if (trainedState.applied_style) {
          setSaving(true);
          void processEditedPreviews(
            false,
            trainedState.selected_asset_ids,
            trainedState.needs_review,
          ).then((outcome) => {
            if (cancelled || outcome.cancelled) return;
            setSaving(false);
          });
        }
      })
      .catch((cause) => {
        if (!cancelled) setError(errorMessage(cause));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
      batchVersion.current += 1;
      const requestId = activeRequest.current;
      if (requestId) void api.cancelDevelopment(requestId).catch(() => {});
      const styleRequestId = activeStyleRequest.current;
      if (styleRequestId)
        void api.cancelTrainedStyle(styleRequestId).catch(() => {});
    };
  }, [jobId, photoType, processEditedPreviews]);

  const activePreset = useMemo(
    () => presets.find((preset) => preset.id === applied) ?? null,
    [applied, presets],
  );

  const selectedStyle = useMemo(
    () => trainedStyles.find((style) => style.style_id === styleChoice) ?? null,
    [styleChoice, trainedStyles],
  );

  const activeName = activePreset?.name ?? appliedStyle?.name ?? null;

  async function apply() {
    if (!choice || !selectedIds.length || saving) return;
    setSaving(true);
    setError(null);
    setExportSummary(null);
    try {
      const result = await api.applyBuiltInPreset(jobId, choice, selectedIds);
      setSelectedIds(result.selected_asset_ids);
      setApplied(result.preset.id);
      setAppliedStyle(null);
      setStyleInferences([]);
      setAppliedCount(result.selected_asset_ids.length);
      setUpdatedCount(result.recipes_updated);
      setUnchangedCount(result.recipes_unchanged);
      setUnresolved(result.unresolved_subject_masks);
      setInspected(null);
      const outcome = await processEditedPreviews(
        result.preset.id === "pop",
        result.selected_asset_ids,
      );
      if (!outcome.cancelled) setUnresolved(outcome.maskFailures);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }

  async function applyAIStyle() {
    if (!selectedStyle || !selectedIds.length || saving) return;
    const requestId = crypto.randomUUID();
    activeStyleRequest.current = requestId;
    setSaving(true);
    setError(null);
    setExportSummary(null);
    setStyleProgress({
      job_id: jobId,
      request_id: requestId,
      photo_type: photoType,
      style_id: selectedStyle.style_id,
      status: "queued",
      stage: `Applying ${selectedStyle.name}`,
      completed: 0,
      total: selectedIds.length,
      succeeded: 0,
      failed: 0,
      duration_ms: 0,
      error: null,
    });
    const poll = window.setInterval(() => {
      void api
        .trainedStyleProgress(jobId, photoType)
        .then((progress) => {
          if (activeStyleRequest.current === requestId && progress)
            setStyleProgress(progress);
        })
        .catch(() => {});
    }, 120);
    try {
      const result = await api.applyTrainedStyle({
        job_id: jobId,
        photo_type: photoType,
        style_id: selectedStyle.style_id,
        selected_asset_ids: selectedIds,
        request_id: requestId,
      });
      if (activeStyleRequest.current !== requestId) return;
      setSelectedIds(result.selected_asset_ids);
      setApplied(null);
      setAppliedStyle(result.style);
      setAppliedCount(result.predictions_succeeded);
      setUpdatedCount(result.recipes_updated);
      setUnchangedCount(result.recipes_unchanged);
      setStyleInferences(result.inferences);
      setAttention(result.needs_review);
      setInspected(null);
      setStyleProgress(null);
      const outcome = await processEditedPreviews(
        false,
        result.selected_asset_ids,
        result.needs_review,
      );
      if (!outcome.cancelled) setAttention(outcome.attention);
    } catch (cause) {
      if (activeStyleRequest.current === requestId)
        setError(errorMessage(cause));
    } finally {
      window.clearInterval(poll);
      if (activeStyleRequest.current === requestId) {
        activeStyleRequest.current = null;
        setStyleProgress(null);
        setSaving(false);
      }
    }
  }

  async function exportAll() {
    if (!selectedIds.length || saving || exporting) return;
    const version = ++batchVersion.current;
    const current = () => batchVersion.current === version;
    let exported = 0;
    let failed = 0;
    setExporting(true);
    setExportSummary(null);
    setError(null);
    setExportProgress({ completed: 0, total: selectedIds.length });
    try {
      const persisted = await api.presetEditingState(jobId);
      const selected = new Set(selectedIds);
      if (
        persisted.selected_asset_ids.length !== selected.size ||
        persisted.selected_asset_ids.some((assetId) => !selected.has(assetId))
      ) {
        throw new Error(
          "Editing selection changed. Return to culling or reload editing before exporting.",
        );
      }
      for (let index = 0; index < selectedIds.length; index += 1) {
        if (!current()) return;
        const assetId = selectedIds[index];
        let requestId: string | null = null;
        try {
          const development = await api.development(jobId, assetId);
          const generation = development.recipe_state?.generation;
          if (generation === undefined)
            throw new Error("This photo does not have a current edit recipe.");
          requestId = crypto.randomUUID();
          activeRequest.current = requestId;
          const result = await api.renderRecipe({
            job_id: jobId,
            asset_id: assetId,
            request_id: requestId,
            expected_generation: generation,
            preview: false,
            output_format: "jpeg",
            jpeg_quality: 95,
            commit: true,
          });
          if (!result.state.export_path)
            throw new Error("The renderer did not return an export path.");
          exported += 1;
        } catch {
          if (!current()) return;
          failed += 1;
          setAttention((currentAttention) =>
            currentAttention.includes(assetId)
              ? currentAttention
              : [...currentAttention, assetId],
          );
        } finally {
          if (activeRequest.current === requestId) activeRequest.current = null;
          if (current())
            setExportProgress({
              completed: index + 1,
              total: selectedIds.length,
            });
        }
      }
      if (current()) setExportSummary({ exported, failed });
    } catch (cause) {
      if (current()) setError(errorMessage(cause));
    } finally {
      if (current()) {
        setExportProgress(null);
        setExporting(false);
      }
    }
  }

  function cancelBatch() {
    batchVersion.current += 1;
    const requestId = activeRequest.current;
    if (requestId) void api.cancelDevelopment(requestId).catch(() => {});
    activeRequest.current = null;
    setBatchProgress(null);
    setSaving(false);
    setError("Preview preparation stopped. Completed previews are preserved.");
  }

  function cancelStyleInference() {
    const requestId = activeStyleRequest.current;
    if (requestId) void api.cancelTrainedStyle(requestId).catch(() => {});
    activeStyleRequest.current = null;
    setStyleProgress(null);
    setSaving(false);
    setError(
      "AI style stopped. Completed recipes are preserved; remaining photographs were not changed.",
    );
  }

  function cancelExport() {
    batchVersion.current += 1;
    const requestId = activeRequest.current;
    if (requestId) void api.cancelDevelopment(requestId).catch(() => {});
    activeRequest.current = null;
    setExportProgress(null);
    setExporting(false);
    setError(
      "Export stopped. Files already completed remain in the output folder.",
    );
  }

  return (
    <section className="screen preset-editing-screen">
      <div className="eyebrow">EDITING</div>
      <header className="job-header">
        <div>
          <h1>{activeName ? `Editing — ${activeName}` : "Editing"}</h1>
          <p className="subtitle">
            {selectedIds.length.toLocaleString()} photos selected
          </p>
        </div>
        <div className="header-actions">
          <button onClick={onBack}>Back to Culling</button>
          <button
            className="primary"
            disabled={
              !editingReady || !selectedIds.length || saving || exporting
            }
            onClick={() => void exportAll()}
          >
            {exporting ? "Exporting..." : "Export All"}
          </button>
        </div>
      </header>
      <details className="batch-context-shell">
        <summary>Batch Context</summary>
        <BatchContextInspector
          jobId={jobId}
          photoType={photoType}
          assets={assets}
          selectedAssetId={inspected?.id ?? assets[0]?.id ?? null}
          onSelectAsset={setInspected}
        />
      </details>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {exportProgress && (
        <div className="notice preset-progress" role="status">
          <span>
            Exporting... {exportProgress.completed.toLocaleString()} /{" "}
            {exportProgress.total.toLocaleString()}
          </span>
          <button onClick={cancelExport}>Cancel Export</button>
        </div>
      )}
      {exportSummary && (
        <p className="notice" role="status">
          Export complete · {exportSummary.exported.toLocaleString()}{" "}
          {exportSummary.exported === 1 ? "file" : "files"} exported ·{" "}
          {exportSummary.failed.toLocaleString()} failed
        </p>
      )}
      {styleProgress && (
        <div className="notice preset-progress" role="status">
          <span>
            {styleProgress.stage}... {styleProgress.completed.toLocaleString()}{" "}
            / {styleProgress.total.toLocaleString()}
          </span>
          <button onClick={cancelStyleInference}>Cancel Style</button>
        </div>
      )}
      {loading ? (
        <div className="empty-state" role="status">
          Loading the saved editing selection…
        </div>
      ) : !activePreset && !appliedStyle ? (
        <div className="preset-chooser">
          <div className="section-heading">
            <h2>AI Styles</h2>
            <span className="muted">Adaptive local control predictions</span>
          </div>
          <div className="preset-grid" role="group" aria-label="AI styles">
            {trainedStyles.map((style) => (
              <button
                key={style.style_id}
                className={`preset-card ai-style-card ${styleChoice === style.style_id ? "selected" : ""}`}
                aria-pressed={styleChoice === style.style_id}
                disabled={exporting || saving}
                onClick={() => {
                  setStyleChoice(style.style_id);
                  setChoice(null);
                }}
              >
                <strong>{style.name}</strong>
                <span>{style.description}</span>
                {style.development_only && <small>Development model</small>}
              </button>
            ))}
          </div>
          {selectedStyle && (
            <div className="preset-actions ai-style-actions">
              <span className="muted">
                Produces an individual recipe for each selected photograph.
              </span>
              <button
                className="primary"
                disabled={
                  !editingReady || !selectedIds.length || saving || exporting
                }
                onClick={() => void applyAIStyle()}
              >
                {saving ? "Applying…" : "Apply AI Style"}
              </button>
            </div>
          )}
          <div className="section-heading">
            <h2>Choose a preset</h2>
            <span className="muted">Deterministic local recipes</span>
          </div>
          <div
            className="preset-grid"
            role="group"
            aria-label="Built-in presets"
          >
            {presets.map((preset) => (
              <button
                key={preset.id}
                className={`preset-card ${choice === preset.id ? "selected" : ""}`}
                aria-pressed={choice === preset.id}
                disabled={exporting}
                onClick={() => {
                  setChoice(preset.id);
                  setStyleChoice(null);
                }}
              >
                <strong>{preset.name}</strong>
                <span>{preset.description}</span>
              </button>
            ))}
          </div>
          <div className="preset-actions">
            <button onClick={onBack}>Back to Culling</button>
            <button
              className="primary"
              disabled={
                !editingReady ||
                !choice ||
                !selectedIds.length ||
                saving ||
                exporting
              }
              onClick={() => void apply()}
            >
              {saving ? "Applying…" : "Apply Preset"}
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="preset-summary" role="status">
            <strong>{appliedCount.toLocaleString()} photos</strong>
            <span>
              {activePreset ? "Preset" : "AI Style"}: {activeName}
            </span>
            <span>
              {updatedCount
                ? `${updatedCount.toLocaleString()} recipes created or updated`
                : `${unchangedCount || appliedCount} recipes already saved`}
            </span>
            <button
              disabled={saving || exporting}
              onClick={() => {
                setApplied(null);
                setAppliedStyle(null);
                setStyleChoice(null);
                setStyleInferences([]);
                setInspected(null);
                setEditedPreviews({});
                setAttention([]);
                setRenderedCount(0);
                setExportSummary(null);
              }}
            >
              {activePreset ? "Change Preset" : "Change Style"}
            </button>
          </div>
          {batchProgress && (
            <div className="notice preset-progress" role="status">
              <span>
                {batchProgress.stage === "masks"
                  ? "Preparing subject masks..."
                  : "Rendering previews..."}{" "}
                {batchProgress.completed.toLocaleString()} /{" "}
                {batchProgress.total.toLocaleString()}
              </span>
              <button onClick={cancelBatch}>Cancel</button>
            </div>
          )}
          {!batchProgress && renderedCount > 0 && (
            <p className="notice" role="status">
              Edited previews ready · {renderedCount.toLocaleString()} /{" "}
              {selectedIds.length.toLocaleString()}
            </p>
          )}
          {activePreset?.id === "pop" && !saving && unresolved.length > 0 && (
            <p className="notice" role="status">
              Subject mask could not be prepared for{" "}
              {unresolved.length.toLocaleString()} photos. Those images remain
              unchanged and need attention; POP never applies its subject
              exposure globally.
            </p>
          )}
          {inspected && (
            <>
              {appliedStyle && (
                <StyleInferenceInspector
                  style={appliedStyle}
                  asset={inspected}
                  inference={
                    styleInferences.find(
                      (inference) => inference.asset_id === inspected.id,
                    ) ?? null
                  }
                />
              )}
              <DevelopmentPanel
                key={`${inspected.job_id}-${inspected.id}`}
                asset={inspected}
              />
            </>
          )}
          <div className="section-heading">
            <h2>Selected photographs</h2>
            <span className="muted">Proxy previews · originals untouched</span>
          </div>
          <div className="photo-grid editing-photo-grid">
            {assets.map((asset) => {
              const needsAttention = attention.includes(asset.id);
              return (
                <article
                  className={`editing-preview-card ${needsAttention ? "needs-attention" : ""}`}
                  key={`${asset.id}-${asset.fingerprint}`}
                >
                  <Thumbnail
                    asset={asset}
                    selected={inspected?.id === asset.id}
                    onSelect={() => setInspected(asset)}
                    sourceOverride={editedPreviews[asset.id] ?? null}
                    sourceDescription={`${activeName} edited preview for ${asset.filename}`}
                  />
                  {needsAttention && <small>Needs attention</small>}
                </article>
              );
            })}
          </div>
        </>
      )}
    </section>
  );
}
