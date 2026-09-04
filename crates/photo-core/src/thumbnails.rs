use crate::{metadata::read_exif, models::DiscoveredFile};
use exif::Tag;
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageFormat, ImageReader, Limits};
use std::{
    fs::{self, File},
    io::{BufReader, Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const THUMBNAIL_EDGE: u32 = 384;
pub const CACHE_VERSION: &str = "v2";
pub const MAX_CACHED_BYTES: u64 = 1024 * 1024;
const MAX_PREVIEW_BYTES: u64 = 16 * 1024 * 1024;
const MAX_JPEG_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PIXELS: u64 = 48_000_000;

pub trait EmbeddedPreviewProvider: Send + Sync {
    fn jpeg_preview(&self, path: &Path) -> Result<Option<Vec<u8>>, String>;
}

/// Standard TIFF/EXIF JPEGInterchangeFormat previews only. No proprietary MakerNotes,
/// CR3 container parser, or RAW pixel development. Offset reads are bounded and validated.
pub struct ExifEmbeddedPreview;

impl EmbeddedPreviewProvider for ExifEmbeddedPreview {
    fn jpeg_preview(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        let Ok(exif) = read_exif(path) else {
            return Ok(None);
        };
        let mut source = File::open(path).map_err(|error| error.to_string())?;
        let file_size = source.metadata().map_err(|error| error.to_string())?.len();
        for field in exif
            .fields()
            .filter(|field| field.tag == Tag::JPEGInterchangeFormat)
        {
            let offset = field.value.get_uint(0).map(u64::from);
            let length = exif
                .get_field(Tag::JPEGInterchangeFormatLength, field.ifd_num)
                .and_then(|field| field.value.get_uint(0))
                .map(u64::from);
            if let (Some(offset), Some(length)) = (offset, length) {
                if length == 0
                    || length > MAX_PREVIEW_BYTES
                    || offset.checked_add(length).is_none_or(|end| end > file_size)
                {
                    continue;
                }
                source
                    .seek(SeekFrom::Start(offset))
                    .map_err(|error| error.to_string())?;
                let mut bytes = vec![0; length as usize];
                source
                    .read_exact(&mut bytes)
                    .map_err(|error| error.to_string())?;
                if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                    return Ok(Some(bytes));
                }
            }
        }
        Ok(None)
    }
}

pub struct ThumbnailResult {
    pub path: Option<PathBuf>,
    pub status: &'static str,
    pub warning: Option<String>,
}

pub struct ThumbnailService {
    root: PathBuf,
    raw: Box<dyn EmbeddedPreviewProvider>,
}

impl ThumbnailService {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            raw: Box::new(ExifEmbeddedPreview),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn set_preview_provider(&mut self, provider: Box<dyn EmbeddedPreviewProvider>) {
        self.raw = provider;
    }

    pub fn cache_path(&self, file: &DiscoveredFile) -> PathBuf {
        self.root.join(format!(
            "{CACHE_VERSION}-{}-{}.jpg",
            file.id, file.fingerprint
        ))
    }

    pub fn generate(&self, file: &DiscoveredFile, orientation: Option<u32>) -> ThumbnailResult {
        let path = self.cache_path(file);
        if valid_cached_thumbnail(&path) {
            return ThumbnailResult {
                path: Some(path),
                status: "ready",
                warning: None,
            };
        }
        match self.generate_inner(file, orientation, &path) {
            Ok(true) => ThumbnailResult {
                path: Some(path),
                status: "ready",
                warning: None,
            },
            Ok(false) => ThumbnailResult {
                path: None,
                status: "unavailable",
                warning: Some(if matches!(file.file_type.format().family, photo_contracts::formats::FormatFamily::Heif) { "No embedded JPEG preview is available. HEIC/HEIF pixel decoding is not bundled on this platform; the photo and available metadata are retained." } else { "No supported embedded JPEG preview was found. RAW pixel development is not part of ingestion; the photo and available metadata are retained." }.into()),
            },
            Err(error) => {
                tracing::warn!(target: "metadata", asset_id = %file.id, error = %error, "Preview could not be generated");
                ThumbnailResult {
                    path: None,
                    status: "failed",
                    warning: Some(format!("A preview could not be generated. The original is unchanged. {error}")),
                }
            }
        }
    }

    fn generate_inner(
        &self,
        file: &DiscoveredFile,
        orientation: Option<u32>,
        destination: &Path,
    ) -> Result<bool, String> {
        use photo_contracts::formats::FormatFamily;
        let family = file.file_type.format().family;
        let image = if family == FormatFamily::Tiff {
            crate::tiff_preview::thumbnail(&file.original_path, THUMBNAIL_EDGE)?
        } else if matches!(family, FormatFamily::CameraRaw | FormatFamily::Heif) {
            let Some(preview) = self.raw.jpeg_preview(&file.original_path)? else {
                return Ok(false);
            };
            decode_bounded(Cursor::new(preview), ImageFormat::Jpeg)?
        } else {
            // A single bounded JPEG is decoded at a time, never the whole job.
            let source = File::open(&file.original_path).map_err(|error| error.to_string())?;
            if source.metadata().map_err(|error| error.to_string())?.len() > MAX_JPEG_BYTES {
                return Err("Image exceeds the 64 MiB encoded preview budget".into());
            }
            decode_bounded(
                BufReader::new(source),
                if family == FormatFamily::Png {
                    ImageFormat::Png
                } else {
                    ImageFormat::Jpeg
                },
            )?
        };
        let small = image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE);
        let small = orient(small, orientation.unwrap_or(1));
        // Same-directory atomic publication. The cache may be deleted while the app is open.
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(&self.root).map_err(|error| error.to_string())?;
        JpegEncoder::new_with_quality(temporary.as_file_mut(), 82)
            .encode_image(&small)
            .map_err(|error| error.to_string())?;
        temporary
            .persist(destination)
            .map_err(|error| error.to_string())?;
        Ok(true)
    }
}

fn decode_bounded<R: std::io::BufRead + Seek>(
    mut source: R,
    format: ImageFormat,
) -> Result<DynamicImage, String> {
    let (width, height) = ImageReader::with_format(&mut source, format)
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("Image exceeds the 48 megapixel preview budget".into());
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut reader = ImageReader::with_format(source, format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|error| error.to_string())
}

fn orient(image: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

pub fn valid_cached_thumbnail(path: &Path) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.len() > 0 && metadata.len() <= MAX_CACHED_BYTES)
        && image::image_dimensions(path).is_ok_and(|(width, height)| {
            width > 0 && height > 0 && width <= THUMBNAIL_EDGE && height <= THUMBNAIL_EDGE
        })
}
