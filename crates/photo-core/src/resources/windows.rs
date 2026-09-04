use photo_contracts::GpuInfo;
use windows::Win32::Graphics::{
    Direct3D::D3D_FEATURE_LEVEL_11_0,
    Direct3D12::{
        D3D12CreateDevice, ID3D12Device, D3D12_FEATURE_ARCHITECTURE1,
        D3D12_FEATURE_DATA_ARCHITECTURE1,
    },
    Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND},
};

pub(super) fn detect() -> Result<Vec<GpuInfo>, String> {
    // SAFETY: COM interfaces own/release handles; output buffers use exact SDK layouts.
    // No GPU work or user pointers are submitted.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for index in 0..32 {
            let adapter = match factory.EnumAdapters1(index) {
                Ok(a) => a,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(e.to_string()),
            };
            let desc = adapter.GetDesc1().map_err(|e| e.to_string())?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let model = String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .to_owned();
            let mut device: Option<ID3D12Device> = None;
            let d3d12 = D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device).is_ok();
            let mut architecture = D3D12_FEATURE_DATA_ARCHITECTURE1::default();
            let uma = device.and_then(|device| {
                device
                    .CheckFeatureSupport(
                        D3D12_FEATURE_ARCHITECTURE1,
                        (&mut architecture as *mut D3D12_FEATURE_DATA_ARCHITECTURE1).cast(),
                        std::mem::size_of_val(&architecture) as u32,
                    )
                    .ok()
                    .map(|_| architecture.UMA.as_bool())
            });
            let (device_type, memory_model) = super::memory_classification(uma);
            result.push(GpuInfo {
                vendor: Some(match desc.VendorId {
                    0x10de => "NVIDIA".into(),
                    0x1002 | 0x1022 => "AMD".into(),
                    0x8086 => "Intel".into(),
                    _ => format!("PCI {:04X}", desc.VendorId),
                }),
                model,
                device_type: device_type.into(),
                memory_model: memory_model.into(),
                dedicated_vram_bytes: (uma != Some(true) && desc.DedicatedVideoMemory > 0)
                    .then_some(desc.DedicatedVideoMemory as u64),
                shared_memory_budget_bytes: (desc.SharedSystemMemory > 0)
                    .then_some(desc.SharedSystemMemory as u64),
                graphics_api: Some(
                    if d3d12 {
                        "Direct3D 12 (feature level ≥11.0)"
                    } else {
                        "DXGI adapter; Direct3D 12 unavailable"
                    }
                    .into(),
                ),
                compute_capability: d3d12
                    .then(|| "D3D12 device available; AI runtimes not probed".into()),
                detection_source: "DXGI AdapterDesc1 / D3D12 Architecture1".into(),
            });
        }
        Ok(result)
    }
}
