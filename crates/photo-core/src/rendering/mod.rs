//! Deterministic CPU renderer, independent of desktop state and SQLite.
pub mod decode;
pub mod masks;
pub mod optics;
mod output;
pub mod pixels;
pub mod tools;
pub(crate) use output::copy_to_publishable;
pub use output::publish_unique;
use photo_contracts::{
    formats::{photo_format, FormatFamily},
    *,
};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex};
pub const RENDERER_VERSION: &str = "photo-cpu-linear-srgb-v2.1";
pub const PREVIEW_EDGE: u32 = 1600;
#[derive(Clone, Copy)]
pub struct RenderLimits {
    pub memory_bytes: u64,
}
impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 4 * 1024 * 1024 * 1024,
        }
    }
}
impl RenderLimits {
    /// Conservative allowance for decoder + working + geometry/spatial scratch.
    pub fn max_pixels(self) -> u64 {
        self.memory_bytes / 64
    }
    pub fn check(self, w: u32, h: u32) -> ProcessingResult<()> {
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > self.max_pixels() {
            Err(ProcessingError::new(
                ProcessingErrorCode::InsufficientMemory,
                "Dimensions exceed the conservative render memory budget; no pixels were decoded",
            ))
        } else {
            Ok(())
        }
    }
}
pub fn internal(e: impl std::fmt::Display) -> ProcessingError {
    ProcessingError::new(ProcessingErrorCode::InternalProcessingError, e.to_string())
}
pub fn io_error(e: std::io::Error) -> ProcessingError {
    ProcessingError::new(
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            ProcessingErrorCode::OutputPermissionDenied
        } else {
            ProcessingErrorCode::ExportFailed
        },
        e.to_string(),
    )
}
pub fn source_identity(path: &Path) -> ProcessingResult<String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| ProcessingError::new(ProcessingErrorCode::DecodeFailed, e.to_string()))?;
    let m = canonical.metadata().map_err(io_error)?;
    let modified = m
        .modified()
        .map_err(io_error)?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(internal)?
        .as_nanos();
    Ok(format!("{}:{}:{modified}", canonical.display(), m.len()))
}
pub fn preview_key(
    source: &str,
    adjustments: &RenderAdjustments,
    backend: &str,
) -> ProcessingResult<String> {
    let json = serde_json::to_vec(&adjustments.validated()?).map_err(internal)?;
    let mut hash = Sha256::new();
    for part in [
        source.as_bytes(),
        RENDERER_VERSION.as_bytes(),
        backend.as_bytes(),
        b"preview1600-half".as_slice(),
        &json,
    ] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    Ok(format!("{:x}", hash.finalize()))
}
struct CachedSource {
    identity: String,
    image: pixels::FloatImage,
    warnings: Vec<String>,
}
pub struct CpuProcessingEngine {
    raw: Box<dyn decode::RawDecoder>,
    limits: RenderLimits,
    worker: Mutex<Option<CachedSource>>,
    lenses: optics::LensProfileResolver,
    masks: Option<masks::MaskCache>,
}
impl CpuProcessingEngine {
    pub fn new(raw: Box<dyn decode::RawDecoder>, limits: RenderLimits) -> Self {
        Self {
            raw,
            limits,
            worker: Mutex::new(None),
            lenses: optics::LensProfileResolver::unavailable("Lens database is not configured"),
            masks: None,
        }
    }
    pub fn backend_id(&self) -> &str {
        self.raw.id()
    }
    pub fn optics_diagnostic(
        &self,
        metadata: &OpticsMetadata,
        options: Optics,
        w: u32,
        h: u32,
    ) -> LensDiagnostic {
        self.lenses.resolve(metadata, options, w, h).1
    }
    pub fn cached_mask_status(&self, source: &Path) -> ProcessingResult<MaskDiagnostic> {
        let identity = source_identity(source)?;
        Ok(self
            .masks
            .as_ref()
            .map(|cache| cache.load(&identity, self.raw.id()).1)
            .unwrap_or_default())
    }
    pub fn with_toolkit(
        mut self,
        lenses: optics::LensProfileResolver,
        masks: masks::MaskCache,
    ) -> Self {
        self.lenses = lenses;
        self.masks = Some(masks);
        self
    }
    pub fn mask_preview(
        &self,
        source: &Path,
        metadata: &OpticsMetadata,
        a: &RenderAdjustments,
        layer: Option<&LocalAdjustmentLayer>,
        generate: bool,
        cancel: &CancellationToken,
    ) -> ProcessingResult<(MaskDiagnostic, Option<Vec<u8>>)> {
        let a = a.validated()?;
        cancel.check()?;
        let Some(masks) = &self.masks else {
            return Ok((
                MaskDiagnostic {
                    warnings: vec!["Mask provider unavailable".into()],
                    ..Default::default()
                },
                None,
            ));
        };
        let identity = source_identity(source)?;
        let mut cache = self.worker.lock().map_err(internal)?;
        if cache.as_ref().is_none_or(|c| c.identity != identity) {
            let format = photo_format(source).ok_or_else(|| internal("Unsupported source"))?;
            let decoded = if format.family == FormatFamily::CameraRaw {
                self.raw.decode(source, true, self.limits, cancel)?
            } else {
                decode::raster(source, self.limits, cancel)?
            };
            *cache = Some(CachedSource {
                identity: identity.clone(),
                image: decoded.image.reduced(PREVIEW_EDGE, cancel)?,
                warnings: decoded.warnings,
            });
        }
        let source_image = &cache
            .as_ref()
            .ok_or_else(|| internal("Source cache unavailable"))?
            .image;
        if generate {
            let diag = masks.generate(&identity, self.raw.id(), source_image, cancel)?;
            if diag.status != MaskStatus::Ready {
                return Ok((diag, None));
            }
        }
        let (mask, mut diag) = masks.load(&identity, self.raw.id());
        if source_identity(source)? != identity {
            return Err(ProcessingError::new(
                ProcessingErrorCode::SourceChanged,
                "Source changed during mask generation",
            ));
        }
        if let (Some(mask), Some(layer)) = (mask, layer) {
            if layer.mask_type == MaskType::Custom {
                diag.status = MaskStatus::Unsupported;
                diag.warnings
                    .push("Custom masks are reserved for a future provider".into());
                return Ok((diag, None));
            }
            if layer
                .mask_reference
                .as_ref()
                .is_some_and(|r| Some(r) != diag.reference.as_ref())
            {
                diag.status = MaskStatus::Stale;
                diag.warnings
                    .push("Layer references an older source/model mask".into());
                return Ok((diag, None));
            }
            let (map, _) =
                self.lenses
                    .resolve(metadata, a.optics, source_image.width, source_image.height);
            let overlay = masks::overlay(
                &mask,
                &map,
                layer,
                source_image.width,
                source_image.height,
                &a,
                cancel,
            )?;
            Ok((diag, Some(overlay)))
        } else {
            Ok((diag, None))
        }
    }
}
impl ProcessingEngine for CpuProcessingEngine {
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            engine_id: format!("{RENDERER_VERSION}/{}", self.raw.id()),
            recipe_versions: vec![1, 2],
            supports_gpu: false,
            supports_remote_execution: false,
        }
    }
    fn render(
        &self,
        request: &RenderRequest,
        cancel: &CancellationToken,
    ) -> ProcessingResult<RenderResult> {
        self.render_with_metadata(request, &request.source_metadata, cancel)
    }
}
impl CpuProcessingEngine {
    pub fn render_with_metadata(
        &self,
        request: &RenderRequest,
        metadata: &OpticsMetadata,
        cancel: &CancellationToken,
    ) -> ProcessingResult<RenderResult> {
        cancel.check()?;
        let adjustments = request.adjustments.validated()?;
        if !(1..=100).contains(&request.jpeg_quality) {
            return Err(ProcessingError::new(
                ProcessingErrorCode::InvalidAdjustments,
                "JPEG quality must be 1..100",
            ));
        }
        let format = photo_format(&request.original).ok_or_else(|| {
            ProcessingError::new(
                ProcessingErrorCode::UnsupportedRenderFormat,
                "Unrecognized render format",
            )
        })?;
        if format.family == FormatFamily::Heif {
            return Err(ProcessingError::new(
                ProcessingErrorCode::UnsupportedRenderFormat,
                "HEIC/HEIF development is not available in Phase 2",
            ));
        }
        let identity = source_identity(&request.original)?;
        // Reject accidental source writes at the engine boundary, not only in UI orchestration.
        if request.destination.exists() {
            return Err(ProcessingError::new(
                ProcessingErrorCode::ExportFailed,
                "Render destination already exists",
            ));
        }
        let parent = request
            .destination
            .parent()
            .ok_or_else(|| internal("Missing render destination parent"))?
            .canonicalize()
            .map_err(io_error)?;
        if request.original.canonicalize().map_err(io_error)?
            == parent.join(
                request
                    .destination
                    .file_name()
                    .ok_or_else(|| internal("Missing destination filename"))?,
            )
        {
            return Err(internal("Original sources are immutable"));
        }
        // One decode/render allocation at a time, including independently-created clients.
        let mut cache = self.worker.lock().map_err(internal)?;
        cancel.check()?;
        let cached = cache
            .as_ref()
            .filter(|c| request.preview && c.identity == identity);
        let (mut image, mut warnings) = if let Some(c) = cached {
            (c.image.clone(), c.warnings.clone())
        } else {
            if !request.preview {
                *cache = None;
            }
            let decoded = if format.family == FormatFamily::CameraRaw {
                self.raw
                    .decode(&request.original, request.preview, self.limits, cancel)?
            } else {
                decode::raster(&request.original, self.limits, cancel)?
            };
            let image = if request.preview {
                decoded.image.reduced(PREVIEW_EDGE, cancel)?
            } else {
                decoded.image
            };
            if request.preview {
                *cache = Some(CachedSource {
                    identity: identity.clone(),
                    image: image.clone(),
                    warnings: decoded.warnings.clone(),
                });
            }
            (image, decoded.warnings)
        };
        let (map, lens) =
            self.lenses
                .resolve(metadata, adjustments.optics, image.width, image.height);
        let mut diagnostics = ToolkitDiagnostics {
            lens,
            ..Default::default()
        };
        image = map.apply(image, cancel)?;
        pixels::apply(&mut image, &adjustments, cancel)?;
        tools::color(&mut image, &adjustments, cancel)?;
        tools::presence(&mut image, adjustments.presence, cancel)?;
        if adjustments
            .local_layers
            .iter()
            .any(|l| l.enabled && l.strength > 0.)
        {
            if let Some(masks) = &self.masks {
                let (mask, diag) = masks.load(&identity, self.raw.id());
                diagnostics.mask = diag;
                if let Some(mask) = mask {
                    masks::apply_layers(
                        &mut image,
                        &adjustments.local_layers,
                        &mask,
                        diagnostics.mask.reference.as_deref().unwrap_or(""),
                        &map,
                        cancel,
                    )?;
                }
                for layer in adjustments
                    .local_layers
                    .iter()
                    .filter(|l| l.enabled && l.strength > 0.)
                {
                    if layer.mask_type == MaskType::Custom {
                        diagnostics.mask.warnings.push(format!(
                            "Layer {} skipped: custom provider unsupported",
                            layer.id
                        ));
                    }
                    if layer
                        .mask_reference
                        .as_ref()
                        .is_some_and(|r| Some(r) != diagnostics.mask.reference.as_ref())
                    {
                        diagnostics.mask.status = MaskStatus::Stale;
                        diagnostics
                            .mask
                            .warnings
                            .push(format!("Layer {} skipped: stale mask reference", layer.id));
                    }
                }
            } else {
                diagnostics.mask.warnings.push(
                    "Subject/background adjustments skipped: local mask provider unavailable"
                        .into(),
                );
            }
        }
        tools::detail(&mut image, adjustments.detail, cancel)?;
        image = pixels::geometry(image, &adjustments, self.limits.max_pixels(), cancel)?;
        tools::vignette(&mut image, adjustments.effects.vignette, cancel)?;
        if image.pixels.iter().flatten().any(|v| !v.is_finite()) {
            return Err(internal(
                "Nonfinite creative output; reduce extreme adjustments",
            ));
        }
        if request.preview {
            image = image.reduced(PREVIEW_EDGE, cancel)?;
        }
        cancel.check()?;
        if source_identity(&request.original)? != identity {
            return Err(ProcessingError::new(
                ProcessingErrorCode::SourceChanged,
                "Source changed while rendering; refresh and retry",
            ));
        }
        output::encode_new(
            &request.destination,
            &image,
            request.output_format,
            request.jpeg_quality,
            cancel,
        )?;
        if request.preview {
            warnings.push(
                "Preview uses reduced source data; assess fine detail in full-resolution export"
                    .into(),
            );
        }
        Ok(RenderResult {
            rendered_image: request.destination.clone(),
            width: image.width,
            height: image.height,
            warnings,
            diagnostics,
        })
    }
}
