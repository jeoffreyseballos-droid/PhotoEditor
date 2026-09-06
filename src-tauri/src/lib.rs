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
            let analysis=photo_core::analysis::AnalysisService::new(service.repository.clone(),engine.clone(),Some(photo_core::rendering::masks::MaskCache::new(cache.join("analysis-masks-v1"),Box::new(photo_core::rendering::masks::ModnetProvider{resources:app.path().resource_dir()?.join("toolkit"),scratch:cache.join("analysis-mask-scratch")}))));
            let analysis=Arc::new(analysis);
            let culling=photo_core::culling::CullingService::new(service.repository.clone(),analysis.clone(),engine.clone(),Arc::new(photo_core::culling::features::YuNetDetector{toolkit:app.path().resource_dir()?.join("toolkit"),scratch:cache.join("face-scratch")}),Arc::new(photo_core::culling::features::UnavailableEyes));
            let batch_context=Arc::new(photo_core::batch_context::BatchContextService::new(service.repository.clone(),analysis.clone()));
            let trained_styles=Arc::new(photo_core::trained_styles::TrainedStyleService::new(service.repository.clone(),analysis.clone(),batch_context.clone(),&app.path().resource_dir()?.join("styles"))?);
            let development=DevelopmentService::new(service.repository.clone(),engine,cache,Some(ExifTool::new(app.path().resource_dir()?.join("exiftool"))))?;
            app.manage(DesktopState(Arc::new(service),Arc::new(development),analysis,Arc::new(culling),batch_context,trained_styles));
            tracing::info!(target: "application", "Desktop foundation initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::list_jobs, commands::get_job, commands::create_job, commands::resume_job, commands::list_assets, commands::get_thumbnail, commands::machine_resources, commands::photo_formats, commands::list_warnings,commands::get_development,commands::save_development,commands::render_development,commands::cancel_development,commands::development_mask,commands::save_recipe,commands::render_recipe,commands::recipe_mask,commands::recipe_history,commands::restore_recipe,commands::recipe_diff,commands::export_recipe,commands::import_recipe,commands::recipe_json,commands::get_analysis,commands::analyze_asset,commands::cancel_analysis,commands::invalidate_analysis,commands::export_analysis,commands::run_culling,commands::cancel_culling,commands::culling_progress,commands::culling_overview,commands::culling_detail,commands::culling_rating,commands::culling_select_asset,commands::culling_select_assets,commands::culling_select_ratings,commands::run_batch_context,commands::batch_context_state,commands::batch_context_progress,commands::cancel_batch_context,commands::builtin_presets,commands::preset_editing_state,commands::apply_builtin_preset,commands::trained_styles,commands::trained_style_state,commands::apply_trained_style,commands::trained_style_progress,commands::cancel_trained_style])
        .run(tauri::generate_context!())
        .expect("Photo Editor could not start. Check platform prerequisites and local storage permissions.");
}
