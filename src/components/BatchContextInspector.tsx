import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { PhotoType } from "../analysis";
import { api, errorMessage } from "../api";
import type {
  AssetBatchContext,
  BatchContextProgress,
  BatchContextState,
} from "../batch-context";
import type { Asset } from "../types";

function confidence(value: number) {
  if (value >= 0.75) return "High";
  if (value >= 0.5) return "Medium";
  return "Low";
}

function relation(asset: AssetBatchContext | undefined) {
  if (!asset?.exposure_delta_from_group) return "Unavailable";
  if (asset.reference_asset_id === asset.asset_id) return "Reference";
  const delta = asset.exposure_delta_from_group.delta_ev;
  return Math.abs(delta) < 0.05
    ? "Near group median"
    : `${delta >= 0 ? "+" : ""}${delta.toFixed(2)} EV from group median`;
}

function whiteBalance(asset: AssetBatchContext | undefined) {
  const wb = asset?.wb_delta_from_group;
  if (!wb) return "Unavailable";
  if (
    Math.abs(wb.warm_cool_delta) <= 0.08 &&
    Math.abs(wb.green_magenta_delta) <= 0.08
  ) {
    return "Near group median";
  }
  return `Warm/cool ${wb.warm_cool_delta >= 0 ? "+" : ""}${wb.warm_cool_delta.toFixed(3)} · green/magenta ${wb.green_magenta_delta >= 0 ? "+" : ""}${wb.green_magenta_delta.toFixed(3)}`;
}

