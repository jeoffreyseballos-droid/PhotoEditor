use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Default for Crop {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            width: 1.,
            height: 1.,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderAdjustments {
    pub schema_version: u32,
    pub curve: crate::ToneCurve,
    pub hsl: [crate::HslBand; 8],
    pub presence: crate::Presence,
    pub detail: crate::Detail,
    pub optics: crate::Optics,
    pub effects: crate::Effects,
    pub local_layers: Vec<crate::LocalAdjustmentLayer>,
    pub batch_context: Option<crate::BatchContext>,
    pub exposure_ev: f32,
    /// Relative to camera/source WB: 6500 is exactly neutral, not measured camera Kelvin.
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub rotation_degrees: f32,
    pub crop: Crop,
    pub sharpening: f32,
    pub noise_reduction: f32,
}
impl Default for RenderAdjustments {
    fn default() -> Self {
        Self {
            schema_version: 2,
            curve: Default::default(),
            hsl: Default::default(),
            presence: Default::default(),
            detail: Default::default(),
            optics: Default::default(),
            effects: Default::default(),
            local_layers: Vec::new(),
            batch_context: None,
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
            rotation_degrees: 0.,
            crop: Crop::default(),
            sharpening: 0.,
            noise_reduction: 0.,
        }
    }
}
impl RenderAdjustments {
    pub fn validated(&self) -> ProcessingResult<Self> {
        crate::toolkit::validate(self)?;
        fn range(name: &str, value: f32, low: f32, high: f32) -> ProcessingResult<()> {
            if !value.is_finite() || value < low || value > high {
                return Err(ProcessingError::new(
                    ProcessingErrorCode::InvalidAdjustments,
                    format!("{name} must be finite and in {low}..{high}"),
                ));
            }
            Ok(())
        }
        range("Exposure", self.exposure_ev, -5., 5.)?;
        range("Temperature", self.temperature, 2000., 12000.)?;
        for (name, value) in [
            ("Tint", self.tint),
            ("Contrast", self.contrast),
            ("Highlights", self.highlights),
            ("Shadows", self.shadows),
            ("Whites", self.whites),
            ("Blacks", self.blacks),
            ("Saturation", self.saturation),
            ("Vibrance", self.vibrance),
        ] {
            range(name, value, -100., 100.)?;
        }
        range("Sharpening", self.sharpening, 0., 100.)?;
        range("Noise reduction", self.noise_reduction, 0., 100.)?;
        range("Rotation", self.rotation_degrees, -36000., 36000.)?;
        for (name, value) in [
            ("Crop x", self.crop.x),
            ("Crop y", self.crop.y),
            ("Crop width", self.crop.width),
            ("Crop height", self.crop.height),
        ] {
            range(name, value, 0., 1.)?;
        }
        if self.crop.width <= 0.
            || self.crop.height <= 0.
            || self.crop.x + self.crop.width > 1.000001
            || self.crop.y + self.crop.height > 1.000001
        {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "Crop must have positive area inside the rotated canvas",
            ));
        }
        let mut result = self.clone();
        result.schema_version = 2;
        result.rotation_degrees = (self.rotation_degrees + 180.).rem_euclid(360.) - 180.;
        if result.rotation_degrees == 0. {
            result.rotation_degrees = 0.;
        }
        Ok(result)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Jpeg,
    Tiff,
}
impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Tiff => "tif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingErrorCode {
    InvalidAdjustments,
    UnsupportedRenderFormat,
    DecoderUnavailable,
    CorruptSource,
    DecodeFailed,
    InsufficientMemory,
    ExportFailed,
    OutputPermissionDenied,
    Cancelled,
    InternalProcessingError,
    SourceChanged,
    Busy,
}
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct ProcessingError {
    pub code: ProcessingErrorCode,
    pub message: String,
}
impl ProcessingError {
    pub fn new(code: ProcessingErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
pub type ProcessingResult<T> = Result<T, ProcessingError>;
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> ProcessingResult<()> {
        if self.is_cancelled() {
            Err(ProcessingError::new(
                ProcessingErrorCode::Cancelled,
                "Rendering cancelled",
            ))
        } else {
            Ok(())
        }
    }
}
