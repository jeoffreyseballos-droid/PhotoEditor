//! Provider-neutral contracts. No desktop, network, renderer, or authentication implementation.
pub mod analysis;
pub mod batch_context;
pub mod culling;
pub mod development;
pub mod formats;
pub use development::*;
pub mod recipe;
pub mod toolkit;
pub mod trained_style;
pub use recipe::*;
use serde::{Deserialize, Serialize};
use std::{future::Future, path::PathBuf, pin::Pin};
pub use toolkit::*;

pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ServiceError>> + Send + 'a>>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("This capability is not implemented yet")]
    Unavailable,
    #[error("The operation was cancelled")]
    Cancelled,
    #[error("The service request failed: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderRequest {
    pub asset_id: String,
    pub original: PathBuf,
    pub adjustments: RenderAdjustments,
    /// Source-derived objective context, not a creative recipe parameter.
    #[serde(default)]
    pub source_metadata: OpticsMetadata,
    pub destination: PathBuf,
    pub output_format: OutputFormat,
    pub preview: bool,
    pub jpeg_quality: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub rendered_image: PathBuf,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<String>,
    pub diagnostics: ToolkitDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub engine_id: String,
    pub recipe_versions: Vec<u32>,
    pub supports_gpu: bool,
    pub supports_remote_execution: bool,
}

/// An implementation may use a CPU/GPU library, a sidecar, or a remote adapter.
/// It receives a recipe, never React or Tauri state. Upload policy belongs to future orchestration.
pub trait ProcessingEngine: Send + Sync {
    fn capabilities(&self) -> EngineCapabilities;
    /// Blocking CPU work; callers must dispatch onto a bounded background worker.
    fn render(
        &self,
        request: &RenderRequest,
        cancel: &CancellationToken,
    ) -> ProcessingResult<RenderResult>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageProxy {
    pub asset_id: String,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub color_space: String,
}

/// Reserved provider API uses the authoritative Phase 4 contract, not an untyped dictionary.
pub type ImageAnalysis = analysis::PhotoAnalysis;

pub trait ImageAnalyzer: Send + Sync {
    fn analyze(&self, proxy: ImageProxy) -> ServiceFuture<'_, ImageAnalysis>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub user_id: String,
    pub company_id: Option<String>,
}

/// Future adapter boundary for PhotographerApp. No passwords or tokens are implemented here.
pub trait AuthenticationProvider: Send + Sync {
    fn current_identity(&self) -> ServiceFuture<'_, Option<Identity>>;
    fn begin_sign_in(&self) -> ServiceFuture<'_, Identity>;
    fn sign_out(&self) -> ServiceFuture<'_, ()>;
}

/// The future adapter owns protected secret memory and must not implement Debug/Serialize.
/// Persist only through OS-backed Credential Manager/Keychain implementations, never SQLite.
pub trait CredentialStore: Send + Sync {
    type Secret: Send;
    fn retrieve(&self, key: &str) -> Result<Option<Self::Secret>, ServiceError>;
    fn store(&self, key: &str, secret: &Self::Secret) -> Result<(), ServiceError>;
    fn remove(&self, key: &str) -> Result<(), ServiceError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineResources {
    pub logical_cpu_count: usize,
    pub available_ram_bytes: u64,
    pub total_ram_bytes: u64,
    pub gpu_name: Option<String>,
    pub gpu_memory_bytes: Option<u64>,
    pub available_disk_bytes: Option<u64>,
    pub gpus: Vec<GpuInfo>,
    pub gpu_detection: String,
    pub os: String,
    pub architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub vendor: Option<String>,
    pub model: String,
    pub device_type: String,
    pub memory_model: String,
    pub dedicated_vram_bytes: Option<u64>,
    /// DXGI sharing limit / Metal suggested working-set budget, not dedicated or free VRAM.
    pub shared_memory_budget_bytes: Option<u64>,
    pub graphics_api: Option<String>,
    pub compute_capability: Option<String>,
    pub detection_source: String,
}

pub trait GpuProbe: Send + Sync {
    fn detect(&self) -> Result<Vec<GpuInfo>, String>;
}

pub trait ResourceProvider: Send + Sync {
    fn snapshot(&self) -> MachineResources;
}
