//! Bounded contact-sheet sampling of TIFF's first image. Source size is not a limit:
//! decode only strips/tiles containing sample points, one bounded chunk at a time.
use image::{DynamicImage, Rgb, RgbImage};
use std::{
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};
use tiff::{
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
    ColorType,
};

pub fn thumbnail(path: &Path, edge: u32) -> Result<DynamicImage, String> {
    let mut decoder = Decoder::new(BufReader::new(File::open(path).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?
        .with_limits(Limits::default());
    let (width, height) = decoder.dimensions().map_err(|e| e.to_string())?;
    if width == 0 || height == 0 || edge == 0 {
        return Err("TIFF has invalid dimensions".into());
    }
    if decoder.get_tag_u32(Tag::PlanarConfiguration).unwrap_or(1) != 1 {
        return Err(
            "Planar TIFF preview is not supported yet; metadata and the original remain available."
                .into(),
        );
    }
    let color = decoder.colortype().map_err(|e| e.to_string())?;
    if !matches!(
        color,
        ColorType::RGB(8 | 16 | 32)
            | ColorType::RGBA(8 | 16 | 32)
            | ColorType::Gray(8 | 16 | 32)
            | ColorType::GrayA(8 | 16 | 32)
            | ColorType::CMYK(8 | 16)
    ) {
        return Err(format!(
            "This TIFF color layout ({color:?}) has no preview decoder yet."
        ));
    }
    let channels = color.num_samples() as usize;
    let scale = f64::from(edge) / f64::from(width.max(height));
    let out_width = ((width as f64 * scale.min(1.0)).round() as u32).max(1);
    let out_height = ((height as f64 * scale.min(1.0)).round() as u32).max(1);
    let mut output = RgbImage::new(out_width, out_height);
    let (cw, ch) = decoder.chunk_dimensions();
    if cw == 0 || ch == 0 {
        return Err("TIFF has invalid chunk dimensions".into());
    }
    let across = width.div_ceil(cw);
    let mut samples = BTreeMap::<u32, Vec<(u32, u32, u32, u32)>>::new();
    for y in 0..out_height {
        for x in 0..out_width {
            let sx = (u64::from(x) * u64::from(width) / u64::from(out_width)) as u32;
            let sy = (u64::from(y) * u64::from(height) / u64::from(out_height)) as u32;
            let index = (sy / ch)
                .checked_mul(across)
                .and_then(|i| i.checked_add(sx / cw))
                .ok_or("TIFF chunk index overflow")?;
            samples
                .entry(index)
                .or_default()
                .push((x, y, sx % cw, sy % ch));
        }
    }
    let start = Instant::now();
    for (index, points) in samples {
        if start.elapsed() > Duration::from_secs(30) {
            return Err("TIFF preview exceeded its processing time budget.".into());
        }
        let data = decoder.read_chunk(index).map_err(|e| {
            format!("TIFF chunk could not be decoded within the memory budget: {e}")
        })?;
        let (stride, _) = decoder.chunk_data_dimensions(index);
        for (x, y, sx, sy) in points {
            let offset =
                ((u64::from(sy) * u64::from(stride) + u64::from(sx)) * channels as u64) as usize;
            let sample = |channel: usize| -> Result<u8, String> {
                let i = offset + channel;
                match &data {
                    DecodingResult::U8(v) => v.get(i).copied(),
                    DecodingResult::U16(v) => v.get(i).map(|v| (v >> 8) as u8),
                    DecodingResult::F32(v) => {
                        v.get(i).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                    }
                    _ => None,
                }
                .ok_or_else(|| "TIFF sample layout is unavailable or damaged.".into())
            };
            let rgb = match color {
                ColorType::Gray(_) | ColorType::GrayA(_) => [sample(0)?; 3],
                ColorType::CMYK(_) => {
                    let k = u16::from(sample(3)?);
                    [0, 1, 2]
                        .map(|c| sample(c).map(|v| ((255 - u16::from(v)) * (255 - k) / 255) as u8))
                        .into_iter()
                        .collect::<Result<Vec<_>, _>>()?
                        .try_into()
                        .unwrap()
                }
                _ => [sample(0)?, sample(1)?, sample(2)?],
            };
            output.put_pixel(x, y, Rgb(rgb));
        }
    }
    Ok(DynamicImage::ImageRgb8(output))
}
