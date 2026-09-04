import { useState } from "react";
import type { RenderAdjustments } from "../types";
import {
  neutralAdjustments,
  neutralLocal,
  neutralPresence,
  neutralDetail,
} from "../toolkit";
import type {
  Basic,
  Detail,
  Presence,
  ToneCurve,
  LocalLayer,
  MaskDiagnostic,
  LensDiagnostic,
} from "../toolkit";
export function NumberControl({
  label,
  value,
  min = -100,
  max = 100,
  step = 1,
  change,
  disabled = false,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  change: (v: number) => void;
  disabled?: boolean;
}) {
  return (
    <label>
      {label}
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={(e) => {
          if (Number.isFinite(e.currentTarget.valueAsNumber))
            change(e.currentTarget.valueAsNumber);
        }}
      />
    </label>
  );
}
const basicControls: [keyof Basic, string, number, number, number][] = [
  ["exposure_ev", "Exposure (EV)", -5, 5, 0.1],
  ["temperature", "Temperature (relative K)", 2000, 12000, 100],
  ["tint", "Tint (green − / magenta +)", -100, 100, 1],
  ["contrast", "Contrast", -100, 100, 1],
  ["highlights", "Highlights", -100, 100, 1],
  ["shadows", "Shadows", -100, 100, 1],
  ["whites", "Whites", -100, 100, 1],
  ["blacks", "Blacks", -100, 100, 1],
  ["saturation", "Saturation", -100, 100, 1],
  ["vibrance", "Vibrance", -100, 100, 1],
];
export function BasicControls({
  value,
  change,
  prefix = "",
  color = false,
}: {
  value: Basic;
  change: (v: Basic) => void;
  prefix?: string;
  color?: boolean;
}) {
  return (
    <div className="development-controls">
      {basicControls
        .filter(
          ([key]) =>
            ["temperature", "tint", "saturation", "vibrance"].includes(key) ===
            color,
        )
        .map(([key, label, min, max, step]) => (
          <NumberControl
            key={key}
            label={`${prefix}${label}`}
            value={value[key]}
            min={min}
            max={max}
            step={step}
            change={(v) => change({ ...value, [key]: v })}
          />
        ))}
    </div>
  );
}
function PresenceControls({
  value,
  change,
  prefix = "",
}: {
  value: Presence;
  change: (v: Presence) => void;
  prefix?: string;
}) {
  return (
    <div className="development-controls">
      {(["texture", "clarity", "dehaze"] as const).map((key) => (
        <NumberControl
          key={key}
          label={`${prefix}${key[0].toUpperCase() + key.slice(1)}`}
          value={value[key]}
          change={(v) => change({ ...value, [key]: v })}
        />
      ))}
    </div>
  );
}
function DetailControls({
  value,
  change,
  prefix = "",
}: {
  value: Detail;
  change: (v: Detail) => void;
  prefix?: string;
}) {
  return (
    <>
      <h4>Sharpening</h4>
      <div className="development-controls">
        {(["amount", "radius", "detail", "masking"] as const).map((key) => (
          <NumberControl
            key={key}
            label={`${prefix}Sharpening ${key}`}
            value={value.sharpening[key]}
            min={key === "radius" ? 0.5 : 0}
            max={key === "radius" ? 3 : 100}
            step={key === "radius" ? 0.1 : 1}
            change={(v) =>
              change({
                ...value,
                sharpening: { ...value.sharpening, [key]: v },
              })
            }
          />
        ))}
      </div>
      <h4>Noise reduction</h4>
      <div className="development-controls">
        {(
          ["luminance", "luminance_detail", "color", "color_detail"] as const
        ).map((key) => (
          <NumberControl
            key={key}
            label={`${prefix}NR ${key.replaceAll("_", " ")}`}
            value={value.noise[key]}
            min={0}
            change={(v) =>
              change({ ...value, noise: { ...value.noise, [key]: v } })
            }
          />
        ))}
      </div>
    </>
  );
}
export function ToolkitControls({
  a,
  change,
  lens,
  mask,
  onMask,
  hideOverlay,
  reset,
}: {
  a: RenderAdjustments;
  change: (a: RenderAdjustments) => void;
  reset: (a: RenderAdjustments) => void;
  lens?: LensDiagnostic;
  mask?: MaskDiagnostic;
  onMask: (generate: boolean, layerId: string | null) => void;
  hideOverlay: () => void;
}) {
  const [channel, setChannel] = useState<keyof ToneCurve>("master");
  const points = a.curve[channel];
  function layerChange(index: number, layer: LocalLayer, isReset = false) {
    (isReset ? reset : change)({
      ...a,
      local_layers: a.local_layers.map((old, i) => (i === index ? layer : old)),
    });
  }
  function addLayer(kind: "subject" | "background") {
    change({
      ...a,
      local_layers: [
        ...a.local_layers,
        {
          id: crypto.randomUUID(),
          mask_type: kind,
          enabled: true,
          strength: 1,
          invert: false,
          confidence: null,
          mask_reference: null,
          adjustments: neutralLocal(),
        },
      ],
    });
  }
  return (
    <>
      <details>
        <summary>Color Mixer</summary>
        <p className="muted">
          Overlapping hue bands; ±100. Hue range ±30°. Luminance range ±1 EV
          within each band.
        </p>
        {[
          "Red",
          "Orange",
          "Yellow",
          "Green",
          "Aqua",
          "Blue",
          "Purple",
          "Magenta",
        ].map((name, i) => (
          <fieldset key={name}>
            <legend>{name}</legend>
            <div className="development-controls">
              {(["hue", "saturation", "luminance"] as const).map((key) => (
                <NumberControl
                  key={key}
                  label={`${name} ${key}`}
                  value={a.hsl[i][key]}
                  change={(v) =>
                    change({
                      ...a,
                      hsl: a.hsl.map((b, j) =>
                        i === j ? { ...b, [key]: v } : b,
                      ),
                    })
                  }
                />
              ))}
            </div>
          </fieldset>
        ))}
        <button onClick={() => reset({ ...a, hsl: neutralAdjustments().hsl })}>
          Reset Mixer
        </button>
      </details>
      <details>
        <summary>Curve</summary>
        <label>
          Curve channel
          <select
            value={channel}
            onChange={(e) => setChannel(e.target.value as keyof ToneCurve)}
          >
            {["master", "red", "green", "blue"].map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
        </label>
        <p className="muted">
          Perceptual RGB point curve. X strictly increasing; Y nondecreasing.
          End segments extend beyond display white.
        </p>
        {points.map((point, i) => (
          <div className="curve-point" key={i}>
            <NumberControl
              label={`Point ${i + 1} x`}
              value={point.x}
              min={0}
              max={1}
              step={0.01}
              disabled={i === 0 || i === points.length - 1}
              change={(v) =>
                change({
                  ...a,
                  curve: {
                    ...a.curve,
                    [channel]: points.map((p, j) =>
                      i === j ? { ...p, x: v } : p,
                    ),
                  },
                })
              }
            />
            <NumberControl
              label={`Point ${i + 1} y`}
              value={point.y}
              min={0}
              max={1}
              step={0.01}
              change={(v) =>
                change({
                  ...a,
                  curve: {
                    ...a.curve,
                    [channel]: points.map((p, j) =>
                      i === j ? { ...p, y: v } : p,
                    ),
                  },
                })
              }
            />
            {i > 0 && i < points.length - 1 && (
              <button
                onClick={() =>
                  change({
                    ...a,
                    curve: {
                      ...a.curve,
                      [channel]: points.filter((_, j) => i !== j),
                    },
                  })
                }
              >
                Remove point {i + 1}
              </button>
            )}
          </div>
        ))}
        <button
          disabled={points.length >= 16}
          onClick={() => {
            let index = 0;
            for (let i = 1; i < points.length - 1; i++)
              if (
                points[i + 1].x - points[i].x >
                points[index + 1].x - points[index].x
              )
                index = i;
            const p = {
              x: (points[index].x + points[index + 1].x) / 2,
              y: (points[index].y + points[index + 1].y) / 2,
            };
            change({
              ...a,
              curve: {
                ...a.curve,
                [channel]: [
                  ...points.slice(0, index + 1),
                  p,
                  ...points.slice(index + 1),
                ],
              },
            });
          }}
        >
          Add curve point
        </button>
        <button
          onClick={() => reset({ ...a, curve: neutralAdjustments().curve })}
        >
          Reset Curves
        </button>
      </details>
      <details>
        <summary>Effects / Presence</summary>
        <PresenceControls
          value={a.presence}
          change={(presence) => change({ ...a, presence })}
        />
        <h4>Creative post-crop vignette</h4>
        <div className="development-controls">
          {(["amount", "midpoint", "feather", "roundness"] as const).map(
            (key) => (
              <NumberControl
                key={key}
                label={`Creative vignette ${key}`}
                value={a.effects.vignette[key]}
                min={key === "feather" ? 1 : key === "midpoint" ? 0 : -100}
                change={(v) =>
                  change({
                    ...a,
                    effects: { vignette: { ...a.effects.vignette, [key]: v } },
                  })
                }
              />
            ),
          )}
        </div>
        <button
          onClick={() =>
            reset({
              ...a,
              presence: neutralPresence(),
              effects: neutralAdjustments().effects,
            })
          }
        >
          Reset Effects
        </button>
      </details>
      <details>
        <summary>Detail</summary>
        <DetailControls
          value={a.detail}
          change={(detail) => change({ ...a, detail })}
        />
        {(a.sharpening !== 0 || a.noise_reduction !== 0) && (
          <p className="notice">
            Legacy Phase 2 detail is also active. Reset Detail to start with the
            expanded controls.
          </p>
        )}
        <div className="development-controls">
          <NumberControl
            label="Legacy sharpening"
            value={a.sharpening}
            min={0}
            change={(v) => change({ ...a, sharpening: v })}
          />
          <NumberControl
            label="Legacy noise reduction"
            value={a.noise_reduction}
            min={0}
            change={(v) => change({ ...a, noise_reduction: v })}
          />
        </div>
        <button
          onClick={() =>
            reset({
              ...a,
              detail: neutralDetail(),
              sharpening: 0,
              noise_reduction: 0,
            })
          }
        >
          Reset Detail
        </button>
      </details>
      <details>
        <summary>Optics</summary>
        <p className="muted">
          Objective corrections. Profiles default off; unsupported coefficients
          stay unapplied. Distortion retains canvas bounds; crop black edges if
          needed.
        </p>
        {(
          ["enabled", "distortion", "vignette", "chromatic_aberration"] as const
        ).map((key) => (
          <label className="checkbox-label" key={key}>
            <input
              type="checkbox"
              checked={a.optics[key]}
              onChange={(e) =>
                change({
                  ...a,
                  optics: { ...a.optics, [key]: e.target.checked },
                })
              }
            />
            {key === "enabled"
              ? "Enable lens profile correction"
              : key.replaceAll("_", " ")}
          </label>
        ))}
        <div className="development-controls">
          {(
            [
              "distortion_strength",
              "vignette_strength",
              "manual_distortion",
              "manual_vignette",
            ] as const
          ).map((key) => (
            <NumberControl
              key={key}
              label={key.replaceAll("_", " ")}
              value={a.optics[key]}
              min={key.startsWith("manual") ? -100 : 0}
              max={key.startsWith("manual") ? 100 : 1}
              step={key.startsWith("manual") ? 1 : 0.05}
              change={(v) =>
                change({ ...a, optics: { ...a.optics, [key]: v } })
              }
            />
          ))}
        </div>
        <p>
          Last render profile: {lens?.state ?? "not evaluated"} ·{" "}
          {lens?.profile ?? "none"}
        </p>
        {lens?.applied.length ? (
          <p>Applied: {lens.applied.join(", ")}</p>
        ) : null}
        {lens?.warnings.map((w, i) => (
          <p className="notice" key={i}>
            {w}
          </p>
        ))}
        <button
          onClick={() => reset({ ...a, optics: neutralAdjustments().optics })}
        >
          Reset Optics
        </button>
      </details>
      <details open>
        <summary>Masks</summary>
        <p className="muted">
          Local CPU portrait matting. Generate once; source/model changes
          invalidate the cache. Overlay is debug-only and never exported. Local
          temperature is relative to the current image (6500 neutral).
        </p>
        <p>
          Mask: {mask?.status ?? "unavailable"}
          {mask?.width ? ` · ${mask.width} × ${mask.height}` : ""}
        </p>
        {mask?.warnings.map((w, i) => (
          <p className="notice" key={i}>
            {w}
          </p>
        ))}
        <button onClick={() => onMask(true, null)}>
          Generate Subject / Background masks
        </button>
        <button onClick={hideOverlay}>Hide Mask Overlay</button>
        <div className="development-actions">
          {(["subject", "background"] as const).map((kind) => (
            <button
              key={kind}
              disabled={
                a.local_layers.some((l) => l.mask_type === kind) ||
                a.local_layers.length >= 8
              }
              onClick={() => addLayer(kind)}
            >
              Add {kind} layer
            </button>
          ))}
        </div>
        {a.local_layers.map((layer, index) => {
          const title =
            layer.mask_type[0].toUpperCase() + layer.mask_type.slice(1);
          const prefix = `${title} `;
          return (
            <fieldset className="local-layer" key={layer.id}>
              <legend>{title}</legend>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={layer.enabled}
                  onChange={(e) =>
                    layerChange(index, { ...layer, enabled: e.target.checked })
                  }
                />
                {title} enabled
              </label>
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={layer.invert}
                  onChange={(e) =>
                    layerChange(index, { ...layer, invert: e.target.checked })
                  }
                />
                {title} invert
              </label>
              <NumberControl
                label={`${title} strength`}
                value={layer.strength}
                min={0}
                max={1}
                step={0.05}
                change={(v) => layerChange(index, { ...layer, strength: v })}
              />
              <button onClick={() => onMask(false, layer.id)}>
                Show {title} Mask
              </button>
              <details>
                <summary>{title} Light</summary>
                <BasicControls
                  value={layer.adjustments}
                  prefix={prefix}
                  change={(v) =>
                    layerChange(index, {
                      ...layer,
                      adjustments: { ...layer.adjustments, ...v },
                    })
                  }
                />
              </details>
              <details>
                <summary>{title} Color</summary>
                <BasicControls
                  color
                  value={layer.adjustments}
                  prefix={prefix}
                  change={(v) =>
                    layerChange(index, {
                      ...layer,
                      adjustments: { ...layer.adjustments, ...v },
                    })
                  }
                />
              </details>
              <details>
                <summary>{title} Presence</summary>
                <PresenceControls
                  value={layer.adjustments.presence}
                  prefix={prefix}
                  change={(v) =>
                    layerChange(index, {
                      ...layer,
                      adjustments: { ...layer.adjustments, presence: v },
                    })
                  }
                />
              </details>
              <details>
                <summary>{title} Detail</summary>
                <DetailControls
                  value={layer.adjustments.detail}
                  prefix={prefix}
                  change={(v) =>
                    layerChange(index, {
                      ...layer,
                      adjustments: { ...layer.adjustments, detail: v },
                    })
                  }
                />
              </details>
              <button
                onClick={() =>
                  layerChange(
                    index,
                    {
                      ...layer,
                      strength: 1,
                      invert: false,
                      adjustments: neutralLocal(),
                    },
                    true,
                  )
                }
              >
                Reset {title}
              </button>
              <button
                onClick={() =>
                  change({
                    ...a,
                    local_layers: a.local_layers.filter((_, i) => i !== index),
                  })
                }
              >
                Remove {title} layer
              </button>
            </fieldset>
          );
        })}
      </details>
    </>
  );
}