export function BatchContextInspector({
  jobId,
  photoType,
  assets,
  selectedAssetId,
  onSelectAsset,
}: {
  jobId: string;
  photoType: PhotoType;
  assets: Asset[];
  selectedAssetId: string | null;
  onSelectAsset: (asset: Asset) => void;
}) {
  const [state, setState] = useState<BatchContextState | null>(null);
  const [progress, setProgress] = useState<BatchContextProgress | null>(null);
  const [loading, setLoading] = useState(true);
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeRequest = useRef<string | null>(null);
  const mounted = useRef(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const loaded = await api.batchContextState(jobId, photoType);
      if (!mounted.current) return;
      setState(loaded);
      setProgress(loaded.progress);
      setError(null);
    } catch (cause) {
      if (mounted.current) setError(errorMessage(cause));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [jobId, photoType]);

  useEffect(() => {
    mounted.current = true;
    void load();
    return () => {
      mounted.current = false;
      const requestId = activeRequest.current;
      if (requestId) void api.cancelBatchContext(requestId).catch(() => {});
    };
  }, [load]);

  async function build(force: boolean) {
    if (building) return;
    const requestId = crypto.randomUUID();
    activeRequest.current = requestId;
    setBuilding(true);
    setError(null);
    setProgress({
      job_id: jobId,
      request_id: requestId,
      photo_type: photoType,
      status: "queued",
      stage: "Building batch context",
      completed: 0,
      total: state?.selected_count ?? assets.length,
      cached: false,
      duration_ms: 0,
      error: null,
    });
    const timer = window.setInterval(() => {
      void api
        .batchContextProgress(jobId, photoType)
        .then((current) => {
          if (mounted.current && current?.request_id === requestId)
            setProgress(current);
        })
        .catch(() => {});
    }, 300);
    try {
      const result = await api.runBatchContext({
        job_id: jobId,
        photo_type: photoType,
        request_id: requestId,
        force,
      });
      if (!mounted.current) return;
      setState(result);
      setProgress(result.progress);
    } catch (cause) {
      if (mounted.current) setError(errorMessage(cause));
    } finally {
      window.clearInterval(timer);
      if (activeRequest.current === requestId) activeRequest.current = null;
      if (mounted.current) setBuilding(false);
    }
  }

  async function cancel() {
    const requestId = activeRequest.current;
    if (!requestId) return;
    try {
      await api.cancelBatchContext(requestId);
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }

  const context = state?.context;
  const assetContext = context?.asset_contexts.find(
    (asset) => asset.asset_id === selectedAssetId,
  );
  const scene = context?.scene_groups.find(
    (group) => group.group_id === assetContext?.scene_group_id,
  );
  const lighting = context?.lighting_groups.find(
    (group) => group.group_id === assetContext?.lighting_group_id,
  );
  const sequence = context?.sequence_groups.find(
    (group) => group.group_id === assetContext?.sequence_group_id,
  );
  const assetsById = useMemo(
    () => new Map(assets.map((asset) => [asset.id, asset])),
    [assets],
  );

  return (
    <section className="batch-context-inspector">
      <div className="section-heading">
        <div>
          <h3>Batch Context</h3>
          <p className="muted">
            Source relationships only · does not generate or change edits
          </p>
        </div>
        <div className="header-actions">
          {context && <span className="status-pill">Cached locally</span>}
          <button
            disabled={loading || building}
            onClick={() => void build(!!context)}
          >
            {context ? "Rebuild Context" : "Build Context"}
          </button>
        </div>
      </div>
      {loading && <p role="status">Loading batch context…</p>}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {state?.stale && !context && (
        <p className="notice">
          The editing selection or source evidence changed. Build a current
          context; the older cached context remains untouched.
        </p>
      )}
      {building && progress && (
        <div className="notice preset-progress" role="status">
          <span>
            {progress.stage}… {progress.completed.toLocaleString()} /{" "}
            {progress.total.toLocaleString()}
          </span>
          <button onClick={() => void cancel()}>Cancel Context</button>
        </div>
      )}
      {!loading && !context && !building && (
        <p className="muted">
          No current context for this exact editing selection.
        </p>
      )}
      {context && (
        <>
          <div className="batch-context-counts">
            <span>
              Selected: {context.selected_asset_ids.length.toLocaleString()}
            </span>
            <span>
              Scene groups: {context.scene_groups.length.toLocaleString()}
            </span>
            <span>
              Lighting groups: {context.lighting_groups.length.toLocaleString()}
            </span>
            <span>
              Sequences: {context.sequence_groups.length.toLocaleString()}
            </span>
            <span>
              References: {context.reference_candidates.length.toLocaleString()}
            </span>
          </div>
          {assetContext ? (
            <div className="batch-context-detail">
              <dl>
                <div>
                  <dt>Selected asset</dt>
                  <dd>
                    {assetsById.get(assetContext.asset_id)?.filename ??
                      assetContext.asset_id}
                  </dd>
                </div>
                <div>
                  <dt>Scene</dt>
                  <dd>
                    {scene ? `${scene.asset_ids.length} photos` : "Unavailable"}
                  </dd>
                </div>
                <div>
                  <dt>Lighting</dt>
                  <dd>
                    {lighting
                      ? `${lighting.asset_ids.length} photos`
                      : "Unavailable"}
                  </dd>
                </div>
                <div>
                  <dt>Sequence</dt>
                  <dd>
                    {sequence ? sequence.kind.replaceAll("_", " ") : "None"}
                  </dd>
                </div>
                <div>
                  <dt>Reference</dt>
                  <dd>
                    {assetContext.reference_asset_id
                      ? (assetsById.get(assetContext.reference_asset_id)
                          ?.filename ?? assetContext.reference_asset_id)
                      : "No reliable reference"}
                  </dd>
                </div>
                <div>
                  <dt>Exposure relationship</dt>
                  <dd>{relation(assetContext)}</dd>
                </div>
                <div>
                  <dt>WB relationship</dt>
                  <dd>{whiteBalance(assetContext)}</dd>
                </div>
                <div>
                  <dt>Confidence</dt>
                  <dd>{confidence(assetContext.group_confidence)}</dd>
                </div>
              </dl>
              {scene && (
                <div>
                  <h4>Scene group</h4>
                  <div
                    className="batch-context-group"
                    aria-label="Scene group photographs"
                  >
                    {scene.asset_ids.map((assetId) => {
                      const asset = assetsById.get(assetId);
                      const reference =
                        scene.reference_candidate_ids.includes(assetId);
                      return (
                        <button
                          key={assetId}
                          disabled={!asset}
                          className={
                            assetId === selectedAssetId ? "selected" : ""
                          }
                          onClick={() => asset && onSelectAsset(asset)}
                        >
                          {asset?.filename ?? assetId}
                          {reference && <small>REFERENCE</small>}
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <p className="muted">
              Select a photograph to inspect its group context.
            </p>
          )}
          <details>
            <summary>Batch diagnostics</summary>
            <p>
              Available {context.diagnostics.available_assets} · Partial{" "}
              {context.diagnostics.partial_assets} · Unavailable{" "}
              {context.diagnostics.unavailable_assets}
            </p>
            <p>
              {context.diagnostics.candidate_comparisons.toLocaleString()}{" "}
              bounded comparisons · limit{" "}
              {context.diagnostics.candidate_limit_per_asset} anchors per asset
            </p>
          </details>
        </>
      )}
    </section>
  );
}
