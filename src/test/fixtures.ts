import type { Asset, Job } from "../types";

export const job: Job = {
  id: "job-1",
  name: "Studio portraits",
  input_path: "C:\\Photos\\Input",
  output_path: "C:\\Photos\\Output",
  created_at: "2026-09-03T12:00:00Z",
  updated_at: "2026-09-03T12:00:00Z",
  status: "ready",
  asset_count: 1,
  warning_count: 0,
  warnings: { metadata: 0, preview: 0, unreadable: 0, access: 0, traversal: 0 },
  last_error: null,
};

export function asset(id = "photo-1"): Asset {
  return {
    id,
    job_id: job.id,
    original_path: `C:\\Photos\\Input\\${id}.nef`,
    filename: `${id}.nef`,
    file_type: "nef",
    file_size: 32 * 1024 ** 2,
    modified_at: null,
    fingerprint: id,
    metadata: {
      width: null,
      height: null,
      camera_make: "Nikon",
      camera_model: null,
      lens: null,
      iso: 100,
      shutter_speed: null,
      aperture: null,
      focal_length: null,
      capture_timestamp: null,
      orientation: null,
      lens_make: null,
      exposure_compensation: null,
      color_space: null,
      color_profile: null,
      raw_width: null,
      raw_height: null,
      camera_white_balance: null,
      bit_depth: null,
    },
    thumbnail_path: null,
    preview_status: "unavailable",
    metadata_warning: null,
    warnings: [],
    created_at: job.created_at,
  };
}
