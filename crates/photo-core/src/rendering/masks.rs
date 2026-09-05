//! Disposable 16-bit soft masks in oriented, uncorrected source coordinates.
use super::{
    internal, io_error,
    optics::OpticalMap,
    pixels::{self, FloatImage},
    tools,
};
use photo_contracts::*;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
pub const MODEL_VERSION: &str = "modnet-fa2fa546052fba4c08921230a26cc69a333fca12-fp32-cpu-v1";
const PREPROCESS: &str =
    "oriented-linear-to-srgb-rgb-minus1to1-bilinear-short512-max1024-multiple32-v1";
#[derive(Clone, Debug)]
pub struct SoftMask {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}
impl SoftMask {
    pub fn validated(self) -> ProcessingResult<Self> {
        if self.width == 0
            || self.height == 0
            || self.width > 1024
            || self.height > 1024
            || self.values.len() != (self.width * self.height) as usize
            || self
                .values
                .iter()
                .any(|v| !v.is_finite() || *v < 0. || *v > 1.)
        {
            Err(internal("Invalid soft mask dimensions/range"))
        } else {
            Ok(self)
        }
    }
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.width as f32 - 0.5).clamp(0., self.width as f32 - 1.);
        let y = (v * self.height as f32 - 0.5).clamp(0., self.height as f32 - 1.);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |xx, yy| self.values[(yy * self.width + xx) as usize];
        (p(x0, y0) * (1. - fx) + p(x1, y0) * fx) * (1. - fy)
            + (p(x0, y1) * (1. - fx) + p(x1, y1) * fx) * fy
    }
}
pub trait SegmentationProvider: Send + Sync {
    fn version(&self) -> &str;
    fn infer(&self, source: &FloatImage, cancel: &CancellationToken) -> ProcessingResult<SoftMask>;
}
pub struct ModnetProvider {
    pub resources: PathBuf,
    pub scratch: PathBuf,
}
impl SegmentationProvider for ModnetProvider {
    fn version(&self) -> &str {
        MODEL_VERSION
    }
    fn infer(&self, source: &FloatImage, cancel: &CancellationToken) -> ProcessingResult<SoftMask> {
        let helper = self.resources.join(if cfg!(windows) {
            "photo-mask-helper.exe"
        } else {
            "photo-mask-helper"
        });
        let runtime = self.resources.join(if cfg!(windows) {
            "onnxruntime.dll"
        } else {
            "libonnxruntime.dylib"
        });
        let model = self.resources.join("modnet.onnx");
        if !helper.is_file() || !runtime.is_file() || !model.is_file() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::DecoderUnavailable,
                "Local portrait model/runtime is not installed; run prepare:native",
            ));
        }
        std::fs::create_dir_all(&self.scratch).map_err(io_error)?;
        let temp = tempfile::tempdir_in(&self.scratch).map_err(io_error)?;
        let input = temp.path().join("input.f32");
        let output = temp.path().join("alpha.f32");
        let scale = (512. / source.width.min(source.height) as f32)
            .min(1024. / source.width.max(source.height) as f32);
        let w = ((source.width as f32 * scale / 32.).round() as u32 * 32).clamp(32, 1024);
        let h = ((source.height as f32 * scale / 32.).round() as u32 * 32).clamp(32, 1024);
        let mut writer = std::io::BufWriter::new(std::fs::File::create(&input).map_err(io_error)?);
        for y in 0..h {
            cancel.check()?;
            for x in 0..w {
                let p = source.sample(
                    (x as f32 + 0.5) * source.width as f32 / w as f32 - 0.5,
                    (y as f32 + 0.5) * source.height as f32 / h as f32 - 0.5,
                );
                for v in p {
                    writer
                        .write_all(&(pixels::linear_to_srgb(v) * 2. - 1.).to_le_bytes())
                        .map_err(io_error)?;
                }
            }
        }
        writer.flush().map_err(io_error)?;
        drop(writer);
        let request=serde_json::to_vec(&serde_json::json!({"runtime":runtime,"model":model,"input":input,"output":output,"width":w,"height":h})).map_err(internal)?;
        let response = crate::process::output_cancellable(
            &mut Command::new(helper),
            &request,
            4096,
            Duration::from_secs(120),
            cancel,
        );
        cancel.check()?;
        let (_, stderr) = response.map_err(internal)?;
        if !output.is_file() {
            return Err(internal(format!("Portrait inference failed: {stderr}")));
        }
        let expected = (w * h * 4) as usize;
        let mut bytes = Vec::new();
        std::fs::File::open(&output)
            .map_err(io_error)?
            .take(expected as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() != expected {
            return Err(internal("Truncated portrait alpha output"));
        }
        SoftMask {
            width: w,
            height: h,
            values: bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect(),
        }
        .validated()
    }
}
pub fn cache_key(source: &str, decoder: &str, model: &str) -> String {
    let mut hash = Sha256::new();
    for s in [source, decoder, model, PREPROCESS] {
        hash.update((s.len() as u64).to_le_bytes());
        hash.update(s);
    }
    format!("{:x}", hash.finalize())
}
pub struct MaskCache {
    directory: PathBuf,
    provider: Box<dyn SegmentationProvider>,
}
impl MaskCache {
    pub fn provider_version(&self) -> &str {
        self.provider.version()
    }
    pub fn new(directory: PathBuf, provider: Box<dyn SegmentationProvider>) -> Self {
        Self {
            directory,
            provider,
        }
    }
    pub fn key(&self, source: &str, decoder: &str) -> String {
        cache_key(source, decoder, self.provider.version())
    }
    fn diagnostic(&self, key: &str, status: MaskStatus) -> MaskDiagnostic {
        MaskDiagnostic {
            status,
            reference: Some(key.into()),
            model_version: Some(self.provider.version().into()),
            cache_path: Some(
                self.directory
                    .join(format!("{key}.png"))
                    .to_string_lossy()
                    .into(),
            ),
            ..Default::default()
        }
    }
    pub fn load(&self, source: &str, decoder: &str) -> (Option<SoftMask>, MaskDiagnostic) {
        let key = self.key(source, decoder);
        let mut diag = self.diagnostic(&key, MaskStatus::Stale);
        let result = (|| -> ProcessingResult<SoftMask> {
            let meta_path = self.directory.join(format!("{key}.json"));
            if meta_path.metadata().map_err(io_error)?.len() > 64 * 1024 {
                return Err(internal("Mask metadata exceeds size limit"));
            }
            let saved: MaskDiagnostic =
                serde_json::from_slice(&std::fs::read(meta_path).map_err(io_error)?)
                    .map_err(internal)?;
            if saved.reference.as_deref() != Some(&key)
                || saved.model_version.as_deref() != Some(self.provider.version())
            {
                return Err(internal("Mask metadata identity mismatch"));
            }
            let path = self.directory.join(format!("{key}.png"));
            if path.metadata().map_err(io_error)?.len() > 4 * 1024 * 1024 {
                return Err(internal("Mask cache exceeds size limit"));
            }
            let mut reader = image::ImageReader::open(&path).map_err(io_error)?;
            reader.set_format(image::ImageFormat::Png);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(1024);
            limits.max_image_height = Some(1024);
            limits.max_alloc = Some(16 * 1024 * 1024);
            reader.limits(limits);
            let png = reader.decode().map_err(internal)?.to_luma16();
            let (width, height) = png.dimensions();
            let mask = SoftMask {
                width,
                height,
                values: png
                    .into_raw()
                    .into_iter()
                    .map(|v| v as f32 / 65535.)
                    .collect(),
            }
            .validated()?;
            if saved.width != width || saved.height != height {
                return Err(internal("Mask dimensions changed"));
            }
            diag = saved;
            diag.cache_path = Some(path.to_string_lossy().into());
            diag.status = MaskStatus::Ready;
            Ok(mask)
        })();
        match result {
            Ok(mask) => (Some(mask), diag),
            Err(_) => {
                diag.warnings.push(
                    "No valid cached subject mask. Generate masks to enable local development."
                        .into(),
                );
                (None, diag)
            }
        }
    }
    pub fn generate(
        &self,
        source: &str,
        decoder: &str,
        image: &FloatImage,
        cancel: &CancellationToken,
    ) -> ProcessingResult<MaskDiagnostic> {
        let (cached, diag) = self.load(source, decoder);
        if cached.is_some() {
            return Ok(diag);
        }
        let key = self.key(source, decoder);
        let mut diag = self.diagnostic(&key, MaskStatus::Generating);
        let mask = match self.provider.infer(image, cancel) {
            Ok(mask) => mask.validated()?,
            Err(e) => {
                cancel.check()?;
                diag.status = if e.code == ProcessingErrorCode::DecoderUnavailable {
                    MaskStatus::Unavailable
                } else {
                    MaskStatus::Failed
                };
                diag.warnings.push(e.message);
                return Ok(diag);
            }
        };
        cancel.check()?;
        std::fs::create_dir_all(&self.directory).map_err(io_error)?;
        let png: image::ImageBuffer<image::Luma<u16>, Vec<u16>> = image::ImageBuffer::from_raw(
            mask.width,
            mask.height,
            mask.values
                .iter()
                .map(|v| (v * 65535.).round() as u16)
                .collect(),
        )
        .ok_or_else(|| internal("Mask dimensions"))?;
        let temporary = tempfile::NamedTempFile::new_in(&self.directory).map_err(io_error)?;
        image::DynamicImage::ImageLuma16(png)
            .write_to(&mut temporary.as_file(), image::ImageFormat::Png)
            .map_err(internal)?;
        cancel.check()?;
        temporary
            .persist(self.directory.join(format!("{key}.png")))
            .map_err(|e| io_error(e.error))?;
        diag.status = MaskStatus::Ready;
        diag.width = mask.width;
        diag.height = mask.height;
        // MODNet predicts alpha, not calibrated confidence. Never fabricate a confidence score.
        diag.warnings.push("Portrait-only alpha matte; inspect hair, clothing and non-portrait results. Confidence is not provided by this model.".into());
        let coverage = mask.values.iter().sum::<f32>() / mask.values.len() as f32;
        if !(0.01..=0.99).contains(&coverage) {
            diag.warnings
                .push("Mask is nearly empty or full; subject detection may be unsuitable.".into());
        }
        let mut metadata = tempfile::NamedTempFile::new_in(&self.directory).map_err(io_error)?;
        serde_json::to_writer(metadata.as_file_mut(), &diag).map_err(internal)?;
        metadata
            .persist(self.directory.join(format!("{key}.json")))
            .map_err(|e| io_error(e.error))?;
        Ok(diag)
    }
}
pub fn layer_weight(
    mask: &SoftMask,
    map: &OpticalMap,
    layer: &LocalAdjustmentLayer,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> f32 {
    if !layer.enabled || layer.strength == 0. || layer.mask_type == MaskType::Custom {
        return 0.;
    }
    let (sx, sy) = map.source_coordinate(x as f32, y as f32, w, h, 1);
    let (u, v) = ((sx + 0.5) / w as f32, (sy + 0.5) / h as f32);
    if !(0. ..=1.).contains(&u) || !(0. ..=1.).contains(&v) {
        return 0.;
    }
    let mut weight = mask.sample(u, v);
    if layer.mask_type == MaskType::Background {
        weight = 1. - weight;
    }
    if layer.invert {
        weight = 1. - weight;
    }
    weight * layer.strength
}
pub fn apply_layers(
    image: &mut FloatImage,
    layers: &[LocalAdjustmentLayer],
    mask: &SoftMask,
    reference: &str,
    map: &OpticalMap,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    for layer in layers {
        if !layer.enabled
            || layer.strength == 0.
            || layer.mask_type == MaskType::Custom
            || layer
                .mask_reference
                .as_ref()
                .is_some_and(|r| r != reference)
            || layer.adjustments == LocalAdjustments::default()
        {
            continue;
        }
        let mut candidate = image.clone();
        let a = layer.adjustments.as_global();
        pixels::apply(&mut candidate, &a, cancel)?;
        tools::presence(&mut candidate, a.presence, cancel)?;
        tools::detail(&mut candidate, a.detail, cancel)?;
        for y in 0..image.height {
            cancel.check()?;
            for x in 0..image.width {
                let i = (y * image.width + x) as usize;
                let weight = layer_weight(mask, map, layer, x, y, image.width, image.height);
                for c in 0..3 {
                    image.pixels[i][c] += (candidate.pixels[i][c] - image.pixels[i][c]) * weight;
                }
            }
        }
    }
    Ok(())
}
/// Pure debug visualization; no renderer/export parameter can request this color overlay.
pub fn overlay(
    mask: &SoftMask,
    map: &OpticalMap,
    layer: &LocalAdjustmentLayer,
    w: u32,
    h: u32,
    a: &RenderAdjustments,
    cancel: &CancellationToken,
) -> ProcessingResult<Vec<u8>> {
    let mut alpha = FloatImage::blank(w, h, 1600 * 1600)?;
    let mut visible = layer.clone();
    visible.enabled = true;
    visible.strength = 1.;
    for y in 0..h {
        cancel.check()?;
        for x in 0..w {
            let v = layer_weight(mask, map, &visible, x, y, w, h);
            alpha.pixels[(y * w + x) as usize] = [v; 3];
        }
    }
    let alpha = pixels::geometry(alpha, a, 1600 * 1600 * 2, cancel)?.reduced(1600, cancel)?;
    let bytes: Vec<u8> = alpha
        .pixels
        .iter()
        .flat_map(|p| [231, 57, 208, (p[0].clamp(0., 1.) * 150.).round() as u8])
        .collect();
    let png = image::RgbaImage::from_raw(alpha.width, alpha.height, bytes)
        .ok_or_else(|| internal("Overlay dimensions"))?;
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(png)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(internal)?;
    Ok(output.into_inner())
}
pub fn resource_available(directory: &Path) -> bool {
    directory.join("modnet.onnx").is_file()
}
