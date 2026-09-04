use super::{pixels::FloatImage, RenderLimits};
use image::{ImageDecoder, ImageReader};
use lcms2::{CIExyY, CIExyYTRIPLE, Intent, PixelFormat, Profile, ToneCurve, Transform};
use photo_contracts::{
    CancellationToken, ProcessingError, ProcessingErrorCode as Code, ProcessingResult,
};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub struct Decoded {
    pub image: FloatImage,
    pub warnings: Vec<String>,
}
pub trait RawDecoder: Send + Sync {
    fn id(&self) -> &str;
    fn decode(
        &self,
        source: &Path,
        preview: bool,
        limits: RenderLimits,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Decoded>;
}
pub struct LibRawDecoder {
    pub helper: PathBuf,
    pub scratch: PathBuf,
}
impl RawDecoder for LibRawDecoder {
    fn id(&self) -> &str {
        "libraw-0.22.2-ahd-linear16-v1"
    }
    fn decode(
        &self,
        source: &Path,
        preview: bool,
        limits: RenderLimits,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Decoded> {
        cancel.check()?;
        if !self.helper.is_file() {
            return Err(ProcessingError::new(
                Code::DecoderUnavailable,
                "Bundled LibRaw helper is missing; run prepare:native or reinstall",
            ));
        }
        std::fs::create_dir_all(&self.scratch).map_err(super::io_error)?;
        let temp = tempfile::Builder::new()
            .prefix("raw-")
            .tempdir_in(&self.scratch)
            .map_err(super::io_error)?;
        let dest = temp.path().join("linear.rgb16");
        let input=serde_json::to_vec(&serde_json::json!({"source":source,"destination":dest,"half_size":preview,"max_pixels":limits.max_pixels()})).map_err(super::internal)?;
        let mut command = Command::new(&self.helper);
        command.current_dir(temp.path());
        let response = crate::process::output_cancellable(
            &mut command,
            &input,
            16 * 1024,
            Duration::from_secs(300),
            cancel,
        );
        cancel.check()?;
        let (out, err) = response.map_err(|e| ProcessingError::new(Code::DecodeFailed, e))?;
        if !dest.exists() {
            let value = serde_json::from_slice::<serde_json::Value>(&out).unwrap_or_default();
            let code = match value["code"].as_i64() {
                Some(-2 | -8) => Code::UnsupportedRenderFormat,
                Some(-100007 | -100012 | -100013) => Code::InsufficientMemory,
                Some(-100008) => Code::CorruptSource,
                _ => Code::DecodeFailed,
            };
            return Err(ProcessingError::new(
                code,
                format!(
                    "LibRaw: {}. {err}",
                    value["message"]
                        .as_str()
                        .unwrap_or("Decoder did not produce an image")
                ),
            ));
        }
        let mut file = BufReader::new(File::open(&dest).map_err(super::io_error)?);
        let mut header = [0u8; 20];
        file.read_exact(&mut header)
            .map_err(|e| ProcessingError::new(Code::DecodeFailed, e.to_string()))?;
        if &header[..8] != b"PERAW001" {
            return Err(ProcessingError::new(
                Code::DecodeFailed,
                "Invalid RAW transport header",
            ));
        }
        let number = |at| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        let (w, h) = (number(8), number(12));
        if file.get_ref().metadata().map_err(super::io_error)?.len()
            != 20 + u64::from(w) * u64::from(h) * 6
        {
            return Err(ProcessingError::new(
                Code::DecodeFailed,
                "Incomplete RAW transport",
            ));
        }
        let mut image = FloatImage::blank(w, h, limits.max_pixels())?;
        let mut row = vec![0u8; w as usize * 6];
        for pixels in image.pixels.chunks_mut(w as usize) {
            cancel.check()?;
            file.read_exact(&mut row).map_err(super::io_error)?;
            for (p, bytes) in pixels.iter_mut().zip(row.as_chunks::<6>().0) {
                for c in 0..3 {
                    p[c] = u16::from_le_bytes([bytes[c * 2], bytes[c * 2 + 1]]) as f32 / 65535.;
                }
            }
        }
        let mut warnings = vec![];
        if number(16) != 0 {
            warnings.push(format!("LibRaw reported processing warnings: 0x{:x}; inspect camera WB/color and image quality",number(16)));
        }
        Ok(Decoded { image, warnings })
    }
}
fn linear_profile() -> ProcessingResult<Profile> {
    let xy = |x, y| CIExyY { x, y, Y: 1. };
    let curve = ToneCurve::new(1.);
    Profile::new_rgb(
        &xy(0.3127, 0.3290),
        &CIExyYTRIPLE {
            Red: xy(0.64, 0.33),
            Green: xy(0.30, 0.60),
            Blue: xy(0.15, 0.06),
        },
        &[&curve, &curve, &curve],
    )
    .map_err(super::internal)
}
pub fn raster(
    source: &Path,
    limits: RenderLimits,
    cancel: &CancellationToken,
) -> ProcessingResult<Decoded> {
    let mut reader = ImageReader::open(source)
        .map_err(|e| ProcessingError::new(Code::DecodeFailed, e.to_string()))?
        .with_guessed_format()
        .map_err(super::io_error)?;
    let mut allocation = image::Limits::default();
    allocation.max_alloc = Some(limits.memory_bytes / 2);
    reader.limits(allocation);
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let (w, h) = decoder.dimensions();
    limits.check(w, h)?;
    let profile = decoder.icc_profile().map_err(decode_error)?;
    let orientation = decoder.orientation().map_err(decode_error)?;
    cancel.check()?;
    let mut dynamic = image::DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    dynamic.apply_orientation(orientation);
    let rgba = dynamic.into_rgba32f();
    let mut image = FloatImage::blank(rgba.width(), rgba.height(), limits.max_pixels())?;
    let mut alpha = false;
    for (to, from) in image.pixels.iter_mut().zip(rgba.pixels()) {
        *to = [from[0], from[1], from[2]];
        alpha |= from[3] < 1.;
    }
    let mut warnings = vec![];
    if let Some(icc) = profile {
        let source_profile = Profile::new_icc(&icc).map_err(|e| {
            ProcessingError::new(
                Code::DecodeFailed,
                format!("Invalid source ICC profile: {e}"),
            )
        })?;
        if source_profile.color_space() != lcms2::ColorSpaceSignature::RgbData {
            return Err(ProcessingError::new(
                Code::UnsupportedRenderFormat,
                "Only RGB ICC source profiles are supported",
            ));
        }
        let transform: Transform<[f32; 3], [f32; 3]> = Transform::new(
            &source_profile,
            PixelFormat::RGB_FLT,
            &linear_profile()?,
            PixelFormat::RGB_FLT,
            Intent::RelativeColorimetric,
        )
        .map_err(super::internal)?;
        for row in image.pixels.chunks_mut(image.width as usize) {
            cancel.check()?;
            transform.transform_in_place(row);
        }
    } else {
        warnings.push("No embedded RGB ICC profile: assumed sRGB (PNG gamma/chromaticity-only tags are not color-managed)".into());
        for row in image.pixels.chunks_mut(image.width as usize) {
            cancel.check()?;
            for p in row {
                *p = p.map(super::pixels::srgb_to_linear);
            }
        }
    }
    if alpha {
        warnings.push("Transparency composited over white in linear light".into());
        for (p, a) in image.pixels.iter_mut().zip(rgba.pixels()) {
            for channel in p {
                *channel = *channel * a[3] + 1. - a[3];
            }
        }
    }
    if image.pixels.iter().flatten().any(|v| !v.is_finite()) {
        return Err(ProcessingError::new(
            Code::CorruptSource,
            "Source contains non-finite pixel values",
        ));
    }
    Ok(Decoded { image, warnings })
}
fn decode_error(error: image::ImageError) -> ProcessingError {
    let code = match &error {
        image::ImageError::Limits(_) => Code::InsufficientMemory,
        image::ImageError::Unsupported(_) => Code::UnsupportedRenderFormat,
        _ => Code::CorruptSource,
    };
    ProcessingError::new(code, error.to_string())
}
