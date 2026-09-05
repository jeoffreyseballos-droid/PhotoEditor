import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import type { Asset, DevelopmentState, RenderAdjustments } from "../types";
import { neutralAdjustments } from "../toolkit";
import type { MaskDiagnostic } from "../toolkit";
import {
  BasicControls,
  NumberControl,
  ToolkitControls,
} from "./ToolkitControls";

import { recipeControls, updateRecipeControls } from "../recipe";
import type { EditRecipe, RecipeState, RevisionReason } from "../recipe";
import { RecipeInspector } from "./RecipeInspector";
import { AnalysisInspector } from "./AnalysisInspector";
export function DevelopmentPanel({ asset }: { asset: Asset }) {
  const [recipe, setRecipe] = useState<EditRecipe | null>(null);
  const recipeRef = useRef<EditRecipe | null>(null);
  const persisted = useRef<RecipeState | null>(null);
  const a = useMemo(
    () => (recipe ? recipeControls(recipe) : neutralAdjustments()),
    [recipe],
  );
  const [loaded, setLoaded] = useState(false);
  const [state, setState] = useState<DevelopmentState | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [edited, setEdited] = useState<string | null>(null);
  const [before, setBefore] = useState(false);
  const [overlay, setOverlay] = useState<string | null>(null);
  const [mask, setMask] = useState<MaskDiagnostic | undefined>();
  const [renderedParams, setRenderedParams] = useState("");
  const [busy, setBusy] = useState(false);
  const [autoPreview, setAutoPreview] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("Loading saved adjustments…");
  const [format, setFormat] = useState<"jpeg" | "tiff">("jpeg");
  const [quality, setQuality] = useState(95);
  const requestId = useRef<string | null>(null);
  const cancelRequested = useRef(false);
  const mounted = useRef(true);
  const saveQueue = useRef<Promise<unknown>>(Promise.resolve());
  const lastSaved = useRef("");
  const unsupported = asset.file_type === "heic" || asset.file_type === "heif";
  useEffect(() => {
    mounted.current = true;
    void api
      .development(asset.job_id, asset.id)
      .then((s) => {
        if (mounted.current) {
          if (!s.recipe_state)
            throw new Error(
              "This desktop version did not return an edit recipe.",
            );
          persisted.current = s.recipe_state;
          recipeRef.current = s.recipe_state.recipe;
          setRecipe(s.recipe_state.recipe);
          if (s.recipe_state.error) setError(s.recipe_state.error.message);
          setMask(s.diagnostics?.mask);
          lastSaved.current = JSON.stringify(s.recipe_state.recipe);
          setState(s);
          setLoaded(true);
          setStatus(
            "Adjustments saved locally. Render a preview to inspect edits.",
          );
        }
      })
      .catch((e) => {
        if (mounted.current) setError(errorMessage(e));
      });
    void api
      .thumbnail(asset.job_id, asset.id)
      .then((s) => {
        if (mounted.current) setSource(s);
      })
      .catch(() => {});
    return () => {
      mounted.current = false;
      if (requestId.current)
        void api.cancelDevelopment(requestId.current).catch(() => {});
    };
  }, [asset.job_id, asset.id]);
  const save = useCallback(
    (value: EditRecipe, reason: RevisionReason | null = null) => {
      const json = JSON.stringify(value);
      const next = saveQueue.current
        .catch(() => {})
        .then(async () => {
          if (!reason && json === lastSaved.current) return;
          if (!persisted.current) throw new Error("Recipe is not loaded.");
          const result = await api.saveRecipe(
            asset.job_id,
            asset.id,
            value,
            persisted.current.generation,
            reason,
          );
          if (!result.recipe_state) throw new Error("Missing saved recipe.");
          persisted.current = result.recipe_state;
          lastSaved.current = json;
          if (mounted.current && recipeRef.current === value) {
            recipeRef.current = result.recipe_state.recipe;
            setRecipe(result.recipe_state.recipe);
            lastSaved.current = JSON.stringify(result.recipe_state.recipe);
          }
          if (mounted.current) {
            setState(result);
            setStatus("Adjustments saved locally.");
          }
        });
      saveQueue.current = next;
      void next.catch((e) => {
        if (mounted.current) setError(errorMessage(e));
      });
      return next;
    },
    [asset.job_id, asset.id],
  );
  // Saving tiny parameters is immediate and serial; it never starts image decoding.
  function change(
    next: RenderAdjustments,
    reason: RevisionReason | null = null,
  ) {
    if (!recipeRef.current) return;
    const updated = updateRecipeControls(recipeRef.current, next);
    recipeRef.current = updated;
    setRecipe(updated);
    setOverlay(null);
    setError("");
    void save(updated, reason);
  }
  const render = useCallback(
    async (preview: boolean, commit = true) => {
      setOverlay(null);
      cancelRequested.current = false;
      setBusy(true);
      setError("");
      setStatus(
        preview
          ? "Developing reduced preview…"
          : "Rendering full-resolution source…",
      );
      const id = crypto.randomUUID();
      requestId.current = id;
      try {
        if (!recipeRef.current) return;
        await save(recipeRef.current);
        if (!mounted.current) return;
        if (cancelRequested.current) throw new Error("Rendering cancelled");
        const result = await api.renderRecipe({
          job_id: asset.job_id,
          asset_id: asset.id,
          request_id: id,
          expected_generation: persisted.current!.generation,
          commit,
          preview,
          output_format: format,
          jpeg_quality: quality,
        });
        if (!mounted.current || requestId.current !== id) return;
        setState(result.state);
        if (result.state.recipe_state) {
          persisted.current = result.state.recipe_state;
          recipeRef.current = result.state.recipe_state.recipe;
          setRecipe(result.state.recipe_state.recipe);
          lastSaved.current = JSON.stringify(result.state.recipe_state.recipe);
        }
        if (result.state.diagnostics && a.local_layers.some((l) => l.enabled))
          setMask(result.state.diagnostics.mask);
        if (preview) {
          setEdited(result.preview_data);
          setRenderedParams(
            JSON.stringify(
              result.state.recipe_state
                ? recipeControls(result.state.recipe_state.recipe)
                : a,
            ),
          );
          setBefore(false);
        }
        setStatus(
          `${preview ? "Preview ready" : "Export written"} · ${result.width} × ${result.height}${preview ? "" : ` · ${result.state.export_path ?? ""}`}`,
        );
      } catch (e) {
        if (mounted.current) setError(errorMessage(e));
      } finally {
        if (requestId.current === id) requestId.current = null;
        if (mounted.current) setBusy(false);
      }
    },
    [a, asset.job_id, asset.id, format, quality, save],
  );
  useEffect(() => {
    if (
      !autoPreview ||
      !loaded ||
      busy ||
      unsupported ||
      error ||
      renderedParams === JSON.stringify(a)
    )
      return;
    const timer = window.setTimeout(() => void render(true, false), 350);
    return () => window.clearTimeout(timer);
  }, [
    a,
    autoPreview,
    loaded,
    busy,
    unsupported,
    error,
    renderedParams,
    render,
  ]);
  const dirty = edited && renderedParams !== JSON.stringify(a);
  async function showMask(generate: boolean, layerId: string | null) {
    if (!generate && (!edited || dirty)) {
      setError("Update Preview before showing an aligned mask overlay.");
      return;
    }
    const id = crypto.randomUUID();
    requestId.current = id;
    cancelRequested.current = false;
    setBusy(true);
    setOverlay(null);
    setError("");
    setStatus(
      generate
        ? "Generating local CPU portrait mask…"
        : "Loading aligned mask overlay…",
    );
    try {
      if (!recipeRef.current) return;
      await save(recipeRef.current);
      if (!mounted.current || cancelRequested.current) return;
      const result = await api.recipeMask({
        job_id: asset.job_id,
        asset_id: asset.id,
        request_id: id,
        expected_generation: persisted.current!.generation,
        layer_id: layerId,
        generate,
      });
      if (!mounted.current || requestId.current !== id) return;
      setMask(result.diagnostic);
      setOverlay(result.overlay_data);
      setBefore(false);
      if (generate && a.local_layers.some((l) => l.enabled))
        setRenderedParams("");
      setStatus(
        `Mask ${result.diagnostic.status}. ${generate ? "Update Preview to apply local adjustments." : "Overlay is not exported."}`,
      );
    } catch (e) {
      if (mounted.current) setError(errorMessage(e));
    } finally {
      if (requestId.current === id) requestId.current = null;
      if (mounted.current) setBusy(false);
    }
  }
  async function recipeAction(
    action: "snapshot" | "export" | "import" | "restore",
    revisionId?: string,
  ) {
    if (!recipeRef.current || !persisted.current) return;
    setBusy(true);
    setError("");
    try {
      // Corrupt storage is recoverable by import/restore; never save its fallback implicitly.
      if (!persisted.current.error) await save(recipeRef.current);
      let result: DevelopmentState | null = null;
      if (action === "snapshot") {
        await save(recipeRef.current, "snapshot");
        setStatus("Recipe snapshot saved.");
      } else if (action === "export") {
        const path = await api.exportRecipe(asset.job_id, asset.id);
        setStatus(`Recipe JSON written · ${path}`);
      } else if (action === "import") {
        const path = await api.chooseRecipe();
        if (path)
          result = await api.importRecipe(
            asset.job_id,
            asset.id,
            path,
            persisted.current.generation,
          );
      } else if (revisionId)
        result = await api.restoreRecipe(
          asset.job_id,
          asset.id,
          revisionId,
          persisted.current.generation,
        );
      if (result?.recipe_state) {
        persisted.current = result.recipe_state;
        recipeRef.current = result.recipe_state.recipe;
        setRecipe(result.recipe_state.recipe);
        lastSaved.current = JSON.stringify(result.recipe_state.recipe);
        setState(result);
        setOverlay(null);
        setRenderedParams("");
        setStatus(
          `Recipe ${action === "restore" ? "restored" : "imported"}. Update Preview to inspect it. Local masks use this photo only.`,
        );
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      className="development-panel"
      aria-label={`Develop ${asset.filename}`}
    >
      <header>
        <h2>Develop · {asset.filename}</h2>
        <p>CPU · linear RGB · originals remain unchanged</p>
      </header>
      {unsupported && (
        <p className="notice">
          HEIC/HEIF editing is not available. Discovery and embedded previews
          remain supported.
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="development-layout">
        <div>
          <div className="development-view">
            {(before ? source : edited) ? (
              <div className="development-image-stack">
                <img
                  src={(before ? source : edited)!}
                  alt={
                    before
                      ? "Original embedded/source preview"
                      : "Rendered edit preview"
                  }
                />
                {overlay && !before && !dirty && (
                  <img
                    className="mask-overlay"
                    src={overlay}
                    alt="Local mask overlay"
                  />
                )}
              </div>
            ) : (
              <p>
                {before
                  ? "No embedded/source preview available"
                  : "Update Preview to develop this photo"}
              </p>
            )}
          </div>
          <button onClick={() => setBefore((v) => !v)}>
            {before ? "Show edited preview" : "Show original/source preview"}
          </button>
          <p className="muted">
            {before
              ? "Original: camera/source thumbnail, not a neutral engine render."
              : "Edited: reduced development preview; export for full-resolution inspection."}
          </p>
          {dirty && (
            <p className="notice">
              Preview is out of date. Update Preview to see these adjustments.
            </p>
          )}
          <p role="status">{status}</p>
          {state?.export_path && (
            <p className="export-path">Last export: {state.export_path}</p>
          )}
          {state?.warnings.map((w, i) => (
            <p className="muted" key={i}>
              {w}
            </p>
          ))}
        </div>
        <div>
          <fieldset disabled={!loaded || busy || unsupported}>
            <legend>Development adjustments</legend>
            <p className="muted">
              6500 K / tint 0 preserves camera/source WB. Positive temperature
              warms. Detail controls are conservative foundations.
            </p>
            <details open>
              <summary>Light</summary>
              <BasicControls value={a} change={(v) => change({ ...a, ...v })} />
            </details>
            <details>
              <summary>Color</summary>
              <BasicControls
                color
                value={a}
                change={(v) => change({ ...a, ...v })}
              />
            </details>
            <ToolkitControls
              a={a}
              change={change}
              reset={(value) => change(value, "reset")}
              lens={state?.diagnostics?.lens}
              mask={mask}
              onMask={(generate, id) => void showMask(generate, id)}
              hideOverlay={() => setOverlay(null)}
            />
            <details>
              <summary>Geometry</summary>
              <NumberControl
                label="Rotation (degrees)"
                value={a.rotation_degrees}
                min={-180}
                max={180}
                step={0.1}
                change={(v) => change({ ...a, rotation_degrees: v })}
              />
              <fieldset>
                <legend>Crop · normalized rotated canvas</legend>
                <div className="development-controls">
                  {(["x", "y", "width", "height"] as const).map((key) => (
                    <label key={key}>
                      {`Crop ${key}`}
                      <input
                        type="number"
                        min={0}
                        max={1}
                        step={0.01}
                        value={a.crop[key]}
                        onChange={(e) => {
                          if (Number.isFinite(e.currentTarget.valueAsNumber))
                            change({
                              ...a,
                              crop: {
                                ...a.crop,
                                [key]: e.currentTarget.valueAsNumber,
                              },
                            });
                        }}
                      />
                    </label>
                  ))}
                </div>
              </fieldset>
              <p className="muted">
                Fine rotation expands the canvas with black corners. Crop is
                applied afterwards. Crop x + width and y + height must not
                exceed 1.
              </p>
            </details>
            <button onClick={() => change(neutralAdjustments(), "reset")}>
              Reset All
            </button>
            <button
              onClick={() =>
                change(
                  {
                    ...neutralAdjustments(),
                    local_layers: a.local_layers,
                    batch_context: a.batch_context,
                  },
                  "reset",
                )
              }
            >
              Reset Global
            </button>
          </fieldset>
          {state?.recipe_state && (
            <RecipeInspector
              asset={asset}
              state={state.recipe_state}
              recipe={recipe ?? state.recipe_state.recipe}
              mask={mask}
              unresolvedMasks={state.unresolved_masks}
              busy={busy}
              onAction={(action, id) => void recipeAction(action, id)}
            />
          )}
          <AnalysisInspector asset={asset} />
          <div className="development-actions">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={autoPreview}
                disabled={!loaded || busy || unsupported}
                onChange={(e) => setAutoPreview(e.target.checked)}
              />
              Auto preview · 350 ms debounce
            </label>
            <button
              disabled={!loaded || busy || unsupported}
              onClick={() => void render(true)}
            >
              Update Preview
            </button>
            <button
              disabled={!busy}
              onClick={() => {
                cancelRequested.current = true;
                if (requestId.current)
                  void api
                    .cancelDevelopment(requestId.current)
                    .catch((e) => setError(errorMessage(e)));
              }}
            >
              Cancel render
            </button>
          </div>
          <fieldset disabled={!loaded || busy || unsupported}>
            <legend>Full-resolution export</legend>
            <label>
              Format
              <select
                value={format}
                onChange={(e) => setFormat(e.target.value as "jpeg" | "tiff")}
              >
                <option value="jpeg">JPEG · 8-bit sRGB</option>
                <option value="tiff">TIFF · 16-bit sRGB</option>
              </select>
            </label>
            {format === "jpeg" && (
              <label>
                JPEG quality
                <input
                  type="number"
                  min={1}
                  max={100}
                  value={quality}
                  onChange={(e) => setQuality(e.currentTarget.valueAsNumber)}
                />
              </label>
            )}
            <button onClick={() => void render(false)}>
              Export full resolution
            </button>
            <p className="muted">
              Writes to the job output folder. Existing exports are never
              overwritten. GPS and maker notes are omitted.
            </p>
          </fieldset>
        </div>
      </div>
    </section>
  );
}
