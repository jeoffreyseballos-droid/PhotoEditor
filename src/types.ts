// IPC DTOs mirror photo-core/models.rs and photo-contracts/MachineResources.
import type { Toolkit } from "./toolkit";
export interface AppError {
  code: "invalid_input" | "not_found" | "io" | "database" | "busy" | "internal";
  message: string;
}

export type FileType =
  | "cr3"
  | "cr2"
  | "nef"
  | "arw"
  | "dng"
  | "raf"
  | "orf"
  | "rw2"
  | "pef"
  | "jpg"
  | "jpeg"
  | "tif"
  | "tiff"
  | "png"
  | "heic"
  | "heif";
export interface PhotoFormat {
  file_type: FileType;
  extension: string;
  family: "camera_raw" | "jpeg" | "tiff" | "png" | "heif";
  discoverable: boolean;
  metadata_supported: "built_in" | "bundled_exiftool" | "partial";
  preview_supported: "built_in" | "bundled_exiftool" | "partial";
  editable_future: boolean;
  develop_supported:
    "libraw_camera_dependent" | "unavailable" | "built_in_variant_dependent";
}
export type WarningCategory =
  "metadata" | "preview" | "unreadable" | "access" | "traversal";
export type WarningSummary = Record<WarningCategory, number>;
export interface IngestionWarning {
  category: WarningCategory;
  code: string;
  message: string;
  path: string | null;
}

export interface ImageMetadata {
  width: number | null;
  height: number | null;
  camera_make: string | null;
  camera_model: string | null;
  lens: string | null;
  iso: number | null;
  shutter_speed: string | null;
  aperture: string | null;
  focal_length: string | null;
  capture_timestamp: string | null;
  orientation: number | null;
  lens_make: string | null;
  exposure_compensation: string | null;
  color_space: string | null;
  color_profile: string | null;
  raw_width: number | null;
  raw_height: number | null;
  camera_white_balance: string | null;
  bit_depth: number | null;
}

export interface Job {
  id: string;
  name: string;
  input_path: string;
  output_path: string;
  created_at: string;
  updated_at: string;
  status: "scanning" | "ready" | "interrupted" | "failed";
  asset_count: number;
  warning_count: number;
  warnings: WarningSummary;
  last_error: string | null;
}

export interface Asset {
  id: string;
  job_id: string;
  original_path: string;
  filename: string;
  file_type: FileType;
  file_size: number;
  modified_at: string | null;
  fingerprint: string;
  metadata: ImageMetadata;
  thumbnail_path: string | null;
  preview_status: "ready" | "unavailable" | "failed";
  metadata_warning: string | null;
  warnings: IngestionWarning[];
  created_at: string;
}

export interface Page<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

export interface NewJobInput {
  name: string;
  input_path: string;
  output_path: string;
}

export interface MachineResources {
  logical_cpu_count: number;
  available_ram_bytes: number;
  total_ram_bytes: number;
  gpu_name: string | null;
  gpu_memory_bytes: number | null;
  available_disk_bytes: number | null;
  gpus: GpuInfo[];
  gpu_detection: string;
  os: string;
  architecture: string;
}

export interface GpuInfo {
  vendor: string | null;
  model: string;
  device_type: string;
  memory_model: string;
  dedicated_vram_bytes: number | null;
  shared_memory_budget_bytes: number | null;
  graphics_api: string | null;
  compute_capability: string | null;
  detection_source: string;
}

export interface RenderAdjustments extends Toolkit {
  exposure_ev: number;
  temperature: number;
  tint: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  saturation: number;
  vibrance: number;
  rotation_degrees: number;
  crop: { x: number; y: number; width: number; height: number };
  sharpening: number;
  noise_reduction: number;
}
export interface DevelopmentState {
  recipe_state?: import("./recipe").RecipeState | null;
  unresolved_masks?: string[] | null;
  diagnostics?: import("./toolkit").ToolkitDiagnostics;
  adjustments: RenderAdjustments;
  revision: number;
  state: string;
  source_identity: string | null;
  preview_path: string | null;
  export_path: string | null;
  error: { code: string; message: string } | null;
  warnings: string[];
}
export interface DevelopmentRequest {
  job_id: string;
  asset_id: string;
  request_id: string;
  adjustments: RenderAdjustments;
  preview: boolean;
  output_format: "jpeg" | "tiff";
  jpeg_quality: number;
}
export interface DevelopmentResult {
  state: DevelopmentState;
  preview_data: string | null;
  width: number;
  height: number;
}
