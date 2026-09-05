//! Deterministic, bounded source measurements. Thresholds are documented in PHOTO_ANALYSIS.md.
use crate::rendering::{
    internal,
    masks::SoftMask,
    pixels::{self, FloatImage},
};
use photo_contracts::{analysis::*, CancellationToken, ProcessingResult};

fn unit(v: f64) -> f64 {
    v.clamp(0., 1.)
}
fn percentile(hist: &[u64; 4096], n: u64, q: f64) -> f64 {
    let target = (q * (n - 1) as f64).floor() as u64;
    let mut sum = 0;
    for (i, count) in hist.iter().enumerate() {
        sum += count;
        if sum > target {
            return i as f64 / 4095.;
        }
    }
    1.
}
pub fn exposure_class(median: f64) -> ExposureClass {
    if median < 0.025 {
        ExposureClass::StronglyUnderexposed
    } else if median < 0.10 {
        ExposureClass::Underexposed
    } else if median < 0.55 {
        ExposureClass::Balanced
    } else if median < 0.80 {
        ExposureClass::Overexposed
    } else {
        ExposureClass::StronglyOverexposed
    }
}
pub fn measure(
    image: &FloatImage,
    source: AnalysisSource,
    warnings: Vec<String>,
    cancel: &CancellationToken,
) -> ProcessingResult<CommonAnalysis> {
    if image.width < 16
        || image.height < 16
        || image.width.max(image.height) > 1600
        || image.pixels.len() != (image.width * image.height) as usize
        || image.pixels.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(internal(
            "Analysis requires finite normalized RGB pixels, minimum 16×16, maximum edge 1600",
        ));
    }
    let n = image.pixels.len() as f64;
    let mut hist = [0u64; 4096];
    let mut lum = Vec::with_capacity(image.pixels.len());
    let mut display = Vec::with_capacity(image.pixels.len());
    let mut sum = 0.;
    let mut zones = [0.; 3];
    let mut clips = [0.; 5];
    let mut rgb = [0.; 3];
    let mut families = [0.; 9];
    let mut saturation = [0.; 3];
    let mut chroma = 0.;
    let mut tile_rgb = [[0.; 3]; 9];
    let mut tile_lum = [0.; 9];
    let mut tile_count = [0.; 9];
    for y in 0..image.height {
        cancel.check()?;
        for x in 0..image.width {
            let p = image.pixels[(y * image.width + x) as usize];
            let l = unit(pixels::luma(p) as f64);
            hist[(l * 4095.).round() as usize] += 1;
            lum.push(l);
            sum += l;
            zones[if l < 0.1 {
                0
            } else if l < 0.7 {
                1
            } else {
                2
            }] += 1.;
            for (i, yes) in [
                l <= 0.001,
                l >= 0.99,
                l <= 0.01,
                l >= 0.95,
                p.iter().any(|v| *v >= 0.99),
            ]
            .into_iter()
            .enumerate()
            {
                if yes {
                    clips[i] += 1.;
                }
            }
            let d = p.map(|v| pixels::linear_to_srgb(v) as f64);
            display.push(d);
            let max = d.into_iter().fold(0f64, f64::max);
            let min = d.into_iter().fold(1f64, f64::min);
            let delta = max - min;
            let sat = if max > 0. { delta / max } else { 0. };
            chroma += delta;
            saturation[0] += sat;
            if sat < 0.15 {
                saturation[1] += 1.;
            }
            if sat >= 0.65 {
                saturation[2] += 1.;
            }
            let family = if sat < 0.15 {
                8
            } else {
                let hue = if max == d[0] {
                    ((d[1] - d[2]) / delta).rem_euclid(6.)
                } else if max == d[1] {
                    (d[2] - d[0]) / delta + 2.
                } else {
                    (d[0] - d[1]) / delta + 4.
                } * 60.;
                let centers = [0f64, 30., 60., 120., 180., 240., 275., 315.];
                (0..8)
                    .min_by(|a, b| {
                        let dist = |c: f64| ((hue - c + 180.).rem_euclid(360.) - 180.).abs();
                        dist(centers[*a]).total_cmp(&dist(centers[*b]))
                    })
                    .unwrap_or(0)
            };
            families[family] += 1.;
            let tile = ((y * 3 / image.height) * 3 + x * 3 / image.width) as usize;
            tile_count[tile] += 1.;
            tile_lum[tile] += l;
            for c in 0..3 {
                rgb[c] += d[c];
                tile_rgb[tile][c] += d[c];
            }
        }
    }
    let p = |q| percentile(&hist, n as u64, q);
    let percentiles = LuminancePercentiles {
        p01: p(0.01),
        p05: p(0.05),
        p25: p(0.25),
        p50: p(0.5),
        p75: p(0.75),
        p95: p(0.95),
        p99: p(0.99),
    };
    let median = percentiles.p50;
    let range = percentiles.p95 - percentiles.p05;
    let iqr = percentiles.p75 - percentiles.p25;
    let ev = ((percentiles.p95 + 0.001) / (percentiles.p05 + 0.001)).log2();
    let mean_rgb = rgb.map(|v| v / n);
    let warm = mean_rgb[0] - mean_rgb[2];
    let green = mean_rgb[1] - (mean_rgb[0] + mean_rgb[2]) / 2.;
    let mut variation = 0.;
    for t in 0..9 {
        let r = tile_rgb[t].map(|v| v / tile_count[t]);
        variation +=
            ((r[0] - r[2] - warm).powi(2) + (r[1] - (r[0] + r[2]) / 2. - green).powi(2)) / 9.;
    }
    let brightest = (0..9)
        .max_by(|a, b| (tile_lum[*a] / tile_count[*a]).total_cmp(&(tile_lum[*b] / tile_count[*b])))
        .unwrap_or(4);
    let detail = detail(
        &lum,
        &display,
        image.width as usize,
        image.height as usize,
        cancel,
    )?;
    let reduced = image.reduced(320, cancel)?;
    let horizontal = line(&reduced, false, cancel)?;
    let vertical = line(&reduced, true, cancel)?;
    Ok(CommonAnalysis {
        source,
        exposure: ExposureAnalysis {
            mean_luminance: sum / n,
            median_luminance: median,
            percentiles,
            shadow_fraction: zones[0] / n,
            midtone_fraction: zones[1] / n,
            highlight_fraction: zones[2] / n,
            shadow_clip_fraction: clips[0] / n,
            highlight_clip_fraction: clips[1] / n,
            near_shadow_clip_fraction: clips[2] / n,
            near_highlight_clip_fraction: clips[3] / n,
            any_channel_highlight_clip_fraction: clips[4] / n,
            classification: Observation::inferred(exposure_class(median), 0.45),
        },
        color: ColorAnalysis {
            mean_rgb,
            warm_cool_balance: warm,
            green_magenta_balance: green,
            average_chroma: chroma / n,
            mean_saturation: saturation[0] / n,
            low_saturation_fraction: saturation[1] / n,
            high_saturation_fraction: saturation[2] / n,
            dominant_families: [
                "red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta", "neutral",
            ]
            .into_iter()
            .zip(families)
            .map(|(name, count)| ColorFamily {
                name: name.into(),
                fraction: count / n,
            })
            .collect(),
            spatial_cast_variation: variation.sqrt(),
        },
        dynamic_range: DynamicRangeAnalysis {
            percentile_range: range,
            interquartile_range: iqr,
            percentile_ev_span: ev,
            high_contrast_tendency: Observation::inferred(unit((range - 0.4) / 0.5), 0.5),
            low_contrast_tendency: Observation::inferred(unit(1. - range / 0.25), 0.5),
        },
        detail,
        composition: CompositionAnalysis {
            aspect_ratio: image.width as f64 / image.height as f64,
            orientation: if image.width > image.height {
                "landscape"
            } else if image.width < image.height {
                "portrait"
            } else {
                "square"
            }
            .into(),
            horizontal_line: horizontal,
            vertical_line: vertical,
            horizon: Observation::unavailable(
                "Line evidence alone does not identify a semantic horizon",
            ),
            keystone_indicator: Observation::unavailable(
                "Converging-line geometry is not implemented",
            ),
        },
        scene: SceneAnalysis {
            low_key_tendency: Observation::inferred(unit(zones[0] / n), 0.4),
            high_key_tendency: Observation::inferred(unit(zones[2] / n), 0.4),
            low_light_tendency: Observation::inferred(unit(1. - median / 0.15), 0.25),
            indoor_outdoor: Observation::unavailable(
                "No reliable local scene classifier configured",
            ),
            brightest_region: Point {
                x: ((brightest % 3) as f64 + 0.5) / 3.,
                y: ((brightest / 3) as f64 + 0.5) / 3.,
            },
        },
        warnings,
    })
}
fn detail(
    lum: &[f64],
    rgb: &[[f64; 3]],
    w: usize,
    h: usize,
    cancel: &CancellationToken,
) -> ProcessingResult<DetailAnalysis> {
    let mut strength = 0.;
    let mut lap = 0.;
    let mut noise = [0.; 2];
    let mut flat = 0.;
    let mut grid = [0.; 9];
    let mut counts = [0.; 9];
    for y in 1..h - 1 {
        cancel.check()?;
        for x in 1..w - 1 {
            let i = y * w + x;
            let dx = (lum[i + 1] - lum[i - 1]) / 2.;
            let dy = (lum[i + w] - lum[i - w]) / 2.;
            let e = dx.hypot(dy);
            strength += e;
            let l = 4. * lum[i] - lum[i - 1] - lum[i + 1] - lum[i - w] - lum[i + w];
            lap += l * l;
            let t = (y * 3 / h) * 3 + x * 3 / w;
            grid[t] += e;
            counts[t] += 1.;
            if e < 0.04 {
                let mut avg = [0.; 3];
                let mut mean = 0.;
                for yy in y - 1..=y + 1 {
                    for xx in x - 1..=x + 1 {
                        if yy == y && xx == x {
                            continue;
                        }
                        let j = yy * w + xx;
                        mean += lum[j] / 8.;
                        for (c, a) in avg.iter_mut().enumerate() {
                            *a += rgb[j][c] / 8.;
                        }
                    }
                }
                if (lum[i] - mean).abs() < 0.15 {
                    flat += 1.;
                    noise[0] += (lum[i] - mean).powi(2) / 1.125;
                    let residual = std::array::from_fn::<_, 3, _>(|c| rgb[i][c] - avg[c]);
                    noise[1] += ((residual[0] - residual[1]).powi(2)
                        + (residual[2] - residual[1]).powi(2))
                        / 4.5;
                }
            }
        }
    }
    let count = ((w - 2) * (h - 2)) as f64;
    strength /= count;
    let noise = if flat < 64. || flat / count < 0.05 {
        Observation::unavailable("Too few low-gradient proxy samples for noise estimation")
    } else {
        let l = (noise[0] / flat).sqrt();
        let c = (noise[1] / flat).sqrt();
        Observation::inferred(
            NoiseEstimate {
                luminance_sigma: l,
                chroma_sigma: c,
                severity: unit(l.max(c) / 0.05),
                flat_region_fraction: flat / count,
            },
            (flat / count * 0.65).min(0.65),
        )
    };
    Ok(DetailAnalysis {
        edge_strength: strength,
        laplacian_rms: (lap / count).sqrt(),
        sharpness_grid: std::array::from_fn(|t| {
            if counts[t] > 0. {
                grid[t] / counts[t]
            } else {
                0.
            }
        }),
        blur_likelihood: Observation::unavailable(
            "Low texture and optical blur cannot be reliably separated by global edge energy",
        ),
        motion_blur_likelihood: Observation::unavailable("No directional motion model configured"),
        noise,
    })
}

