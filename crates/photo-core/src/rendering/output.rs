use super::{
    internal, io_error,
    pixels::{linear_to_srgb, FloatImage},
};
use image::{ExtendedColorType, ImageEncoder};
use photo_contracts::*;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

pub fn encode_new(
    path: &Path,
    image: &FloatImage,
    format: OutputFormat,
    quality: u8,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(io_error)?;
    let result = (|| {
        let mut writer = BufWriter::new(file);
        let profile = lcms2::Profile::new_srgb().icc().map_err(internal)?;
        match format {
            OutputFormat::Jpeg => {
                let mut rgb = Vec::new();
                rgb.try_reserve_exact(image.pixels.len() * 3).map_err(|_| {
                    ProcessingError::new(
                        ProcessingErrorCode::InsufficientMemory,
                        "Unable to allocate JPEG pixels",
                    )
                })?;
                for row in image.pixels.chunks(image.width as usize) {
                    cancel.check()?;
                    for p in row {
                        rgb.extend(p.map(|v| (linear_to_srgb(v) * 255.).round() as u8));
                    }
                }
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, quality);
                encoder.set_icc_profile(profile).map_err(internal)?;
                encoder
                    .encode(&rgb, image.width, image.height, ExtendedColorType::Rgb8)
                    .map_err(|e| {
                        ProcessingError::new(ProcessingErrorCode::ExportFailed, e.to_string())
                    })?;
            }
            OutputFormat::Tiff => {
                let mut encoder = tiff::encoder::TiffEncoder::new(&mut writer).map_err(internal)?;
                let mut output = encoder
                    .new_image::<tiff::encoder::colortype::RGB16>(image.width, image.height)
                    .map_err(internal)?;
                output
                    .encoder()
                    .write_tag(tiff::tags::Tag::IccProfile, profile.as_slice())
                    .map_err(internal)?;
                output
                    .encoder()
                    .write_tag(tiff::tags::Tag::Orientation, 1u16)
                    .map_err(internal)?;
                output.rows_per_strip(32).map_err(internal)?;
                let mut offset = 0;
                while output.next_strip_sample_count() > 0 {
                    cancel.check()?;
                    let count = output.next_strip_sample_count() as usize;
                    let samples: Vec<u16> = image.pixels[offset..offset + count / 3]
                        .iter()
                        .flat_map(|p| p.map(|v| (linear_to_srgb(v) * 65535.).round() as u16))
                        .collect();
                    output.write_strip(&samples).map_err(|e| {
                        ProcessingError::new(ProcessingErrorCode::ExportFailed, e.to_string())
                    })?;
                    offset += count / 3;
                }
                output.finish().map_err(internal)?;
            }
        }
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)?;
        cancel.check()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}
/// Atomically creates a new directory entry. No check-then-overwrite rename race.
/// The temporary file must be on the destination filesystem.
pub fn publish_unique(
    temp: tempfile::NamedTempFile,
    folder: &Path,
    source: &Path,
    format: OutputFormat,
) -> ProcessingResult<PathBuf> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("photo");
    let safe: String = stem
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(100)
        .collect();
    let safe = safe.trim_matches([' ', '.']);
    let safe = if safe.is_empty() { "photo" } else { safe };
    let mut temp = temp;
    for n in 1..=10000 {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!("-{n}")
        };
        let path = folder.join(format!("{safe}-edited{suffix}.{}", format.extension()));
        match temp.persist_noclobber(&path) {
            Ok(file) => {
                file.sync_all().map_err(io_error)?;
                return Ok(path);
            }
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => temp = e.file,
            Err(e) => return Err(io_error(e.error)),
        }
    }
    Err(ProcessingError::new(
        ProcessingErrorCode::ExportFailed,
        "Too many filename collisions",
    ))
}
pub fn copy_to_publishable(
    path: &Path,
    folder: &Path,
) -> ProcessingResult<tempfile::NamedTempFile> {
    let mut temp = tempfile::NamedTempFile::new_in(folder).map_err(io_error)?;
    std::io::copy(&mut File::open(path).map_err(io_error)?, &mut temp).map_err(io_error)?;
    temp.as_file().sync_all().map_err(io_error)?;
    Ok(temp)
}
