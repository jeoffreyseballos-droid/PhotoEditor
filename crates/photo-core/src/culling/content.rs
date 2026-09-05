//! Exact duplicates mean equal complete file bytes, including metadata/container data.
//! The OS generation token is only a cache validator, never the duplicate identity.
use super::digest;
use crate::{
    models::Asset,
    rendering::{internal, io_error},
    repository::JobRepository,
};
use photo_contracts::{
    culling::DuplicateContent, CancellationToken, ProcessingError, ProcessingErrorCode,
    ProcessingResult,
};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
    time::Instant,
};
pub const CONTENT_ALGORITHM: &str = "full-file-sha256-v1";
pub struct ContentHash {
    pub content: DuplicateContent,
    pub stamp: String,
    pub cached: bool,
    pub bytes_hashed: u64,
    pub duration_ms: u64,
}
fn open_source(path: &Path) -> ProcessingResult<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); /* FILE_SHARE_READ: deny concurrent writers/replacement during hashing */
    }
    let f = options.open(path).map_err(io_error)?;
    if !f.metadata().map_err(io_error)?.is_file() {
        return Err(internal("Duplicate hashing requires a regular file"));
    }
    Ok(f)
}
#[cfg(windows)]
fn handle_stamp(file: &File) -> ProcessingResult<String> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FileBasicInfo, FileIdInfo, GetFileInformationByHandleEx, FILE_BASIC_INFO, FILE_ID_INFO,
        },
    };
    let mut basic = FILE_BASIC_INFO::default();
    let mut id = FILE_ID_INFO::default();
    let handle = HANDLE(file.as_raw_handle());
    // SAFETY: live borrowed file handle, correctly sized initialized output structs; no ownership transfer.
    unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
        .map_err(internal)?;
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
        .map_err(internal)?;
    }
    if basic.ChangeTime <= 0 {
        return Err(internal(
            "Filesystem lacks a reliable change token for duplicate caching",
        ));
    }
    Ok(digest(&[
        "windows-file-generation-v1",
        &id.VolumeSerialNumber.to_string(),
        &format!("{:x?}", id.FileId.Identifier),
        &basic.CreationTime.to_string(),
        &basic.LastWriteTime.to_string(),
        &basic.ChangeTime.to_string(),
        &file.metadata().map_err(io_error)?.len().to_string(),
    ]))
}
#[cfg(unix)]
fn handle_stamp(file: &File) -> ProcessingResult<String> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata().map_err(io_error)?;
    Ok(digest(&[
        "unix-file-generation-v1",
        &m.dev().to_string(),
        &m.ino().to_string(),
        &m.len().to_string(),
        &m.mtime().to_string(),
        &m.mtime_nsec().to_string(),
        &m.ctime().to_string(),
        &m.ctime_nsec().to_string(),
    ]))
}
#[cfg(not(any(windows, unix)))]
fn handle_stamp(_file: &File) -> ProcessingResult<String> {
    Err(internal("No file-generation provider for this platform"))
}
pub fn current_stamp(path: &Path) -> ProcessingResult<String> {
    handle_stamp(&open_source(path)?)
}
pub fn identify(
    repo: &JobRepository,
    asset: &Asset,
    force: bool,
    cancel: &CancellationToken,
) -> ProcessingResult<ContentHash> {
    cancel.check()?;
    let started = Instant::now();
    let mut file = open_source(&asset.original_path)?;
    let stamp = handle_stamp(&file)?;
    let length = file.metadata().map_err(io_error)?.len();
    if !force {
        let row:Option<(String,u64)>=repo.connect().map_err(internal)?.query_row("SELECT sha256,byte_length FROM duplicate_content_cache WHERE job_id=?1 AND asset_id=?2 AND file_stamp=?3 AND algorithm=?4",params![asset.job_id,asset.id,stamp,CONTENT_ALGORITHM],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(internal)?;
        if let Some((sha256, byte_length)) = row {
            let content = DuplicateContent {
                sha256,
                byte_length,
            };
            content.validate().map_err(internal)?;
            if byte_length == length
                && handle_stamp(&file)? == stamp
                && current_stamp(&asset.original_path)? == stamp
            {
                cancel.check()?;
                return Ok(ContentHash {
                    content,
                    stamp,
                    cached: true,
                    bytes_hashed: 0,
                    duration_ms: started.elapsed().as_millis() as u64,
                });
            }
        }
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut count = 0u64;
    loop {
        cancel.check()?;
        let n = file.read(&mut buffer).map_err(io_error)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        count += n as u64;
    }
    cancel.check()?;
    if count != length
        || handle_stamp(&file)? != stamp
        || current_stamp(&asset.original_path)? != stamp
    {
        return Err(ProcessingError::new(
            ProcessingErrorCode::SourceChanged,
            "Source changed while computing duplicate content identity",
        ));
    }
    let content = DuplicateContent {
        sha256: format!("{:x}", hash.finalize()),
        byte_length: count,
    };
    let mut db = repo.connect().map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    tx.execute("INSERT INTO duplicate_content_cache(job_id,asset_id,file_stamp,algorithm,sha256,byte_length,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(job_id,asset_id) DO UPDATE SET file_stamp=excluded.file_stamp,algorithm=excluded.algorithm,sha256=excluded.sha256,byte_length=excluded.byte_length,updated_at=excluded.updated_at",params![asset.job_id,asset.id,stamp,CONTENT_ALGORITHM,content.sha256,content.byte_length,chrono::Utc::now().to_rfc3339()]).map_err(internal)?;
    cancel.check()?;
    tx.commit().map_err(internal)?;
    Ok(ContentHash {
        content,
        stamp,
        cached: false,
        bytes_hashed: count,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}
