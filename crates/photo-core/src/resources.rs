use photo_contracts::{GpuInfo, GpuProbe, MachineResources, ResourceProvider};
use std::panic::{catch_unwind, AssertUnwindSafe};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub struct LocalResources;
pub struct PlatformGpuProbe;
impl GpuProbe for PlatformGpuProbe {
    fn detect(&self) -> Result<Vec<GpuInfo>, String> {
        #[cfg(windows)]
        {
            windows::detect()
        }
        #[cfg(target_os = "macos")]
        {
            macos::detect()
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err("GPU detection is unavailable on this platform".into())
        }
    }
}

pub fn snapshot_with(probe: &dyn GpuProbe) -> MachineResources {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let (mut gpus, detection) = match catch_unwind(AssertUnwindSafe(|| probe.detect())) {
        Ok(Ok(gpus)) if !gpus.is_empty() => (gpus, "detected".to_owned()),
        Ok(Err(error)) => (Vec::new(), error),
        _ => (Vec::new(), "GPU detection unavailable".to_owned()),
    };
    gpus.sort_by_key(|gpu| gpu.device_type != "discrete");
    MachineResources {
        logical_cpu_count: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        available_ram_bytes: system.available_memory(),
        total_ram_bytes: system.total_memory(),
        gpu_name: gpus.first().map(|g| g.model.clone()),
        gpu_memory_bytes: gpus.first().and_then(|g| g.dedicated_vram_bytes),
        available_disk_bytes: None,
        gpus,
        gpu_detection: detection,
        os: sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.into()),
        architecture: std::env::consts::ARCH.into(),
    }
}
impl ResourceProvider for LocalResources {
    fn snapshot(&self) -> MachineResources {
        snapshot_with(&PlatformGpuProbe)
    }
}

/// Classification is based on the API's UMA query, never a vendor name or guessed VRAM.
pub fn memory_classification(uma: Option<bool>) -> (&'static str, &'static str) {
    match uma {
        Some(true) => ("integrated", "shared"),
        Some(false) => ("discrete", "dedicated"),
        None => ("unknown", "unknown"),
    }
}
