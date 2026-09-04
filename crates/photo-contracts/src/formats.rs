//! Single, allow-listed registry for photographic containers. Decoding is not recognition.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileType {
    Cr3,
    Cr2,
    Nef,
    Arw,
    Dng,
    Raf,
    Orf,
    Rw2,
    Pef,
    Jpg,
    Jpeg,
    Tif,
    Tiff,
    Png,
    Heic,
    Heif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatFamily {
    CameraRaw,
    Jpeg,
    Tiff,
    Png,
    Heif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    BuiltIn,
    BundledExiftool,
    Partial,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PhotoFormat {
    pub file_type: FileType,
    pub extension: &'static str,
    pub family: FormatFamily,
    pub discoverable: bool,
    pub metadata_supported: Support,
    pub preview_supported: Support,
    /// Future eligibility is distinct from today's decoder capability.
    pub editable_future: bool,
    pub develop_supported: &'static str,
}

const fn format(
    file_type: FileType,
    extension: &'static str,
    family: FormatFamily,
    metadata_supported: Support,
    preview_supported: Support,
) -> PhotoFormat {
    PhotoFormat {
        file_type,
        extension,
        family,
        discoverable: true,
        metadata_supported,
        preview_supported,
        editable_future: true,
        develop_supported: match family {
            FormatFamily::CameraRaw => "libraw_camera_dependent",
            FormatFamily::Heif => "unavailable",
            _ => "built_in_variant_dependent",
        },
    }
}

pub const PHOTO_FORMATS: &[PhotoFormat] = &[
    format(
        FileType::Cr3,
        "cr3",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Cr2,
        "cr2",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Nef,
        "nef",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Arw,
        "arw",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Dng,
        "dng",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Raf,
        "raf",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Orf,
        "orf",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Rw2,
        "rw2",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Pef,
        "pef",
        FormatFamily::CameraRaw,
        Support::BundledExiftool,
        Support::BundledExiftool,
    ),
    format(
        FileType::Jpg,
        "jpg",
        FormatFamily::Jpeg,
        Support::BuiltIn,
        Support::BuiltIn,
    ),
    format(
        FileType::Jpeg,
        "jpeg",
        FormatFamily::Jpeg,
        Support::BuiltIn,
        Support::BuiltIn,
    ),
    format(
        FileType::Tif,
        "tif",
        FormatFamily::Tiff,
        Support::BuiltIn,
        Support::BuiltIn,
    ),
    format(
        FileType::Tiff,
        "tiff",
        FormatFamily::Tiff,
        Support::BuiltIn,
        Support::BuiltIn,
    ),
    format(
        FileType::Png,
        "png",
        FormatFamily::Png,
        Support::BuiltIn,
        Support::BuiltIn,
    ),
    format(
        FileType::Heic,
        "heic",
        FormatFamily::Heif,
        Support::BundledExiftool,
        Support::Partial,
    ),
    format(
        FileType::Heif,
        "heif",
        FormatFamily::Heif,
        Support::BundledExiftool,
        Support::Partial,
    ),
];

pub fn photo_format(path: &Path) -> Option<&'static PhotoFormat> {
    let extension = path.extension()?.to_str()?;
    PHOTO_FORMATS
        .iter()
        .find(|entry| entry.discoverable && entry.extension.eq_ignore_ascii_case(extension))
}

impl FileType {
    pub fn format(self) -> &'static PhotoFormat {
        PHOTO_FORMATS
            .iter()
            .find(|entry| entry.file_type == self)
            .expect("Every FileType must be registered")
    }
    pub fn extension(self) -> &'static str {
        self.format().extension
    }
    pub fn is_raw(self) -> bool {
        self.format().family == FormatFamily::CameraRaw
    }
}
