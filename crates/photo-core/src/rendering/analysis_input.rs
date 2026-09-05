//! Read-only access to the same normalized, oriented, unedited development proxy.
use super::*;
pub(crate) fn lock_cancellable<'a, T>(
    mutex: &'a std::sync::Mutex<T>,
    cancel: &CancellationToken,
) -> ProcessingResult<std::sync::MutexGuard<'a, T>> {
    loop {
        cancel.check()?;
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(e)) => return Err(internal(e)),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
        }
    }
}
pub struct AnalysisInput {
    pub image: pixels::FloatImage,
    pub warnings: Vec<String>,
}
impl CpuProcessingEngine {
    pub fn analysis_mask_version(&self) -> &str {
        self.masks
            .as_ref()
            .map(|m| m.provider_version())
            .unwrap_or("none")
    }
    pub fn analysis_input(
        &self,
        path: &Path,
        cancel: &CancellationToken,
    ) -> ProcessingResult<AnalysisInput> {
        cancel.check()?;
        let identity = source_identity(path)?;
        let mut cache = lock_cancellable(&self.worker, cancel)?;
        cancel.check()?;
        if cache.as_ref().is_none_or(|c| c.identity != identity) {
            let format =
                photo_format(path).ok_or_else(|| internal("Unsupported analysis source"))?;
            let decoded = if format.family == FormatFamily::CameraRaw {
                self.raw.decode(path, true, self.limits, cancel)?
            } else {
                decode::raster(path, self.limits, cancel)?
            };
            *cache = Some(CachedSource {
                identity: identity.clone(),
                image: decoded.image.reduced(PREVIEW_EDGE, cancel)?,
                warnings: decoded.warnings,
            });
        }
        if source_identity(path)? != identity {
            return Err(ProcessingError::new(
                ProcessingErrorCode::SourceChanged,
                "Source changed during analysis preparation",
            ));
        }
        let source = cache
            .as_ref()
            .ok_or_else(|| internal("Source cache unavailable"))?;
        Ok(AnalysisInput {
            image: source.image.clone(),
            warnings: source.warnings.clone(),
        })
    }
    /// Never generate a renderer mask here: doing so could activate unresolved recipe layers.
    pub fn analysis_cached_mask(
        &self,
        path: &Path,
    ) -> ProcessingResult<Option<(masks::SoftMask, MaskDiagnostic)>> {
        let identity = source_identity(path)?;
        Ok(self.masks.as_ref().and_then(|cache| {
            let (mask, diag) = cache.load(&identity, self.raw.id());
            mask.map(|m| (m, diag))
        }))
    }
}
