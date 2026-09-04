//! Typed, UI-independent development vocabulary. All additions are neutral for Phase 2 jobs.
use crate::{ProcessingError, ProcessingErrorCode, ProcessingResult, RenderAdjustments};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}
fn identity() -> Vec<CurvePoint> {
    vec![CurvePoint { x: 0., y: 0. }, CurvePoint { x: 1., y: 1. }]
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToneCurve {
    pub master: Vec<CurvePoint>,
    pub red: Vec<CurvePoint>,
    pub green: Vec<CurvePoint>,
    pub blue: Vec<CurvePoint>,
}
impl Default for ToneCurve {
    fn default() -> Self {
        Self {
            master: identity(),
            red: identity(),
            green: identity(),
            blue: identity(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HslBand {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Presence {
    pub texture: f32,
    pub clarity: f32,
    pub dehaze: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sharpening {
    pub amount: f32,
    pub radius: f32,
    pub detail: f32,
    pub masking: f32,
}
impl Default for Sharpening {
    fn default() -> Self {
        Self {
            amount: 0.,
            radius: 1.,
            detail: 25.,
            masking: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NoiseReduction {
    pub luminance: f32,
    pub luminance_detail: f32,
    pub color: f32,
    pub color_detail: f32,
}
impl Default for NoiseReduction {
    fn default() -> Self {
        Self {
            luminance: 0.,
            luminance_detail: 50.,
            color: 0.,
            color_detail: 50.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Detail {
    pub sharpening: Sharpening,
    pub noise: NoiseReduction,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Optics {
    pub enabled: bool,
    pub distortion: bool,
    pub vignette: bool,
    pub chromatic_aberration: bool,
    pub distortion_strength: f32,
    pub vignette_strength: f32,
    /// Explicit manual terms, independent of profile enable. Percent polynomial / edge EV.
    pub manual_distortion: f32,
    pub manual_vignette: f32,
}
impl Default for Optics {
    fn default() -> Self {
        Self {
            enabled: false,
            distortion: true,
            vignette: true,
            chromatic_aberration: true,
            distortion_strength: 1.,
            vignette_strength: 1.,
            manual_distortion: 0.,
            manual_vignette: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Vignette {
    pub amount: f32,
    pub midpoint: f32,
    pub feather: f32,
    pub roundness: f32,
}
impl Default for Vignette {
    fn default() -> Self {
        Self {
            amount: 0.,
            midpoint: 50.,
            feather: 75.,
            roundness: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Effects {
    pub vignette: Vignette,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BatchContext {
    pub scene_cluster_id: Option<String>,
    pub sequence_id: Option<String>,
    pub reference_asset_id: Option<String>,
    pub consistency_note: Option<String>,
}

/// Closed initial kinds, but storage and renderer operate on generic ordered layers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskType {
    Subject,
    Background,
    Custom,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalAdjustments {
    pub exposure_ev: f32,
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub presence: Presence,
    pub detail: Detail,
}
impl Default for LocalAdjustments {
    fn default() -> Self {
        Self {
            exposure_ev: 0.,
            temperature: 6500.,
            tint: 0.,
            contrast: 0.,
            highlights: 0.,
            shadows: 0.,
            whites: 0.,
            blacks: 0.,
            saturation: 0.,
            vibrance: 0.,
            presence: Default::default(),
            detail: Default::default(),
        }
    }
}
impl LocalAdjustments {
    pub fn as_global(&self) -> RenderAdjustments {
        RenderAdjustments {
            exposure_ev: self.exposure_ev,
            temperature: self.temperature,
            tint: self.tint,
            contrast: self.contrast,
            highlights: self.highlights,
            shadows: self.shadows,
            whites: self.whites,
            blacks: self.blacks,
            saturation: self.saturation,
            vibrance: self.vibrance,
            presence: self.presence,
            detail: self.detail,
            ..Default::default()
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAdjustmentLayer {
    pub id: String,
    pub mask_type: MaskType,
    pub enabled: bool,
    pub strength: f32,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub confidence: Option<f32>,
    /// Content identity, never a caller-supplied disk path. None means current source's mask.
    #[serde(default)]
    pub mask_reference: Option<String>,
    #[serde(default)]
    pub adjustments: LocalAdjustments,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OpticsMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    pub focal_length: Option<f32>,
    pub aperture: Option<f32>,
    pub focus_distance: Option<f32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LensMatch {
    ExactMatch,
    ApproximateMatch,
    NoProfile,
    ProfileUnavailable,
    #[default]
    CorrectionDisabled,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LensDiagnostic {
    pub state: LensMatch,
    pub profile: Option<String>,
    pub database_version: Option<String>,
    pub applied: Vec<String>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskStatus {
    Ready,
    Generating,
    #[default]
    Unavailable,
    Failed,
    Unsupported,
    Stale,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MaskDiagnostic {
    pub status: MaskStatus,
    pub reference: Option<String>,
    pub model_version: Option<String>,
    pub cache_path: Option<String>,
    pub width: u32,
    pub height: u32,
    pub confidence: Option<f32>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolkitDiagnostics {
    pub lens: LensDiagnostic,
    pub mask: MaskDiagnostic,
}

fn invalid(message: &str) -> ProcessingError {
    ProcessingError::new(ProcessingErrorCode::InvalidAdjustments, message)
}
fn range(v: f32, lo: f32, hi: f32) -> ProcessingResult<()> {
    if !v.is_finite() || v < lo || v > hi {
        Err(invalid(
            "Toolkit value is nonfinite or outside its documented range",
        ))
    } else {
        Ok(())
    }
}
fn detail(d: &Detail) -> ProcessingResult<()> {
    range(d.sharpening.radius, 0.5, 3.)?;
    for v in [
        d.sharpening.amount,
        d.sharpening.detail,
        d.sharpening.masking,
        d.noise.luminance,
        d.noise.luminance_detail,
        d.noise.color,
        d.noise.color_detail,
    ] {
        range(v, 0., 100.)?;
    }
    Ok(())
}
pub fn validate(a: &RenderAdjustments) -> ProcessingResult<()> {
    if !(1..=2).contains(&a.schema_version) {
        return Err(invalid("Unsupported adjustment schema version"));
    }
    for points in [&a.curve.master, &a.curve.red, &a.curve.green, &a.curve.blue] {
        if !(2..=16).contains(&points.len())
            || points[0].x != 0.
            || points.last().is_none_or(|p| p.x != 1.)
        {
            return Err(invalid(
                "Curve requires 2..16 points with x endpoints 0 and 1",
            ));
        }
        for p in points {
            range(p.x, 0., 1.)?;
            range(p.y, 0., 1.)?;
        }
        if points
            .windows(2)
            .any(|p| p[0].x >= p[1].x || p[0].y > p[1].y)
        {
            return Err(invalid(
                "Curve x must increase strictly and y must not decrease",
            ));
        }
    }
    for b in &a.hsl {
        for v in [b.hue, b.saturation, b.luminance] {
            range(v, -100., 100.)?;
        }
    }
    for v in [a.presence.texture, a.presence.clarity, a.presence.dehaze] {
        range(v, -100., 100.)?;
    }
    detail(&a.detail)?;
    range(a.optics.distortion_strength, 0., 1.)?;
    range(a.optics.vignette_strength, 0., 1.)?;
    range(a.optics.manual_distortion, -100., 100.)?;
    range(a.optics.manual_vignette, -100., 100.)?;
    range(a.effects.vignette.amount, -100., 100.)?;
    range(a.effects.vignette.midpoint, 0., 100.)?;
    range(a.effects.vignette.feather, 1., 100.)?;
    range(a.effects.vignette.roundness, -100., 100.)?;
    if a.local_layers.len() > 8 {
        return Err(invalid("At most eight local layers are allowed"));
    }
    let mut ids = std::collections::BTreeSet::new();
    for layer in &a.local_layers {
        if layer.id.is_empty() || layer.id.len() > 64 || !ids.insert(&layer.id) {
            return Err(invalid("Local layer IDs must be unique and 1..64 bytes"));
        }
        range(layer.strength, 0., 1.)?;
        if let Some(c) = layer.confidence {
            range(c, 0., 1.)?;
        }
        if layer
            .mask_reference
            .as_ref()
            .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid("Mask reference must be a SHA-256 identity"));
        }
        layer.adjustments.as_global().validated()?;
    }
    if let Some(b) = &a.batch_context {
        for s in [
            &b.scene_cluster_id,
            &b.sequence_id,
            &b.reference_asset_id,
            &b.consistency_note,
        ]
        .into_iter()
        .flatten()
        {
            if s.len() > 1024 {
                return Err(invalid("Batch context field exceeds 1024 bytes"));
            }
        }
    }
    Ok(())
}
