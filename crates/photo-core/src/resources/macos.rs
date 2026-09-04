use photo_contracts::GpuInfo;
pub(super) fn detect() -> Result<Vec<GpuInfo>, String> {
    Ok(metal::Device::all()
        .into_iter()
        .map(|device| {
            let unified = device.has_unified_memory();
            GpuInfo {
                vendor: device.name().contains("Apple").then(|| "Apple".into()),
                model: device.name().into(),
                device_type: if unified || device.is_low_power() {
                    "integrated"
                } else {
                    "discrete"
                }
                .into(),
                memory_model: if unified { "unified" } else { "unknown" }.into(),
                dedicated_vram_bytes: None,
                shared_memory_budget_bytes: Some(device.recommended_max_working_set_size()),
                graphics_api: Some("Metal".into()),
                compute_capability: Some(
                    "Metal device available; Core ML / AI runtimes not probed".into(),
                ),
                detection_source: "Metal device API".into(),
            }
        })
        .collect())
}
