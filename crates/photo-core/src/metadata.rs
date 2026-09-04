use crate::models::{DiscoveredFile, ImageMetadata};
use exif::{Exif, In, Tag};
use std::{
    fs::File,
    io::{BufReader, Cursor, Read},
    path::Path,
};

pub const METADATA_BUDGET: u64 = 8 * 1024 * 1024;

pub struct MetadataResult {
    pub metadata: ImageMetadata,
    pub warning: Option<String>,
}

pub trait MetadataExtractor: Send + Sync {
    fn extract(&self, file: &DiscoveredFile) -> MetadataResult;
}

pub struct BasicMetadataExtractor;

/// Crucial: exif's TIFF container reader reads to EOF. Give it only a capped prefix.
/// Out-of-budget offsets produce partial/null metadata, never an unbounded RAW allocation.
pub(crate) fn read_exif(path: &Path) -> Result<Exif, exif::Error> {
    let mut prefix = Vec::new();
    File::open(path)?
        .take(METADATA_BUDGET)
        .read_to_end(&mut prefix)?;
    exif::Reader::new().continue_on_error(true)
        .read_from_container(&mut BufReader::new(Cursor::new(prefix)))
        .or_else(|error| error.distill_partial_result(|errors| {
            tracing::debug!(target: "metadata", count = errors.len(), "Some EXIF fields were omitted");
        }))
}

fn text(exif: &Exif, tag: Tag) -> Option<String> {
    exif.get_field(tag, In::PRIMARY)
        .map(|field| match &field.value {
            exif::Value::Ascii(values) => values
                .iter()
                .map(|value| {
                    String::from_utf8_lossy(value)
                        .trim_matches('\0')
                        .trim()
                        .to_owned()
                })
                .collect::<Vec<_>>()
                .join(" "),
            _ => field.display_value().with_unit(exif).to_string(),
        })
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(2048).collect())
}

fn number(exif: &Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

impl MetadataExtractor for BasicMetadataExtractor {
    fn extract(&self, file: &DiscoveredFile) -> MetadataResult {
        let mut metadata = ImageMetadata::default();
        let mut warning = None;
        match read_exif(&file.original_path) {
            Ok(exif) => {
                metadata = ImageMetadata {
                    width: number(&exif, Tag::PixelXDimension)
                        .or_else(|| number(&exif, Tag::ImageWidth)),
                    height: number(&exif, Tag::PixelYDimension)
                        .or_else(|| number(&exif, Tag::ImageLength)),
                    camera_make: text(&exif, Tag::Make),
                    camera_model: text(&exif, Tag::Model),
                    lens: text(&exif, Tag::LensModel),
                    iso: number(&exif, Tag::PhotographicSensitivity),
                    shutter_speed: text(&exif, Tag::ExposureTime),
                    aperture: text(&exif, Tag::FNumber),
                    focal_length: text(&exif, Tag::FocalLength),
                    focus_distance: text(&exif, Tag::SubjectDistance),
                    capture_timestamp: text(&exif, Tag::DateTimeOriginal)
                        .or_else(|| text(&exif, Tag::DateTime)),
                    orientation: number(&exif, Tag::Orientation),
                    lens_make: text(&exif, Tag::LensMake),
                    exposure_compensation: text(&exif, Tag::ExposureBiasValue),
                    color_space: text(&exif, Tag::ColorSpace),
                    camera_white_balance: text(&exif, Tag::WhiteBalance),
                    bit_depth: number(&exif, Tag::BitsPerSample),
                    ..ImageMetadata::default()
                };
            }
            Err(exif::Error::NotFound(_)) if !file.file_type.is_raw() => {}
            Err(error) => {
                tracing::debug!(target: "metadata", asset_id = %file.id, error = %error, "EXIF unavailable");
                warning = Some("Some metadata is unavailable or outside the bounded reader's supported formats.".into());
            }
        }
        use photo_contracts::formats::FormatFamily;
        let format = match file.file_type.format().family {
            FormatFamily::Jpeg => Some(image::ImageFormat::Jpeg),
            FormatFamily::Png => Some(image::ImageFormat::Png),
            _ => None,
        };
        if let Some(format) = format {
            // Header-only dimensions; no full-resolution pixel buffer.
            let dimensions = File::open(&file.original_path).ok().and_then(|source| {
                let reader = image::ImageReader::with_format(BufReader::new(source), format);
                reader.into_dimensions().ok()
            });
            if let Some((width, height)) = dimensions {
                metadata.width = Some(width);
                metadata.height = Some(height);
            } else {
                warning =
                    Some("The image header could not be read. The file may be damaged.".into());
            }
        }
        if matches!(file.file_type.format().family, FormatFamily::Tiff) {
            match File::open(&file.original_path)
                .map_err(|e| e.to_string())
                .and_then(|f| {
                    tiff::decoder::Decoder::new(BufReader::new(f)).map_err(|e| e.to_string())
                })
                .and_then(|mut d| d.dimensions().map_err(|e| e.to_string()))
            {
                Ok((width, height)) => {
                    metadata.width = Some(width);
                    metadata.height = Some(height);
                }
                Err(error) => warning = Some(format!("TIFF metadata could not be read: {error}")),
            }
        }
        MetadataResult { metadata, warning }
    }
}
