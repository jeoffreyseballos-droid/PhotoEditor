use photo_contracts::{MachineResources, ResourceProvider};
use photo_core::{
    error::{AppError, AppResult, ErrorCode},
    jobs::JobService,
    models::{Asset, Job, NewJob, Page},
    resources::LocalResources,
};
use std::sync::Arc;
use tauri::State;

pub struct DesktopState(
    pub Arc<JobService>,
    pub Arc<photo_core::development::DevelopmentService>,
);

#[tauri::command]
pub async fn get_development(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentState> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || service.load(&job_id, &asset_id))
        .await
        .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn save_development(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
    adjustments: photo_contracts::RenderAdjustments,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentState> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || service.save(&job_id, &asset_id, &adjustments))
        .await
        .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn render_development(
    state: State<'_, DesktopState>,
    request: photo_core::development::DevelopmentRequest,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentResult> {
    let service = state.1.clone();
    let permit = service.reserve(&request.request_id, request.preview)?;
    tauri::async_runtime::spawn_blocking(move || service.run(request, permit))
        .await
        .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub fn cancel_development(
    state: State<'_, DesktopState>,
    request_id: String,
) -> photo_contracts::ProcessingResult<()> {
    state.1.cancel(&request_id)
}
#[tauri::command]
pub async fn development_mask(
    state: State<'_, DesktopState>,
    request: photo_core::development::MaskRequest,
) -> photo_contracts::ProcessingResult<photo_core::development::MaskResult> {
    let service = state.1.clone();
    let permit = service.reserve(&request.request_id, true)?;
    tauri::async_runtime::spawn_blocking(move || service.mask(request, permit))
        .await
        .map_err(photo_core::rendering::internal)?
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            tracing::error!(target: "application", error = %error, "Background task failed");
            AppError::new(
                ErrorCode::Internal,
                "A background task stopped unexpectedly. Please try again.",
            )
        })?
}

#[tauri::command]
pub async fn list_jobs(
    state: State<'_, DesktopState>,
    offset: u32,
    limit: u32,
) -> AppResult<Page<Job>> {
    let service = state.0.clone();
    blocking(move || service.repository.list_jobs(offset, limit)).await
}

#[tauri::command]
pub async fn get_job(state: State<'_, DesktopState>, job_id: String) -> AppResult<Job> {
    let service = state.0.clone();
    blocking(move || service.repository.get_job(&job_id)).await
}

#[tauri::command]
pub async fn create_job(state: State<'_, DesktopState>, input: NewJob) -> AppResult<Job> {
    let service = state.0.clone();
    let creator = service.clone();
    let (job, permit) = blocking(move || creator.create(input)).await?;
    let id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = service.scan(&id, permit) {
            tracing::error!(target: "scanning", job_id = %id, error = %error, "Job scan failed");
        }
    });
    Ok(job)
}

#[tauri::command]
pub async fn resume_job(state: State<'_, DesktopState>, job_id: String) -> AppResult<Job> {
    let service = state.0.clone();
    let starter = service.clone();
    let (job, permit) = blocking(move || starter.resume(&job_id)).await?;
    let id = job.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = service.scan(&id, permit) {
            tracing::error!(target: "scanning", job_id = %id, error = %error, "Resumed scan failed");
        }
    });
    Ok(job)
}

#[tauri::command]
pub async fn list_assets(
    state: State<'_, DesktopState>,
    job_id: String,
    offset: u32,
    limit: u32,
) -> AppResult<Page<Asset>> {
    let service = state.0.clone();
    blocking(move || service.assets(&job_id, offset, limit)).await
}

#[tauri::command]
pub async fn get_thumbnail(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
) -> AppResult<Option<String>> {
    let service = state.0.clone();
    blocking(move || service.thumbnail_data(&job_id, &asset_id)).await
}

#[tauri::command]
pub async fn machine_resources() -> AppResult<MachineResources> {
    blocking(|| Ok(LocalResources.snapshot())).await
}

#[tauri::command]
pub fn photo_formats() -> Vec<photo_contracts::formats::PhotoFormat> {
    photo_contracts::formats::PHOTO_FORMATS.to_vec()
}

#[tauri::command]
pub async fn list_warnings(
    state: State<'_, DesktopState>,
    job_id: String,
    offset: u32,
    limit: u32,
) -> AppResult<Page<photo_core::warnings::IngestionWarning>> {
    let service = state.0.clone();
    blocking(move || service.repository.warnings(&job_id, offset, limit)).await
}

#[tauri::command]
pub async fn save_recipe(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
    recipe: photo_contracts::EditRecipe,
    expected_generation: u64,
    reason: Option<photo_core::recipes::RevisionReason>,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentState> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| {
            repo.save_recipe(&job_id, &asset_id, &recipe, expected_generation, reason)?;
            service.load(&job_id, &asset_id)
        })
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn render_recipe(
    state: State<'_, DesktopState>,
    request: photo_core::development::RecipeRenderRequest,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentResult> {
    let service = state.1.clone();
    let permit = service.reserve(&request.request_id, request.preview)?;
    tauri::async_runtime::spawn_blocking(move || service.render_recipe(request, permit))
        .await
        .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn recipe_mask(
    state: State<'_, DesktopState>,
    request: photo_core::development::RecipeMaskRequest,
) -> photo_contracts::ProcessingResult<photo_core::development::MaskResult> {
    let service = state.1.clone();
    let permit = service.reserve(&request.request_id, true)?;
    tauri::async_runtime::spawn_blocking(move || service.recipe_mask(request, permit))
        .await
        .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn recipe_history(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
    offset: u32,
    limit: u32,
) -> photo_contracts::ProcessingResult<Vec<photo_core::recipes::RecipeRevision>> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| repo.recipe_history(&job_id, &asset_id, offset, limit))
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn restore_recipe(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
    revision_id: String,
    expected_generation: u64,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentState> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| {
            repo.restore_revision(&job_id, &asset_id, &revision_id, expected_generation)?;
            service.load(&job_id, &asset_id)
        })
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn recipe_diff(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
    revision_id: String,
) -> photo_contracts::ProcessingResult<Vec<photo_contracts::RecipeDifference>> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| repo.recipe_diff(&job_id, &asset_id, &revision_id))
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn export_recipe(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
) -> photo_contracts::ProcessingResult<std::path::PathBuf> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| repo.export_recipe(&job_id, &asset_id))
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn import_recipe(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
    path: std::path::PathBuf,
    expected_generation: u64,
) -> photo_contracts::ProcessingResult<photo_core::development::DevelopmentState> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| {
            repo.import_recipe_file(&job_id, &asset_id, &path, expected_generation)?;
            service.load(&job_id, &asset_id)
        })
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
#[tauri::command]
pub async fn recipe_json(
    state: State<'_, DesktopState>,
    job_id: String,
    asset_id: String,
) -> photo_contracts::ProcessingResult<String> {
    let service = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        service.with_recipes(|repo| {
            let state = repo.get_recipe(&job_id, &asset_id)?;
            if let Some(e) = state.error {
                return Err(e.into());
            }
            Ok(state.recipe.canonical_json()?)
        })
    })
    .await
    .map_err(photo_core::rendering::internal)?
}