/// Hough-style straight reference evidence, not a guarantee of a natural horizon.
pub fn line(
    image: &FloatImage,
    vertical: bool,
    cancel: &CancellationToken,
) -> ProcessingResult<Observation<LevelEstimate>> {
    if image.width < 3 || image.height < 3 {
        return Ok(Observation::unavailable("Too few pixels for line evidence"));
    }
    if image.width.max(image.height) > 1600
        || image.pixels.len() != (image.width * image.height) as usize
        || image.pixels.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(internal("Invalid line-analysis pixels"));
    }
    let (w, h) = (image.width as usize, image.height as usize);
    let lum: Vec<f64> = image
        .pixels
        .iter()
        .map(|p| unit(pixels::luma(*p) as f64))
        .collect();
    let mut points = Vec::new();
    for y in 1..h - 1 {
        cancel.check()?;
        for x in 1..w - 1 {
            let i = y * w + x;
            let dx = (lum[i + 1] - lum[i - 1]).abs();
            let dy = (lum[i + w] - lum[i - w]).abs();
            let (along, across) = if vertical { (dx, dy) } else { (dy, dx) };
            if along > 0.12 && along > across * 2. {
                points.push((x as f64, y as f64));
            }
        }
    }
    let span = if vertical { h } else { w } as f64;
    let cross = if vertical { w } else { h } as f64;
    if points.len() < span as usize / 3 {
        return Ok(Observation::unavailable(
            "No sufficiently long straight reference",
        ));
    }
    let mut best = (0u32, 0., 0.);
    for step in -24..=24 {
        cancel.check()?;
        let angle = step as f64 * 0.5;
        let tangent = angle.to_radians().tan();
        let mut bins = vec![0u32; ((w + h) * 2) + 4];
        for &(x, y) in &points {
            let offset = if vertical {
                x + y * tangent
            } else {
                y - x * tangent
            };
            let bin = ((offset + (w + h) as f64) / 2.).round() as usize;
            if let Some(v) = bins.get_mut(bin) {
                *v += 1;
            }
        }
        if let Some((bin, &count)) = bins.iter().enumerate().max_by_key(|(_, n)| **n) {
            if count > best.0 {
                best = (count, angle, bin as f64 * 2. - (w + h) as f64);
            }
        }
    }
    let support = unit(best.0 as f64 / (2. * span));
    let position = if vertical {
        (best.2 - span / 2. * best.1.to_radians().tan()) / cross
    } else {
        (best.2 + span / 2. * best.1.to_radians().tan()) / cross
    };
    if support < 0.35 || !(0.03..=0.97).contains(&position) || best.1.abs() >= 12. {
        return Ok(Observation::unavailable(
            "Straight-line support is weak or at search boundary",
        ));
    }
    Ok(Observation::inferred(
        LevelEstimate {
            angle_degrees: best.1,
            position,
            support_fraction: support,
        },
        (support * 0.85).min(0.85),
    ))
}

