// View-only mirror of photo-contracts::analysis. Rust owns validation and measurements.
export type PhotoType = "portrait" | "real_estate" | "landscape";
export type Observation<T> =
  | { status: "available"; value: T; confidence: number | null }
  | { status: "unavailable" | "not_applicable" | "failed"; reason: string };
export type Point = { x: number; y: number };
export type BoundingBox = Point & { width: number; height: number };
export type LevelEstimate = {
  angle_degrees: number;
  position: number;
  support_fraction: number;
};
export type RegionMeasurements = {
  mean_luminance: number;
  luminance_stddev: number;
  mean_rgb: number[];
  edge_strength: number;
};
export type SubjectMeasurements = {
  geometry: {
    bbox: BoundingBox;
    centroid: Point;
    area_fraction: number;
    center_distance: number;
    top_margin: number;
    edge_proximity: number;
  };
  subject: RegionMeasurements;
  background: RegionMeasurements;
  subject_background_ev_difference: number;
  mask_reference: string;
};
export interface CommonAnalysis {
  source: {
    width: number;
    height: number;
    metadata_width: number | null;
    metadata_height: number | null;
    exif_orientation: number | null;
    camera_make: string | null;
    camera_model: string | null;
    lens: string | null;
    focal_length: string | null;
    aperture: string | null;
    shutter_speed: string | null;
    iso: number | null;
    capture_timestamp: string | null;
    raw: boolean;
    color_representation: string;
    decoder: string;
  };
  exposure: {
    mean_luminance: number;
    median_luminance: number;
    percentiles: {
      p01: number;
      p05: number;
      p25: number;
      p50: number;
      p75: number;
      p95: number;
      p99: number;
    };
    shadow_fraction: number;
    midtone_fraction: number;
    highlight_fraction: number;
    shadow_clip_fraction: number;
    highlight_clip_fraction: number;
    near_shadow_clip_fraction: number;
    near_highlight_clip_fraction: number;
    any_channel_highlight_clip_fraction: number;
    classification: Observation<
      | "strongly_underexposed"
      | "underexposed"
      | "balanced"
      | "overexposed"
      | "strongly_overexposed"
    >;
  };
  dynamic_range: {
    percentile_range: number;
    interquartile_range: number;
    percentile_ev_span: number;
    high_contrast_tendency: Observation<number>;
    low_contrast_tendency: Observation<number>;
  };
  color: {
    mean_rgb: number[];
    warm_cool_balance: number;
    green_magenta_balance: number;
    average_chroma: number;
    mean_saturation: number;
    low_saturation_fraction: number;
    high_saturation_fraction: number;
    dominant_families: { name: string; fraction: number }[];
    spatial_cast_variation: number;
  };
  detail: {
    edge_strength: number;
    laplacian_rms: number;
    sharpness_grid: number[];
    blur_likelihood: Observation<number>;
    motion_blur_likelihood: Observation<number>;
    noise: Observation<{
      luminance_sigma: number;
      chroma_sigma: number;
      severity: number;
      flat_region_fraction: number;
    }>;
  };
  composition: {
    aspect_ratio: number;
    orientation: string;
    horizontal_line: Observation<LevelEstimate>;
    vertical_line: Observation<LevelEstimate>;
    horizon: Observation<LevelEstimate>;
    keystone_indicator: Observation<number>;
  };
  scene: {
    low_key_tendency: Observation<number>;
    high_key_tendency: Observation<number>;
    low_light_tendency: Observation<number>;
    indoor_outdoor: Observation<string>;
    brightest_region: Point;
  };
  warnings: string[];
}
export interface PhotoAnalysis {
  schema_version: 1;
  analysis_id: string;
  asset_id: string;
  source_fingerprint: string;
  created_at: string;
  photo_type: PhotoType;
  common: CommonAnalysis;
  subjects: {
    subject_present: Observation<boolean>;
    measurements: Observation<SubjectMeasurements>;
    subject_count: Observation<number>;
    faces: Observation<
      {
        bbox: BoundingBox;
        relative_size: number;
        luminance: number;
        sharpness: number;
        confidence: number;
      }[]
    >;
  };
  lighting: {
    overall_light_level: number;
    subject_light_level: Observation<number>;
    background_light_level: Observation<number>;
    subject_background_ev_difference: Observation<number>;
    backlighting_tendency: Observation<number>;
    mixed_lighting_tendency: Observation<number>;
  };
  type_specific:
    | {
        photo_type: "portrait";
        measurements: {
          backlighting: Observation<number>;
          face_provider: string;
        };
      }
    | {
        photo_type: "real_estate";
        measurements: {
          interior_exterior: Observation<string>;
          bright_region_fraction: number;
          shadow_depth: number;
          mixed_lighting: Observation<number>;
          estimated_roll: Observation<LevelEstimate>;
        };
      }
    | {
        photo_type: "landscape";
        measurements: {
          sky_fraction: Observation<number>;
          foreground_fraction: Observation<number>;
          low_contrast_tendency: Observation<number>;
          horizon: Observation<LevelEstimate>;
        };
      };
  confidence: Observation<number>;
  diagnostics: {
    engine_version: string;
    providers: { provider: string; model: string; version: string }[];
    analyzers: { analyzer: string; status: string; message: string }[];
    duration_ms: number;
    common_cache_reused: boolean;
    warnings: string[];
  };
}
export interface AnalysisState {
  status:
    | "not_analyzed"
    | "queued"
    | "analyzing"
    | "complete"
    | "warning"
    | "failed"
    | "cancelled"
    | "interrupted";
  analysis: PhotoAnalysis | null;
  cached: boolean;
  error: string | null;
}
export interface AnalysisRequest {
  job_id: string;
  asset_id: string;
  photo_type: PhotoType;
  request_id: string;
}
