import type { Asset } from "../types";
import type { StyleAssetInference, StyleSummary } from "../trained-styles";

function signed(value: number | null, suffix = "") {
  return value === null
    ? "Unavailable"
    : `${value >= 0 ? "+" : ""}${value.toFixed(2)}${suffix}`;
}

export function StyleInferenceInspector({
  style,
  asset,
  inference,
}: {
  style: StyleSummary;
  asset: Asset | null;
  inference: StyleAssetInference | null;
}) {
  if (!asset) return null;
  return (
    <details className="style-inference-inspector">
      <summary>Style Inference details</summary>
      <div className="inspector-grid">
        <div>
          <span>Style</span>
          <strong>
            {style.name} v{style.version}
          </strong>
        </div>
        <div>
          <span>Photo</span>
          <strong>{asset.filename}</strong>
        </div>
        <div>
          <span>Status</span>
          <strong>
            {inference?.stale
              ? "Inputs changed — reapply"
              : (inference?.status ?? "No saved inference")}
          </strong>
        </div>
        <div>
          <span>Confidence</span>
          <strong>{inference?.prediction?.confidence ?? "Unavailable"}</strong>
        </div>
      </div>
      {inference?.feature_summary && (
        <dl className="inference-values">
          <dt>Source median luminance</dt>
          <dd>{inference.feature_summary.median_luminance.toFixed(3)}</dd>
          <dt>Relative group exposure</dt>
          <dd>
            {signed(inference.feature_summary.batch_exposure_delta_ev, " EV")}
          </dd>
          <dt>Source warmth</dt>
          <dd>{signed(inference.feature_summary.warm_cool_balance)}</dd>
          <dt>Warmth relative to group</dt>
          <dd>{signed(inference.feature_summary.batch_warm_cool_delta)}</dd>
          <dt>Missing inputs</dt>
          <dd>{inference.feature_summary.missing_feature_count}</dd>
        </dl>
      )}
      {inference?.prediction && (
        <dl className="inference-values">
          <dt>Exposure</dt>
          <dd>{signed(inference.prediction.adjustments.exposure_ev, " EV")}</dd>
          <dt>Temperature</dt>
          <dd>
            {signed(inference.prediction.adjustments.temperature_delta, " K")}
          </dd>
          <dt>Highlights</dt>
          <dd>{signed(inference.prediction.adjustments.highlights)}</dd>
          <dt>Shadows</dt>
          <dd>{signed(inference.prediction.adjustments.shadows)}</dd>
          <dt>Vibrance</dt>
          <dd>{signed(inference.prediction.adjustments.vibrance)}</dd>
        </dl>
      )}
      {inference?.error && <p className="error">{inference.error}</p>}
      <p className="muted">
        Development diagnostics only. These are predicted controls, not model
        tensors or a claim about photographer preference.
      </p>
    </details>
  );
}
