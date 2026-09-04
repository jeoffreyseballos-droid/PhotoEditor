import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { Asset } from "../types";

export function Thumbnail({
  asset,
  selected,
  onSelect,
}: {
  asset: Asset;
  selected: boolean;
  onSelect: () => void;
}) {
  const element = useRef<HTMLButtonElement>(null);
  const [visible, setVisible] = useState(false);
  const [source, setSource] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const node = element.current;
    if (!node) return;
    if (typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "150px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!visible || asset.preview_status !== "ready") return;
    let cancelled = false;
    setSource(null);
    setFailed(false);
    void api
      .thumbnail(asset.job_id, asset.id)
      .then((value) => {
        if (!cancelled) {
          setSource(value);
          setFailed(!value);
        }
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [
    visible,
    asset.id,
    asset.job_id,
    asset.preview_status,
    asset.fingerprint,
  ]);

  return (
    <button
      ref={element}
      className={`photo-card ${selected ? "selected" : ""}`}
      onClick={onSelect}
      aria-pressed={selected}
      aria-label={`Select ${asset.filename}`}
    >
      <div className="photo-frame">
        {source && !failed ? (
          <img
            src={source}
            alt={asset.filename}
            loading="lazy"
            decoding="async"
            onError={() => setFailed(true)}
          />
        ) : (
          <div className="preview-placeholder">
            <span aria-hidden="true">▧</span>
            <small>
              {asset.preview_status === "unavailable"
                ? "No embedded preview"
                : asset.preview_status === "failed" || failed
                  ? "Preview unavailable"
                  : "Loading preview…"}
            </small>
          </div>
        )}
        <span className="file-badge">{asset.file_type.toUpperCase()}</span>
      </div>
      <div className="photo-caption" title={asset.filename}>
        <span>{asset.filename}</span>
        {selected && <span className="selected-dot" aria-hidden="true" />}
      </div>
    </button>
  );
}
