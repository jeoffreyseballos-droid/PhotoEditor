import { useEffect, useState } from "react";
import { api, errorMessage } from "../api";
import type { PhotoType } from "../analysis";
import type { BuiltInPreset } from "../presets";
import type { StyleSummary } from "../trained-styles";

const PHOTO_TYPES: PhotoType[] = ["portrait", "real_estate", "landscape"];

function label(value: string) {
  return value.replaceAll("_", " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

export function PresetsScreen({ onClose }: { onClose: () => void }) {
  const [builtIns, setBuiltIns] = useState<BuiltInPreset[]>([]);
  const [trained, setTrained] = useState<StyleSummary[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      api.builtinPresets(),
      ...PHOTO_TYPES.map((photoType) => api.trainedStyles(photoType)),
    ])
      .then(([presets, ...styles]) => {
        if (cancelled) return;
        setBuiltIns(presets);
        setTrained(styles.flat());
      })
      .catch((cause) => {
        if (!cancelled) setError(errorMessage(cause));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <section className="screen presets-screen">
      <header className="job-header">
        <div>
          <div className="eyebrow">PRESETS / STYLES</div>
          <h1>Presets</h1>
          <p className="subtitle">
            Built-in looks and adaptive styles learned in Training Studio.
          </p>
        </div>
        <button onClick={onClose}>Back to home</button>
      </header>
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      <section className="panel preset-library">
        <div className="section-heading">
          <h2>My trained styles</h2>
          <span>{trained.length}</span>
        </div>
        {!trained.length ? (
          <div className="empty-state">
            <h3>No trained styles yet</h3>
            <p>Learn one from before/after examples in Training Studio.</p>
          </div>
        ) : (
          <div className="preset-grid">
            {trained.map((style) => (
              <article className="panel preset-card" key={style.style_id}>
                <span className="eyebrow">TRAINED STYLE</span>
                <h3>{style.name}</h3>
                <p>{label(style.photo_type)}</p>
                <small>{style.description}</small>
                <div className="training-actions">
                  <button
                    disabled
                    title="Choose a job to use this style in Editing"
                  >
                    Use in Editing
                  </button>
                  <span className="muted">v{style.version}</span>
                </div>
              </article>
            ))}
          </div>
        )}
      </section>
      <section className="panel preset-library">
        <div className="section-heading">
          <h2>Built-in</h2>
          <span>{builtIns.length}</span>
        </div>
        <div className="preset-grid">
          {builtIns.map((preset) => (
            <article className="panel preset-card" key={preset.id}>
              <span className="eyebrow">BUILT-IN</span>
              <h3>{preset.name}</h3>
              <p>{preset.description}</p>
              <small>Available from a job's Editing workflow.</small>
            </article>
          ))}
        </div>
      </section>
    </section>
  );
}
