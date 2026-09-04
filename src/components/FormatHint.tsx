import { useEffect, useState } from "react";
import { api } from "../api";

/** Display the native registry, never maintain a second discovery allowlist here. */
export function FormatHint() {
  const [formats, setFormats] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void api
      .formats()
      .then((values) => {
        if (!cancelled)
          setFormats(
            values
              .filter((v) => v.discoverable)
              .map((v) => v.extension.toUpperCase())
              .join(", "),
          );
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);
  return (
    <span>
      {formats
        ? `Still-photo formats: ${formats}.`
        : "Supported still-photo formats only."}{" "}
      Videos, animations and project files are skipped.
    </span>
  );
}
