//! Source observations only. This module deliberately has no edit/recipe types.
use serde::{Deserialize, Serialize};

pub const PHOTO_ANALYSIS_SCHEMA_VERSION: u32 = 1;
pub const MAX_ANALYSIS_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhotoType {
    Portrait,
    RealEstate,
    Landscape,
}
impl PhotoType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Portrait => "portrait",
            Self::RealEstate => "real_estate",
            Self::Landscape => "landscape",
        }
    }
}

/// Confidence is repeatability/evidence strength, NOT a calibrated probability.
/// Direct geometry from a model has None confidence when the provider has none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Observation<T> {
    Available { value: T, confidence: Option<f64> },
    Unavailable { reason: String },
    NotApplicable { reason: String },
    Failed { reason: String },
}
impl<T> Observation<T> {
    pub fn measured(value: T) -> Self {
        Self::Available {
            value,
            confidence: None,
        }
    }
    pub fn inferred(value: T, confidence: f64) -> Self {
        Self::Available {
            value,
            confidence: Some(confidence),
        }
    }
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }
    pub fn value(&self) -> Option<&T> {
        if let Self::Available { value, .. } = self {
            Some(value)
        } else {
            None
        }
    }
}
macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
        impl ValidLeaves for $name { fn check_leaves(&self) -> bool { $(self.$field.check_leaves())&&* } }
    }
}
record!(AnalysisSource {
    width: u32, height: u32, metadata_width: Option<u32>, metadata_height: Option<u32>,
    exif_orientation: Option<u32>, camera_make: Option<String>, camera_model: Option<String>,
    lens: Option<String>, focal_length: Option<String>, aperture: Option<String>,
    shutter_speed: Option<String>, iso: Option<u32>, capture_timestamp: Option<String>,
    raw: bool, color_representation: String, decoder: String
});
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureClass {
    StronglyUnderexposed,
    Underexposed,
    Balanced,
    Overexposed,
    StronglyOverexposed,
}
record!(LuminancePercentiles {
    p01: f64,
    p05: f64,
    p25: f64,
    p50: f64,
    p75: f64,
    p95: f64,
    p99: f64
});
record!(ExposureAnalysis {
    mean_luminance: f64, median_luminance: f64, percentiles: LuminancePercentiles,
    shadow_fraction: f64, midtone_fraction: f64, highlight_fraction: f64,
    shadow_clip_fraction: f64, highlight_clip_fraction: f64,
    near_shadow_clip_fraction: f64, near_highlight_clip_fraction: f64,
    any_channel_highlight_clip_fraction: f64, classification: Observation<ExposureClass>
});
record!(DynamicRangeAnalysis {
    percentile_range: f64, interquartile_range: f64, percentile_ev_span: f64,
    high_contrast_tendency: Observation<f64>, low_contrast_tendency: Observation<f64>
});
record!(ColorFamily {
    name: String,
    fraction: f64
});
record!(ColorAnalysis {
    mean_rgb: [f64; 3], warm_cool_balance: f64, green_magenta_balance: f64,
    average_chroma: f64, mean_saturation: f64, low_saturation_fraction: f64,
    high_saturation_fraction: f64, dominant_families: Vec<ColorFamily>,
    spatial_cast_variation: f64
});
record!(NoiseEstimate {
    luminance_sigma: f64,
    chroma_sigma: f64,
    severity: f64,
    flat_region_fraction: f64
});
record!(DetailAnalysis {
    edge_strength: f64, laplacian_rms: f64, sharpness_grid: [f64; 9],
    blur_likelihood: Observation<f64>, motion_blur_likelihood: Observation<f64>,
    noise: Observation<NoiseEstimate>
});
record!(Point { x: f64, y: f64 });
record!(BoundingBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64
});
record!(LevelEstimate {
    angle_degrees: f64,
    position: f64,
    support_fraction: f64
});
record!(CompositionAnalysis {
    aspect_ratio: f64, orientation: String,
    horizontal_line: Observation<LevelEstimate>, vertical_line: Observation<LevelEstimate>,
    horizon: Observation<LevelEstimate>, keystone_indicator: Observation<f64>
});
record!(SubjectGeometry {
    bbox: BoundingBox,
    centroid: Point,
    area_fraction: f64,
    center_distance: f64,
    top_margin: f64,
    edge_proximity: f64
});
record!(RegionMeasurements {
    mean_luminance: f64,
    luminance_stddev: f64,
    mean_rgb: [f64; 3],
    edge_strength: f64
});
record!(SubjectMeasurements {
    geometry: SubjectGeometry,
    subject: RegionMeasurements,
    background: RegionMeasurements,
    subject_background_ev_difference: f64,
    mask_reference: String
});
record!(FaceGeometry {
    bbox: BoundingBox,
    relative_size: f64,
    luminance: f64,
    sharpness: f64,
    confidence: f64
});
record!(SubjectAnalysis {
    subject_present: Observation<bool>, measurements: Observation<SubjectMeasurements>,
    subject_count: Observation<u32>, faces: Observation<Vec<FaceGeometry>>
});
record!(LightingAnalysis {
    overall_light_level: f64, subject_light_level: Observation<f64>, background_light_level: Observation<f64>,
    subject_background_ev_difference: Observation<f64>, backlighting_tendency: Observation<f64>,
    mixed_lighting_tendency: Observation<f64>
});
record!(SceneAnalysis {
    low_key_tendency: Observation<f64>, high_key_tendency: Observation<f64>,
    low_light_tendency: Observation<f64>, indoor_outdoor: Observation<String>,
    brightest_region: Point
});
record!(PortraitAnalysis { backlighting: Observation<f64>, face_provider: String });
record!(RealEstateAnalysis {
    interior_exterior: Observation<String>, bright_region_fraction: f64,
    shadow_depth: f64, mixed_lighting: Observation<f64>, estimated_roll: Observation<LevelEstimate>
});
record!(LandscapeAnalysis {
    sky_fraction: Observation<f64>, foreground_fraction: Observation<f64>,
    low_contrast_tendency: Observation<f64>, horizon: Observation<LevelEstimate>
});
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "photo_type",
    content = "measurements",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TypeAnalysis {
    Portrait(PortraitAnalysis),
    RealEstate(RealEstateAnalysis),
    Landscape(LandscapeAnalysis),
}
impl ValidLeaves for TypeAnalysis {
    fn check_leaves(&self) -> bool {
        match self {
            Self::Portrait(v) => v.check_leaves(),
            Self::RealEstate(v) => v.check_leaves(),
            Self::Landscape(v) => v.check_leaves(),
        }
    }
}
record!(CommonAnalysis {
    source: AnalysisSource, exposure: ExposureAnalysis, color: ColorAnalysis,
    dynamic_range: DynamicRangeAnalysis, detail: DetailAnalysis, composition: CompositionAnalysis,
    scene: SceneAnalysis, warnings: Vec<String>
});
record!(AnalyzerDiagnostic {
    analyzer: String,
    status: String,
    message: String
});
record!(ProviderIdentity {
    provider: String,
    model: String,
    version: String
});
record!(AnalysisDiagnostics {
    engine_version: String, providers: Vec<ProviderIdentity>, analyzers: Vec<AnalyzerDiagnostic>,
    duration_ms: u64, common_cache_reused: bool, warnings: Vec<String>
});
record!(PhotoAnalysis {
    schema_version: u32, analysis_id: String, asset_id: String, source_fingerprint: String,
    created_at: String, photo_type: PhotoType, common: CommonAnalysis,
    subjects: SubjectAnalysis, lighting: LightingAnalysis, type_specific: TypeAnalysis,
    confidence: Observation<f64>, diagnostics: AnalysisDiagnostics
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum AnalysisError {
    #[error("Unsupported analysis schema version: {0}")]
    UnsupportedVersion(u32),
    #[error("Invalid analysis: {0}")]
    Invalid(String),
}
impl PhotoAnalysis {
    pub fn parse(json: &str) -> Result<Self, AnalysisError> {
        if json.len() > MAX_ANALYSIS_BYTES {
            return Err(AnalysisError::Invalid("Payload exceeds size limit".into()));
        }
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| AnalysisError::Invalid(e.to_string()))?;
        let version = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AnalysisError::Invalid("Missing schema version".into()))?;
        if version != u64::from(PHOTO_ANALYSIS_SCHEMA_VERSION) {
            return Err(AnalysisError::UnsupportedVersion(
                version.min(u32::MAX as u64) as u32,
            ));
        }
        // Future migrations dispatch here; there is no fictitious legacy analysis format.
        let analysis: Self =
            serde_json::from_value(value).map_err(|e| AnalysisError::Invalid(e.to_string()))?;
        analysis.validate()?;
        Ok(analysis)
    }
    pub fn validate(&self) -> Result<(), AnalysisError> {
        let bad = |message: &str| AnalysisError::Invalid(message.into());
        if !self.check_leaves() {
            return Err(bad("Non-finite, invalid confidence, or oversized field"));
        }
        if self.schema_version != PHOTO_ANALYSIS_SCHEMA_VERSION {
            return Err(AnalysisError::UnsupportedVersion(self.schema_version));
        }
        if self.analysis_id.is_empty()
            || self.asset_id.is_empty()
            || self.analysis_id.len() > 128
            || self.asset_id.len() > 128
        {
            return Err(bad("Invalid identity"));
        }
        if self.source_fingerprint.len() != 64
            || !self
                .source_fingerprint
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(bad("Invalid source fingerprint"));
        }
        if chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err() {
            return Err(bad("Invalid timestamp"));
        }
        let s = &self.common.source;
        if s.width < 16 || s.height < 16 || s.width.max(s.height) > 1600 {
            return Err(bad("Invalid analysis dimensions"));
        }
        if s.exif_orientation.is_some_and(|v| !(1..=8).contains(&v)) {
            return Err(bad("Invalid EXIF orientation"));
        }
        if !matches!(
            (&self.photo_type, &self.type_specific),
            (PhotoType::Portrait, TypeAnalysis::Portrait(_))
                | (PhotoType::RealEstate, TypeAnalysis::RealEstate(_))
                | (PhotoType::Landscape, TypeAnalysis::Landscape(_))
        ) {
            return Err(bad("Photo type mismatch"));
        }
        let p = &self.common.exposure.percentiles;
        let percentiles = [p.p01, p.p05, p.p25, p.p50, p.p75, p.p95, p.p99];
        if percentiles.windows(2).any(|w| w[0] > w[1]) {
            return Err(bad("Percentiles are not ordered"));
        }
        let e = &self.common.exposure;
        if (e.median_luminance - p.p50).abs() > 1e-9 {
            return Err(bad("Median does not match p50"));
        }
        if ((e.shadow_fraction + e.midtone_fraction + e.highlight_fraction) - 1.).abs() > 1e-6 {
            return Err(bad("Tonal fractions do not sum to one"));
        }
        // Validate all floating point leaves before JSON serialization (NaN becomes null in serde_json).
        // Deserializing this round-trip rejects every non-optional non-finite numeric field.
        let json = serde_json::to_string(self).map_err(|e| bad(&e.to_string()))?;
        let _: Self = serde_json::from_str(&json).map_err(|_| bad("Non-finite measurement"))?;
        if json.len() > MAX_ANALYSIS_BYTES {
            return Err(bad("Payload exceeds size limit"));
        }
        let value = serde_json::to_value(self).map_err(|e| bad(&e.to_string()))?;
        validate_values(&value, "")?;
        Ok(())
    }
    pub fn canonical_json(&self) -> Result<String, AnalysisError> {
        self.validate()?;
        // serde_json's default map is sorted; converting through Value sorts every object.
        serde_json::to_string(
            &serde_json::to_value(self).map_err(|e| AnalysisError::Invalid(e.to_string()))?,
        )
        .map_err(|e| AnalysisError::Invalid(e.to_string()))
    }
}
fn validate_values(v: &serde_json::Value, key: &str) -> Result<(), AnalysisError> {
    let bad = || AnalysisError::Invalid(format!("Invalid measurement at {key}"));
    match v {
        serde_json::Value::Number(n) => {
            let n = n.as_f64().ok_or_else(bad)?;
            if !n.is_finite() {
                return Err(bad());
            }
            if matches!(
                key,
                "mean_luminance"
                    | "median_luminance"
                    | "p01"
                    | "p05"
                    | "p25"
                    | "p50"
                    | "p75"
                    | "p95"
                    | "p99"
                    | "mean_rgb"
                    | "average_chroma"
                    | "mean_saturation"
                    | "severity"
                    | "percentile_range"
                    | "interquartile_range"
                    | "center_distance"
                    | "top_margin"
                    | "edge_proximity"
                    | "overall_light_level"
                    | "shadow_depth"
            ) && !(0. ..=1.).contains(&n)
            {
                return Err(bad());
            }
            if matches!(key, "warm_cool_balance" | "green_magenta_balance")
                && !(-1. ..=1.).contains(&n)
            {
                return Err(bad());
            }
            if matches!(
                key,
                "luminance_sigma"
                    | "chroma_sigma"
                    | "edge_strength"
                    | "laplacian_rms"
                    | "luminance_stddev"
                    | "percentile_ev_span"
                    | "spatial_cast_variation"
            ) && n < 0.
            {
                return Err(bad());
            }
            if (key.contains("fraction")
                || key == "confidence"
                || key == "position"
                || key == "x"
                || key == "y")
                && !(0. ..=1.).contains(&n)
            {
                return Err(bad());
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                validate_values(v, key)?;
            }
        }
        serde_json::Value::Object(m) => {
            if key == "bbox" {
                let n = |k: &str| m.get(k).and_then(|v| v.as_f64()).unwrap_or(f64::NAN);
                if n("width") <= 0.
                    || n("height") <= 0.
                    || n("x") + n("width") > 1.000001
                    || n("y") + n("height") > 1.000001
                {
                    return Err(bad());
                }
            }
            if key.contains("tendency") {
                if let Some(n) = m.get("value").and_then(|v| v.as_f64()) {
                    if !(0. ..=1.).contains(&n) {
                        return Err(bad());
                    }
                }
            }
            for (key, v) in m {
                validate_values(v, key)?;
            }
        }
        serde_json::Value::String(s) if s.len() > 4096 => return Err(bad()),
        _ => (),
    }
    Ok(())
}
trait ValidLeaves {
    fn check_leaves(&self) -> bool;
}
impl ValidLeaves for f64 {
    fn check_leaves(&self) -> bool {
        self.is_finite()
    }
}
impl ValidLeaves for String {
    fn check_leaves(&self) -> bool {
        self.len() <= 4096
    }
}
macro_rules! finite_primitive {($($t:ty),*)=>{$(impl ValidLeaves for $t {fn check_leaves(&self)->bool{true}})*}}
finite_primitive!(u32, u64, bool, PhotoType, ExposureClass);
impl<T: ValidLeaves> ValidLeaves for Option<T> {
    fn check_leaves(&self) -> bool {
        self.as_ref().is_none_or(ValidLeaves::check_leaves)
    }
}
impl<T: ValidLeaves> ValidLeaves for Vec<T> {
    fn check_leaves(&self) -> bool {
        self.len() <= 256 && self.iter().all(ValidLeaves::check_leaves)
    }
}
impl<T: ValidLeaves, const N: usize> ValidLeaves for [T; N] {
    fn check_leaves(&self) -> bool {
        self.iter().all(ValidLeaves::check_leaves)
    }
}
impl<T: ValidLeaves> ValidLeaves for Observation<T> {
    fn check_leaves(&self) -> bool {
        match self {
            Self::Available { value, confidence } => {
                value.check_leaves()
                    && confidence.is_none_or(|c| c.is_finite() && (0. ..=1.).contains(&c))
            }
            Self::Unavailable { reason }
            | Self::NotApplicable { reason }
            | Self::Failed { reason } => !reason.is_empty() && reason.check_leaves(),
        }
    }
}
