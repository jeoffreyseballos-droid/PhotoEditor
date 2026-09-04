use crate::{
    error::{AppError, AppResult, ErrorCode},
    models::{DiscoveredFile, FileType},
    warnings::{IngestionWarning, WarningCategory},
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use walkdir::{IntoIter, WalkDir};

pub fn supported_type(path: &Path) -> Option<FileType> {
    photo_contracts::formats::photo_format(path)
        .filter(|format| format.discoverable)
        .map(|format| format.file_type)
}

pub fn canonical_directory(path: &Path) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Choose an absolute folder path.",
        ));
    }
    let canonical = fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Choose an existing folder, not a file.",
        ));
    }
    // JSON/SQLite paths are UTF-8. Reject instead of silently corrupting a non-Unicode path.
    if canonical.to_str().is_none() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "This folder name cannot be represented as Unicode.",
        ));
    }
    Ok(canonical)
}

/// Path identity, not image-content identity. Stable across rescans/restarts on this machine.
/// Renames change the ID; separate hard links are intentionally separate path assets.
pub fn stable_asset_id(canonical_path: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(canonical_path.as_os_str().as_encoded_bytes())
    )
}

pub fn inspect_file(path: &Path) -> AppResult<DiscoveredFile> {
    let original_path = fs::canonicalize(path)?;
    let file_type = supported_type(&original_path).ok_or_else(|| {
        AppError::new(ErrorCode::InvalidInput, "This file type is not supported.")
    })?;
    let filename = original_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidInput,
                "This filename cannot be represented as Unicode.",
            )
        })?
        .to_owned();
    if original_path.to_str().is_none() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "This file path cannot be represented as Unicode.",
        ));
    }
    let metadata = fs::metadata(&original_path)?;
    if !metadata.is_file() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "The image is no longer a regular file.",
        ));
    }
    let modified = metadata.modified().ok();
    let modified_at = modified.map(|time| DateTime::<Utc>::from(time).to_rfc3339());
    let stamp = modified
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|time| time.as_nanos().to_string())
        .unwrap_or_else(|| "unknown".into());
    let id = stable_asset_id(&original_path);
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(format!("{id}:{}:{stamp}", metadata.len()))
    );
    Ok(DiscoveredFile {
        id,
        original_path,
        filename,
        file_type,
        file_size: metadata.len(),
        modified_at,
        fingerprint,
        discovery_warning: None,
    })
}

/// Streaming traversal. Symlinks (including directory junctions) are not followed.
/// Only path IDs, never file bytes, are retained for deduplication.
pub struct FileDiscovery {
    entries: IntoIter,
    seen: HashSet<String>,
    excluded: Vec<PathBuf>,
}

impl FileDiscovery {
    pub fn new(root: &Path, excluded: Vec<PathBuf>) -> AppResult<Self> {
        let root = canonical_directory(root)?;
        Ok(Self {
            entries: WalkDir::new(root)
                .follow_links(false)
                .max_open(16)
                .into_iter(),
            seen: HashSet::new(),
            excluded,
        })
    }
}

impl Iterator for FileDiscovery {
    type Item = Result<DiscoveredFile, IngestionWarning>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = match self.entries.next()? {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(target: "scanning", error = %error, "Skipping inaccessible entry");
                    let category = if error
                        .io_error()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
                    {
                        WarningCategory::Access
                    } else {
                        WarningCategory::Traversal
                    };
                    let mut warning = IngestionWarning::new(
                        category,
                        "directory_traversal",
                        format!("Could not inspect this entry: {error}"),
                    );
                    warning.path = error
                        .path()
                        .filter(|p| p.to_str().is_some())
                        .map(Path::to_path_buf);
                    return Some(Err(warning));
                }
            };
            if self
                .excluded
                .iter()
                .any(|path| crate::paths::same_or_descendant(entry.path(), path))
            {
                if entry.file_type().is_dir() {
                    self.entries.skip_current_dir();
                }
                continue;
            }
            if !entry.file_type().is_file() || supported_type(entry.path()).is_none() {
                continue;
            }
            // The extension allowlist is necessary, but animated containers are not photos.
            if is_animated(entry.path()).unwrap_or(false) {
                continue;
            }
            match inspect_file(entry.path()) {
                Ok(file) if self.seen.insert(file.id.clone()) => return Some(Ok(file)),
                Ok(_) => continue,
                Err(error) => {
                    let path = entry.path().to_path_buf();
                    if path.to_str().is_none() {
                        return Some(Err(IngestionWarning::new(WarningCategory::Unreadable, "non_unicode_path", "A supported file has a non-Unicode path that cannot be represented by the catalog.")));
                    }
                    let id = stable_asset_id(&path);
                    if !self.seen.insert(id.clone()) {
                        continue;
                    }
                    let warning = IngestionWarning::new(
                        WarningCategory::Unreadable,
                        "file_stat_failed",
                        error.message,
                    )
                    .at(path.clone());
                    return Some(Ok(DiscoveredFile {
                        id: id.clone(),
                        fingerprint: id,
                        filename: entry.file_name().to_string_lossy().into_owned(),
                        file_type: supported_type(&path).unwrap(),
                        original_path: path,
                        file_size: 0,
                        modified_at: None,
                        discovery_warning: Some(warning),
                    }));
                }
            }
        }
    }
}

/// Inspect only container headers; never decode a raster to recognize animation.
fn is_animated(path: &Path) -> std::io::Result<bool> {
    use photo_contracts::formats::FormatFamily;
    use std::io::{Read, Seek, SeekFrom};
    let family = supported_type(path).unwrap().format().family;
    if !matches!(family, FormatFamily::Png | FormatFamily::Heif) {
        return Ok(false);
    }
    let mut source = fs::File::open(path)?;
    let length = source.metadata()?.len();
    if family == FormatFamily::Png {
        let mut signature = [0; 8];
        source.read_exact(&mut signature)?;
        if &signature != b"\x89PNG\r\n\x1a\n" {
            return Ok(false);
        }
        for _ in 0..4096 {
            let mut chunk = [0; 8];
            source.read_exact(&mut chunk)?;
            if &chunk[4..] == b"acTL" {
                return Ok(true);
            }
            if matches!(&chunk[4..], b"IDAT" | b"IEND") {
                return Ok(false);
            }
            let size = u32::from_be_bytes(chunk[..4].try_into().unwrap()) as u64;
            let next = source.stream_position()?.saturating_add(size + 4);
            if next > length {
                return Ok(false);
            }
            source.seek(SeekFrom::Start(next))?;
        }
    } else {
        let mut header = [0; 8];
        source.read_exact(&mut header)?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap());
        if &header[4..] == b"ftyp" && (16..=4096).contains(&size) {
            let mut brands = vec![0; size as usize - 8];
            source.read_exact(&mut brands)?;
            return Ok(brands
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .any(|(i, b)| i != 1 && b == b"msf1"));
        }
    }
    Ok(false)
}
