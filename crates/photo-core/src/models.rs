use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::warnings::{IngestionWarning, WarningSummary};
pub use photo_contracts::formats::FileType;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<u32>,
    pub shutter_speed: Option<String>,
    pub aperture: Option<String>,
    pub focal_length: Option<String>,
    pub focus_distance: Option<String>,
    /// EXIF local camera time, not assumed to be UTC.
    pub capture_timestamp: Option<String>,
    pub orientation: Option<u32>,
    pub lens_make: Option<String>,
    pub exposure_compensation: Option<String>,
    pub color_space: Option<String>,
    pub color_profile: Option<String>,
    pub raw_width: Option<u32>,
    pub raw_height: Option<u32>,
    pub camera_white_balance: Option<String>,
    pub bit_depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub asset_count: u64,
    pub warning_count: u64,
    pub last_error: Option<String>,
    pub warnings: WarningSummary,
}

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub id: String,
    pub original_path: PathBuf,
    pub filename: String,
    pub file_type: FileType,
    pub file_size: u64,
    pub modified_at: Option<String>,
    pub fingerprint: String,
    pub discovery_warning: Option<IngestionWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub job_id: String,
    pub original_path: PathBuf,
    pub filename: String,
    pub file_type: FileType,
    pub file_size: u64,
    pub modified_at: Option<String>,
    pub fingerprint: String,
    pub metadata: ImageMetadata,
    pub thumbnail_path: Option<PathBuf>,
    pub preview_status: String,
    pub metadata_warning: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub warnings: Vec<IngestionWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub name: String,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
}
