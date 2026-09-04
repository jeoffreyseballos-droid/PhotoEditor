import { formatBytes, formatDate } from "../format";
import type { Asset } from "../types";

export function MetadataPanel({ asset }: { asset: Asset | null }) {
  if (!asset)
    return (
      <aside className="metadata-panel">
        <div className="eyebrow">PHOTO DETAILS</div>
        <div className="metadata-empty">
          <span aria-hidden="true">ⓘ</span>
          <p>
            Select a photo to inspect
            <br />
            its available metadata.
          </p>
        </div>
      </aside>
    );
  const metadata = asset.metadata;
  const fields: [string, string | number | null][] = [
    ["File type", asset.file_type.toUpperCase()],
    ["File size", formatBytes(asset.file_size)],
    [
      "Dimensions",
      metadata.width && metadata.height
        ? `${metadata.width.toLocaleString()} × ${metadata.height.toLocaleString()}`
        : null,
    ],
    ["Camera make", metadata.camera_make],
    ["Camera model", metadata.camera_model],
    ["Lens", metadata.lens],
    ["Lens make", metadata.lens_make],
    ["ISO", metadata.iso],
    ["Shutter speed", metadata.shutter_speed],
    ["Aperture", metadata.aperture],
    ["Focal length", metadata.focal_length],
    ["Exposure compensation", metadata.exposure_compensation],
    ["Orientation (EXIF)", metadata.orientation],
    ["White balance", metadata.camera_white_balance],
    ["Color space", metadata.color_space],
    ["Color profile", metadata.color_profile],
    ["Bit depth", metadata.bit_depth],
    [
      "RAW dimensions",
      metadata.raw_width && metadata.raw_height
        ? `${metadata.raw_width.toLocaleString()} × ${metadata.raw_height.toLocaleString()}`
        : null,
    ],
    ["Captured (camera time)", metadata.capture_timestamp],
    ["Modified", formatDate(asset.modified_at)],
  ];
  return (
    <aside className="metadata-panel" aria-label="Selected photo metadata">
      <div className="eyebrow">PHOTO DETAILS</div>
      <h2>{asset.filename}</h2>
      <span className="pill">{asset.file_type.toUpperCase()}</span>
      <dl>
        {fields.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>
              {value ?? <span className="unavailable">Not available</span>}
            </dd>
          </div>
        ))}
      </dl>
      <div className="path-detail">
        <span className="eyebrow">ORIGINAL PATH</span>
        <p>{asset.original_path}</p>
      </div>
      {asset.warnings.map((warning, index) => (
        <p key={index} className="notice subtle">
          <strong>{warning.category}: </strong>
          {warning.message}
        </p>
      ))}
      <p className="field-hint">
        Missing fields are normal for some cameras and RAW formats. No RAW
        development is performed.
      </p>
    </aside>
  );
}
