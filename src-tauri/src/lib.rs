// These are product targets, not an accidental promise of Linux/mobile support.
#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
compile_error!("Photo Editor supports Windows 11 x64 and macOS Apple Silicon only.");

mod commands;

use commands::DesktopState;
use photo_contracts::ResourceProvider;
use photo_core::jobs::JobService;
use photo_core::{
    development::DevelopmentService,
    external::ExifTool,
    rendering::{decode::LibRawDecoder, CpuProcessingEngine, RenderLimits},
    resources::LocalResources,
};
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn run() {
    tauri::Builder::default()
        // Must be registered first. A second process must not mark the first one's scan interrupted.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let log_dir = app.path().app_log_dir()?;
            std::fs::create_dir_all(&log_dir)?;
            let appender = tracing_appender::rolling::Builder::new()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("photo-editor")
                .filename_suffix("jsonl")
                .max_log_files(7)
                .build(log_dir)?;
            let (writer, guard) = tracing_appender::non_blocking(appender);
            tracing_subscriber::registry()
                .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
                .with(tracing_subscriber::fmt::layer().json().with_ansi(false).with_writer(writer))
                .try_init()?;
            app.manage(guard);
            let service = JobService::with_exiftool(app.path().app_local_data_dir()?, app.path().app_cache_dir()?, app.path().resource_dir()?.join("exiftool"))?;
            service.repository.recover_interrupted()?;
            let cache=app.path().app_cache_dir()?.join("develop-v1");
            let helper=app.path().resource_dir()?.join("raw").join(if cfg!(windows){"photo-raw-helper.exe"}else{"photo-raw-helper"});
            let limits=RenderLimits{memory_bytes:(LocalResources.snapshot().available_ram_bytes/2).min(4*1024*1024*1024)};
            let toolkit=app.path().resource_dir()?.join("toolkit");
            let engine=Arc::new(CpuProcessingEngine::new(Box::new(LibRawDecoder{helper,scratch:cache.join("scratch")}),limits).with_toolkit(
                photo_core::rendering::optics::LensProfileResolver::load(&toolkit.join("lensfun-db")),
                photo_core::rendering::masks::MaskCache::new(cache.join("masks-v1"),Box::new(photo_core::rendering::masks::ModnetProvider{resources:toolkit,scratch:cache.join("mask-scratch")}))));
            let development=DevelopmentService::new(service.repository.clone(),engine,cache,Some(ExifTool::new(app.path().resource_dir()?.join("exiftool"))))?;
            app.manage(DesktopState(Arc::new(service),Arc::new(development)));
            tracing::info!(target: "application", "Desktop foundation initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::list_jobs, commands::get_job, commands::create_job, commands::resume_job, commands::list_assets, commands::get_thumbnail, commands::machine_resources, commands::photo_formats, commands::list_warnings,commands::get_development,commands::save_development,commands::render_development,commands::cancel_development,commands::development_mask])
        .run(tauri::generate_context!())
        .expect("Photo Editor could not start. Check platform prerequisites and local storage permissions.");
}
