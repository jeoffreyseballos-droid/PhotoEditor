import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../api";
import type { AnalysisState, Observation, PhotoType } from "../analysis";
import type { Asset } from "../types";

function observed<T>(
  o: Observation<T>,
  format: (v: T) => string = String,
): string {
  return o.status === "available"
    ? `${format(o.value)}${o.confidence === null ? " · confidence not supplied" : ` · evidence ${o.confidence.toFixed(2)}`}`
    : `${o.status.replaceAll("_", " ")}: ${o.reason}`;
}
const number = (n: number) => n.toFixed(4);
const percent = (n: number) => `${(n * 100).toFixed(2)}%`;
export function AnalysisInspector({ asset }: { asset: Asset }) {
  const [open, setOpen] = useState(false);
  return (
    <details
      className="analysis-inspector"
      onToggle={(e) => setOpen(e.currentTarget.open)}
    >
      <summary>Photo analysis · source measurements</summary>
      {open && (
        <AnalysisContent key={`${asset.job_id}/${asset.id}`} asset={asset} />
      )}
    </details>
  );
}
export function AnalysisContent({ asset }: { asset: Asset }) {
  const [kind, setKind] = useState<PhotoType>("portrait");
  const [state, setState] = useState<AnalysisState | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [diagram, setDiagram] = useState(false);
  const epoch = useRef(0);
  const request = useRef<string | null>(null);
  useEffect(() => {
    const version = ++epoch.current;
    setLoading(true);
    setState(null);
    setError("");
    setMessage("");
    setDiagram(false);
    void api
      .getAnalysis(asset.job_id, asset.id, kind)
      .then((s) => {
        if (epoch.current === version) setState(s);
      })
      .catch((e) => {
        if (epoch.current === version) setError(errorMessage(e));
      })
      .finally(() => {
        if (epoch.current === version) setLoading(false);
      });
    return () => {
      epoch.current = version + 1;
      if (request.current)
        void api.cancelAnalysis(request.current).catch(() => {});
    };
  }, [asset.job_id, asset.id, kind]);
  async function run() {
    const version = epoch.current;
    const id = crypto.randomUUID();
    request.current = id;
    setBusy(true);
    setError("");
    setMessage("Queued / analyzing source…");
    try {
      const result = await api.analyzeAsset({
        job_id: asset.job_id,
        asset_id: asset.id,
        photo_type: kind,
        request_id: id,
      });
      if (epoch.current === version) {
        setState(result);
        setMessage(
          result.cached
            ? "Reused saved source analysis."
            : "Analysis saved. Source and recipe unchanged.",
        );
      }
    } catch (e) {
      if (epoch.current === version) {
        setError(errorMessage(e));
        setMessage("");
      }
    } finally {
      if (epoch.current === version) {
        setBusy(false);
        request.current = null;
      }
    }
  }
  async function invalidate() {
    const version = epoch.current;
    setBusy(true);
    setError("");
    try {
      await api.invalidateAnalysis(asset.job_id, asset.id);
      if (epoch.current === version) {
        setState(null);
        setMessage(
          "Discarded disposable analysis for all photo types. Run again to recompute.",
        );
      }
    } catch (e) {
      if (epoch.current === version) setError(errorMessage(e));
    } finally {
      if (epoch.current === version) setBusy(false);
    }
  }
  async function exportJson() {
    const version = epoch.current;
    setBusy(true);
    setError("");
    try {
      const path = await api.exportAnalysis(asset.job_id, asset.id, kind);
      if (epoch.current === version) setMessage(`Analysis JSON saved: ${path}`);
    } catch (e) {
      if (epoch.current === version) setError(errorMessage(e));
    } finally {
      if (epoch.current === version) setBusy(false);
    }
  }
  const a = state?.analysis;
  const c = a?.common;
  const subject =
    a?.subjects.measurements.status === "available"
      ? a.subjects.measurements.value
      : null;
  const line =
    c?.composition.horizontal_line.status === "available"
      ? c.composition.horizontal_line.value
      : null;
  return (
    <section aria-label="Photo analysis inspector">
      <p>
        Measures the unedited source. Does not change adjustments, recipes,
        previews, or exports.
      </p>
      <label>
        Analysis photo type{" "}
        <select
          value={kind}
          disabled={busy}
          onChange={(e) => setKind(e.target.value as PhotoType)}
        >
          <option value="portrait">Portrait</option>
          <option value="real_estate">Real Estate</option>
          <option value="landscape">Landscape</option>
        </select>
      </label>
      <div className="development-actions">
        <button disabled={busy || loading} onClick={() => void run()}>
          Analyze source
        </button>
        <button
          disabled={!busy || !request.current}
          onClick={() => {
            if (request.current)
              void api
                .cancelAnalysis(request.current)
                .then(() => setMessage("Cancellation requested…"))
                .catch((e) => setError(errorMessage(e)));
          }}
        >
          Cancel analysis
        </button>
        <button disabled={busy || loading} onClick={() => void invalidate()}>
          Invalidate analysis
        </button>
        <button disabled={!a || busy} onClick={() => void exportJson()}>
          Export analysis JSON
        </button>
      </div>
      <p role="status">
        {loading
          ? "Loading source analysis…"
          : message || state?.status.replaceAll("_", " ") || "Not analyzed"}
      </p>
      {(error || state?.error) && <p role="alert">{error || state?.error}</p>}
      {a && c && (
        <>
          <dl className="analysis-metrics">
            <dt>Analysis</dt>
            <dd>
              v{a.schema_version} · {a.analysis_id}
            </dd>
            <dt>Input</dt>
            <dd>
              {c.source.width} × {c.source.height} ·{" "}
              {c.source.raw ? "Developed RAW" : "Normalized raster"}
            </dd>
            <dt>Mean / median luminance</dt>
            <dd>
              {number(c.exposure.mean_luminance)} /{" "}
              {number(c.exposure.median_luminance)}
            </dd>
            <dt>Exposure classification</dt>
            <dd>
              {observed(c.exposure.classification, (v) =>
                v.replaceAll("_", " "),
              )}{" "}
              · brightness heuristic, not an editing judgment
            </dd>
            <dt>Highlight / shadow clipping</dt>
            <dd>
              {percent(c.exposure.highlight_clip_fraction)} /{" "}
              {percent(c.exposure.shadow_clip_fraction)}
            </dd>
            <dt>Near-highlight / near-shadow clipping</dt>
            <dd>
              {percent(c.exposure.near_highlight_clip_fraction)} /{" "}
              {percent(c.exposure.near_shadow_clip_fraction)}
            </dd>
            <dt>Tonal range p05–p95</dt>
            <dd>
              {number(c.dynamic_range.percentile_range)} ·{" "}
              {number(c.dynamic_range.percentile_ev_span)} EV proxy span
            </dd>
            <dt>Warm–cool / green–magenta balance</dt>
            <dd>
              {number(c.color.warm_cool_balance)} /{" "}
              {number(c.color.green_magenta_balance)} · measured color, not a WB
              error
            </dd>
            <dt>Mean saturation</dt>
            <dd>{number(c.color.mean_saturation)}</dd>
            <dt>Subject presence</dt>
            <dd>
              {observed(a.subjects.subject_present, (v) =>
                v ? "Present in alpha" : "No usable subject alpha",
              )}
            </dd>
            <dt>Subject frame occupancy</dt>
            <dd>
              {subject
                ? percent(subject.geometry.area_fraction)
                : observed(a.subjects.measurements, () => "")}
            </dd>
            <dt>Subject / background EV difference</dt>
            <dd>
              {observed(a.lighting.subject_background_ev_difference, number)}
            </dd>
            <dt>Faces</dt>
            <dd>
              {observed(a.subjects.faces, (v) => `${v.length} detections`)}
            </dd>
            <dt>Edge strength / Laplacian RMS</dt>
            <dd>
              {number(c.detail.edge_strength)} /{" "}
              {number(c.detail.laplacian_rms)} · proxy-scale, not a defect score
            </dd>
            <dt>Noise</dt>
            <dd>
              {observed(
                c.detail.noise,
                (v) =>
                  `Luminance σ ${number(v.luminance_sigma)}, chroma σ ${number(v.chroma_sigma)}, severity ${number(v.severity)}`,
              )}
            </dd>
            <dt>Horizontal reference / candidate level</dt>
            <dd>
              {observed(
                c.composition.horizontal_line,
                (v) => `${v.angle_degrees.toFixed(1)}° clockwise`,
              )}
            </dd>
            <dt>Vertical reference</dt>
            <dd>
              {observed(
                c.composition.vertical_line,
                (v) => `${v.angle_degrees.toFixed(1)}° clockwise`,
              )}
            </dd>
            <dt>Mixed-lighting tendency</dt>
            <dd>
              {observed(a.lighting.mixed_lighting_tendency, number)} · object
              colors can mimic mixed light
            </dd>
            <dt>Timing</dt>
            <dd>
              {a.diagnostics.duration_ms} ms ·{" "}
              {a.diagnostics.common_cache_reused
                ? "common metrics reused"
                : "common metrics measured"}
            </dd>
          </dl>
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={diagram}
              onChange={(e) => setDiagram(e.target.checked)}
            />
            Show source-coordinate geometry diagram
          </label>
          {diagram && (
            <figure className="analysis-diagram">
              <svg
                role="img"
                aria-label="Source geometry debug diagram"
                viewBox={`0 0 ${c.source.width} ${c.source.height}`}
              >
                <rect
                  width={c.source.width}
                  height={c.source.height}
                  fill="#16212c"
                />
                {subject && (
                  <rect
                    x={subject.geometry.bbox.x * c.source.width}
                    y={subject.geometry.bbox.y * c.source.height}
                    width={subject.geometry.bbox.width * c.source.width}
                    height={subject.geometry.bbox.height * c.source.height}
                    fill="none"
                    stroke="#ec77cf"
                    strokeWidth={c.source.width / 200}
                  />
                )}
                {line && (
                  <line
                    x1={0}
                    x2={c.source.width}
                    y1={
                      line.position * c.source.height -
                      (Math.tan((line.angle_degrees * Math.PI) / 180) *
                        c.source.width) /
                        2
                    }
                    y2={
                      line.position * c.source.height +
                      (Math.tan((line.angle_degrees * Math.PI) / 180) *
                        c.source.width) /
                        2
                    }
                    stroke="#ffd078"
                    strokeWidth={c.source.width / 200}
                  />
                )}
              </svg>
              <figcaption>
                Oriented source frame; pink = subject box, gold = straight-line
                candidate. Not the cropped/edited preview. UI only.
              </figcaption>
            </figure>
          )}
          <details>
            <summary>Photo-type-specific measurements</summary>
            <pre>{JSON.stringify(a.type_specific, null, 2)}</pre>
          </details>
          <details>
            <summary>Confidence and diagnostics</summary>
            <pre>{JSON.stringify(a.diagnostics, null, 2)}</pre>
          </details>
          <details>
            <summary>View PhotoAnalysis JSON</summary>
            <pre>{JSON.stringify(a, null, 2)}</pre>
          </details>
        </>
      )}
    </section>
  );
}