pub fn subject(
    image: &FloatImage,
    mask: &SoftMask,
    reference: String,
    cancel: &CancellationToken,
) -> ProcessingResult<SubjectAnalysis> {
    if image.width < 16
        || image.height < 16
        || image.width.max(image.height) > 1600
        || image.pixels.len() != (image.width * image.height) as usize
        || image.pixels.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(internal("Invalid subject-analysis pixels"));
    }
    mask.clone().validated()?;
    let mut weight = [0.; 2];
    let mut sum = [0.; 2];
    let mut sq = [0.; 2];
    let mut rgb = [[0.; 3]; 2];
    let mut edge = [0.; 2];
    let (mut xmin, mut ymin, mut xmax, mut ymax) = (1f64, 1f64, 0f64, 0f64);
    let mut center = [0.; 2];
    for y in 0..image.height {
        cancel.check()?;
        for x in 0..image.width {
            let u = (x as f64 + 0.5) / image.width as f64;
            let v = (y as f64 + 0.5) / image.height as f64;
            let alpha = mask.sample(u as f32, v as f32) as f64;
            if alpha >= 0.5 {
                xmin = xmin.min(x as f64 / image.width as f64);
                xmax = xmax.max((x + 1) as f64 / image.width as f64);
                ymin = ymin.min(y as f64 / image.height as f64);
                ymax = ymax.max((y + 1) as f64 / image.height as f64);
            }
            center[0] += u * alpha;
            center[1] += v * alpha;
            let i = (y * image.width + x) as usize;
            let p = image.pixels[i];
            let l = unit(pixels::luma(p) as f64);
            let e = if x > 0 && y > 0 {
                ((l - pixels::luma(image.pixels[i - 1]) as f64).abs()
                    + (l - pixels::luma(image.pixels[i - image.width as usize]) as f64).abs())
                    / 2.
            } else {
                0.
            };
            for (r, a) in [alpha, 1. - alpha].into_iter().enumerate() {
                weight[r] += a;
                sum[r] += l * a;
                sq[r] += l * l * a;
                edge[r] += e * a;
                for (c, v) in p.into_iter().enumerate() {
                    rgb[r][c] += pixels::linear_to_srgb(v) as f64 * a;
                }
            }
        }
    }
    let area = weight[0] / image.pixels.len() as f64;
    let faces = Observation::unavailable(
        "Face detector not installed; MODNet alpha cannot count or locate faces",
    );
    let count = Observation::unavailable("Alpha matte is not an instance detector");
    if area < 0.01 || area > 0.99 || xmax <= xmin || ymax <= ymin {
        return Ok(SubjectAnalysis {
            subject_present: if area < 0.01 {
                Observation::measured(false)
            } else {
                Observation::unavailable("Nearly full/ambiguous alpha matte")
            },
            measurements: Observation::unavailable(
                "Insufficient separable subject/background area",
            ),
            subject_count: count,
            faces,
        });
    }
    let centroid = Point {
        x: center[0] / weight[0],
        y: center[1] / weight[0],
    };
    let geometry = SubjectGeometry {
        bbox: BoundingBox {
            x: xmin,
            y: ymin,
            width: xmax - xmin,
            height: ymax - ymin,
        },
        center_distance: ((centroid.x - 0.5).hypot(centroid.y - 0.5) * 2f64.sqrt()).min(1.),
        centroid,
        area_fraction: area,
        top_margin: ymin,
        edge_proximity: xmin.min(ymin).min(1. - xmax).min(1. - ymax),
    };
    let region = |r: usize| RegionMeasurements {
        mean_luminance: sum[r] / weight[r],
        luminance_stddev: (sq[r] / weight[r] - (sum[r] / weight[r]).powi(2))
            .max(0.)
            .sqrt(),
        mean_rgb: rgb[r].map(|v| v / weight[r]),
        edge_strength: edge[r] / weight[r],
    };
    let subject = region(0);
    let background = region(1);
    let ev = ((subject.mean_luminance + 0.001) / (background.mean_luminance + 0.001)).log2();
    Ok(SubjectAnalysis {
        subject_present: Observation::measured(true),
        measurements: Observation::measured(SubjectMeasurements {
            geometry,
            subject,
            background,
            subject_background_ev_difference: ev,
            mask_reference: reference,
        }),
        subject_count: count,
        faces,
    })
}
