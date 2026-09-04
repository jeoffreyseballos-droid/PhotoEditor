//! Bundled ExifTool adapter: camera-container metadata and embedded JPEG extraction only.
//! No RAW development, shell, network access, or user-config evaluation.
use crate::{
    metadata::{BasicMetadataExtractor, MetadataExtractor, MetadataResult},
    models::{DiscoveredFile, ImageMetadata},
    process,
    thumbnails::EmbeddedPreviewProvider,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone)]
pub struct ExifTool {
    root: PathBuf,
}
impl ExifTool {
    /// Only called on a disposable, newly encoded output, never on an asset source.
    pub(crate) fn copy_export_metadata(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(), String> {
        if same_file::is_same_file(source, destination).unwrap_or(true) {
            return Err("Refusing to write source metadata".into());
        }
        let escape = |p: &Path| -> Result<String, String> {
            Ok(p.to_str()
                .ok_or("Non-Unicode metadata path")?
                .replace('\\', "\\\\")
                .replace('\r', "\\r")
                .replace('\n', "\\n")
                .replace('\t', "\\t"))
        };
        let mut command = self.command()?;
        command.args(["-@", "-"]);
        let input=format!("-TagsFromFile\n#[CSTR]{}\n-Make\n-Model\n-LensMake\n-LensModel\n-DateTimeOriginal\n-SubSecTimeOriginal\n-OffsetTimeOriginal\n-ExposureTime\n-FNumber\n-ISO\n-FocalLength\n-ExposureCompensation\n-Orientation#=1\n-ColorSpace#=1\n-overwrite_original\n--\n#[CSTR]{}\n",escape(source)?,escape(destination)?);
        let (out, err) = process::output(
            &mut command,
            input.as_bytes(),
            16 * 1024,
            Duration::from_secs(30),
        )?;
        if !String::from_utf8_lossy(&out).contains("1 image files updated") || !err.is_empty() {
            return Err(if err.is_empty() {
                "Metadata writer did not confirm success".into()
            } else {
                err
            });
        }
        Ok(())
    }
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn command(&self) -> Result<Command, String> {
        #[cfg(windows)]
        let program = self.root.join("bin/exiftool.exe");
        #[cfg(not(windows))]
        let program = self.root.join("bin/exiftool");
        if !program.is_file() {
            return Err("Bundled ExifTool is unavailable. Reinstall the application to restore camera metadata and embedded-preview support.".into());
        }
        #[cfg(windows)]
        let mut command = Command::new(program);
        #[cfg(not(windows))]
        let mut command = {
            let mut c = Command::new("/usr/bin/perl");
            c.arg(program);
            c
        };
        command.args(["-config", "", "-charset", "filename=UTF8"]);
        command
            .env("LC_ALL", "C")
            .env("LC_CTYPE", "C")
            .env("LANG", "C");
        Ok(command)
    }
    fn run(&self, path: &Path, args: &[&str], cap: usize) -> Result<(Vec<u8>, String), String> {
        let mut command = self.command()?;
        // Windows Perl launchers may convert argv through the ANSI code page. A UTF-8
        // argument stream is lossless, including canonical long paths. CSTR escaping
        // prevents embedded newlines/backslashes from becoming additional arguments.
        let path = path.to_str().ok_or("The original path is not Unicode")?;
        let escaped = path
            .replace('\\', "\\\\")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        let input = format!("--\n#[CSTR]{escaped}\n");
        command.args(args).args(["-@", "-"]);
        process::output(&mut command, input.as_bytes(), cap, Duration::from_secs(30))
    }
}

/// Public for fixture-based contract testing independently of the helper executable.
pub fn metadata_from_json(value: &Value) -> ImageMetadata {
    let text = |names: &[&str]| {
        names.iter().find_map(|name| {
            value
                .get(name)
                .and_then(|v| match v {
                    Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .map(|s| s.chars().take(2048).collect())
        })
    };
    let number = |names: &[&str]| {
        names.iter().find_map(|name| {
            value
                .get(name)
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .and_then(|n| u32::try_from(n).ok())
        })
    };
    ImageMetadata {
        width: number(&["ExifImageWidth", "ImageWidth"]),
        height: number(&["ExifImageHeight", "ImageHeight"]),
        camera_make: text(&["Make"]),
        camera_model: text(&["Model"]),
        lens_make: text(&["LensMake"]),
        lens: text(&["LensModel", "LensID", "Lens"]),
        iso: number(&["ISO"]),
        shutter_speed: text(&["ExposureTime"]),
        aperture: text(&["FNumber"]),
        focal_length: text(&["FocalLength"]),
        focus_distance: text(&["FocusDistance", "SubjectDistance"]),
        capture_timestamp: text(&["SubSecDateTimeOriginal", "DateTimeOriginal", "CreateDate"]),
        orientation: number(&["Orientation"]),
        exposure_compensation: text(&["ExposureCompensation"]),
        color_space: text(&["ColorSpace"]),
        color_profile: text(&["ProfileDescription"]),
        raw_width: number(&["RawImageWidth", "SensorWidth"]),
        raw_height: number(&["RawImageHeight", "SensorHeight"]),
        camera_white_balance: text(&["WhiteBalance", "WB_RGGBLevelsAsShot", "WB_RGGBLevels"]),
        bit_depth: number(&["BitsPerSample", "BitDepth"]).or_else(|| {
            text(&["BitsPerSample"])?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        }),
    }
}
impl MetadataExtractor for ExifTool {
    fn extract(&self, file: &DiscoveredFile) -> MetadataResult {
        use photo_contracts::formats::FormatFamily;
        if matches!(
            file.file_type.format().family,
            FormatFamily::Jpeg | FormatFamily::Png
        ) {
            return BasicMetadataExtractor.extract(file);
        }
        let result = self.run(
            &file.original_path,
            &[
                "-json",
                "-s",
                "-ImageWidth",
                "-ImageHeight",
                "-ExifImageWidth",
                "-ExifImageHeight",
                "-Make",
                "-Model",
                "-LensMake",
                "-LensModel",
                "-LensID",
                "-Lens",
                "-ISO",
                "-ExposureTime",
                "-FNumber",
                "-FocalLength",
                "-FocusDistance",
                "-SubjectDistance",
                "-SubSecDateTimeOriginal",
                "-DateTimeOriginal",
                "-CreateDate",
                "-Orientation#",
                "-ExposureCompensation",
                "-ColorSpace",
                "-ProfileDescription",
                "-RawImageWidth",
                "-RawImageHeight",
                "-SensorWidth",
                "-SensorHeight",
                "-WhiteBalance",
                "-WB_RGGBLevelsAsShot",
                "-BitsPerSample",
                "-BitDepth",
                "-Warning",
                "-Error",
            ],
            1024 * 1024,
        );
        match result {
            Ok((bytes, stderr)) => match serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|v| v.as_array()?.first().cloned())
            {
                Some(value) => {
                    let warning = value
                        .get("Error")
                        .or_else(|| value.get("Warning"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| (!stderr.is_empty()).then_some(stderr));
                    MetadataResult {
                        metadata: metadata_from_json(&value),
                        warning,
                    }
                }
                None => {
                    let mut fallback = BasicMetadataExtractor.extract(file);
                    fallback.warning = Some(format!("Camera metadata could not be read. {stderr}"));
                    fallback
                }
            },
            Err(error) => {
                let mut fallback = BasicMetadataExtractor.extract(file);
                fallback.warning = Some(error);
                fallback
            }
        }
    }
}
impl EmbeddedPreviewProvider for ExifTool {
    fn jpeg_preview(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        // ExifTool's composite PreviewImage does not cover all camera families. Extract one
        // candidate at a time; never concatenate several JPEGs or develop the RAW raster.
        let mut last_error = None;
        for tag in [
            "-JpgFromRaw",
            "-PreviewImage",
            "-ThumbnailImage",
            "-OtherImage",
        ] {
            match self.run(path, &["-b", tag], 32 * 1024 * 1024) {
                Ok((bytes, stderr)) => {
                    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                        return Ok(Some(bytes));
                    }
                    if !stderr.is_empty() {
                        last_error = Some(stderr);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        match last_error {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }
}
