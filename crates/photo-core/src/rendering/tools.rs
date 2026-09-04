//! Deterministic f32 creative tools. Spatial radii use a 4000-pixel long-edge reference.
use super::pixels::{luma, srgb_to_linear, FloatImage};
use photo_contracts::*;

fn encoded(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
pub fn curve_value(x: f32, points: &[CurvePoint]) -> f32 {
    if points.iter().all(|p| p.x == p.y) {
        return x;
    }
    let i = points
        .partition_point(|p| p.x < x)
        .clamp(1, points.len() - 1);
    let (a, b) = (points[i - 1], points[i]);
    a.y + (x - a.x) * (b.y - a.y) / (b.x - a.x)
}
fn hsv(p: [f32; 3]) -> (f32, f32, f32) {
    let max = p.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let min = p.into_iter().fold(f32::INFINITY, f32::min);
    let d = max - min;
    if d < 1e-7 || max < 1e-7 {
        return (0., 0., max);
    }
    let h = if max == p[0] {
        (p[1] - p[2]) / d
    } else if max == p[1] {
        (p[2] - p[0]) / d + 2.
    } else {
        (p[0] - p[1]) / d + 4.
    };
    ((h * 60.).rem_euclid(360.), d / max, max)
}
fn from_hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.) / 60.;
    let c = v * s;
    let x = c * (1. - (h.rem_euclid(2.) - 1.).abs());
    let m = v - c;
    let p = match h as u32 {
        0 => [c, x, 0.],
        1 => [x, c, 0.],
        2 => [0., c, x],
        3 => [0., x, c],
        4 => [x, 0., c],
        _ => [c, 0., x],
    };
    p.map(|n| n + m)
}
/// Raised-cosine overlapping circular hue weights, normalized to a partition of unity.
pub fn hue_weights(h: f32) -> [f32; 8] {
    let centers = [0., 30., 60., 120., 180., 240., 275., 315.];
    let mut w = centers.map(|c| {
        let d = ((h - c + 180.).rem_euclid(360.) - 180.).abs();
        if d >= 65. {
            0.
        } else {
            (1. + (std::f32::consts::PI * d / 65.).cos()) * 0.5
        }
    });
    let total = w.iter().sum::<f32>();
    for v in &mut w {
        *v /= total;
    }
    w
}
pub fn color(
    image: &mut FloatImage,
    a: &RenderAdjustments,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    let curves = [&a.curve.red, &a.curve.green, &a.curve.blue];
    let has_curve = [&a.curve.master, &a.curve.red, &a.curve.green, &a.curve.blue]
        .iter()
        .any(|ps| ps.iter().any(|p| p.x != p.y));
    let has_hsl = a.hsl.iter().any(|b| *b != HslBand::default());
    if !has_curve && !has_hsl {
        return Ok(());
    }
    for row in image.pixels.chunks_mut(image.width as usize) {
        cancel.check()?;
        for p in row {
            if has_curve {
                for c in 0..3 {
                    p[c] = srgb_to_linear(curve_value(
                        curve_value(encoded(p[c]), &a.curve.master),
                        curves[c],
                    ));
                }
            }
            if has_hsl {
                // Retain signed out-of-gamut residuals; hue is defined on nonnegative RGB.
                let residual = p.map(|v| v.min(0.));
                let e = p.map(|v| encoded(v.max(0.)));
                let (h, s, v) = hsv(e);
                if s > 1e-7 {
                    let weights = hue_weights(h);
                    let mut delta = [0.; 3];
                    for (b, w) in a.hsl.iter().zip(weights) {
                        delta[0] += b.hue * w * 0.3;
                        delta[1] += b.saturation * w / 100.;
                        delta[2] += b.luminance * w / 100.;
                    }
                    let rgb = from_hsv(h + delta[0], (s * (1. + delta[1])).clamp(0., 1.), v);
                    *p = std::array::from_fn(|c| {
                        srgb_to_linear(rgb[c]) * delta[2].exp2() + residual[c]
                    });
                }
            }
        }
    }
    Ok(())
}
fn radius(image: &FloatImage, r: f32) -> usize {
    (r * image.width.max(image.height) as f32 / 4000.)
        .round()
        .max(1.) as usize
}
/// Edge-replicated separable box mean, O(pixels), never O(radius * pixels).
fn blur(
    src: &[f32],
    w: usize,
    h: usize,
    r: usize,
    cancel: &CancellationToken,
) -> ProcessingResult<Vec<f32>> {
    let mut tmp = vec![0.; src.len()];
    let mut out = vec![0.; src.len()];
    for y in 0..h {
        cancel.check()?;
        let row = &src[y * w..(y + 1) * w];
        let mut sum = 0f64;
        for k in -(r as isize)..=r as isize {
            sum += row[k.clamp(0, w as isize - 1) as usize] as f64;
        }
        for x in 0..w {
            tmp[y * w + x] = (sum / (2 * r + 1) as f64) as f32;
            let left = (x as isize - r as isize).clamp(0, w as isize - 1) as usize;
            let right = (x + r + 1).min(w - 1);
            sum += row[right] as f64 - row[left] as f64;
        }
    }
    for x in 0..w {
        cancel.check()?;
        let mut sum = 0f64;
        for k in -(r as isize)..=r as isize {
            sum += tmp[k.clamp(0, h as isize - 1) as usize * w + x] as f64;
        }
        for y in 0..h {
            out[y * w + x] = (sum / (2 * r + 1) as f64) as f32;
            let top = (y as isize - r as isize).clamp(0, h as isize - 1) as usize;
            let bottom = (y + r + 1).min(h - 1);
            sum += tmp[bottom * w + x] as f64 - tmp[top * w + x] as f64;
        }
    }
    Ok(out)
}
pub fn presence(
    image: &mut FloatImage,
    p: Presence,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    if p == Presence::default() {
        return Ok(());
    }
    let (w, h) = (image.width as usize, image.height as usize);
    if p.texture != 0. {
        let y: Vec<_> = image.pixels.iter().copied().map(luma).collect();
        let fine = blur(&y, w, h, radius(image, 2.), cancel)?;
        let medium = blur(
            &y,
            w,
            h,
            radius(image, 8.).max(radius(image, 2.) + 1),
            cancel,
        )?;
        for (i, pixel) in image.pixels.iter_mut().enumerate() {
            let delta = (fine[i] - medium[i]).clamp(-0.15, 0.15) * p.texture / 100.;
            *pixel = pixel.map(|v| v + delta);
        }
    }
    if p.clarity != 0. {
        let y: Vec<_> = image.pixels.iter().copied().map(luma).collect();
        let broad = blur(&y, w, h, radius(image, 64.), cancel)?;
        for (i, pixel) in image.pixels.iter_mut().enumerate() {
            let mid = 4. * y[i].max(0.) / (1. + y[i].max(0.)).powi(2);
            let delta = (y[i] - broad[i]).clamp(-0.3, 0.3) * mid * p.clarity / 100.;
            *pixel = pixel.map(|v| v + delta);
        }
    }
    if p.dehaze != 0. {
        let dark: Vec<_> = image
            .pixels
            .iter()
            .map(|p| {
                p.iter()
                    .copied()
                    .fold(f32::INFINITY, f32::min)
                    .clamp(0., 1.)
            })
            .collect();
        let veil = blur(&dark, w, h, radius(image, 128.), cancel)?;
        for (i, pixel) in image.pixels.iter_mut().enumerate() {
            let amount = p.dehaze / 100.;
            let v = veil[i].min(0.8) * amount.abs() * 0.65;
            *pixel = if amount > 0. {
                pixel.map(|c| (c - v) / (1. - v))
            } else {
                pixel.map(|c| c * (1. - v) + v)
            };
        }
    }
    cancel.check()
}
pub fn detail(
    image: &mut FloatImage,
    d: Detail,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    let (w, h) = (image.width as usize, image.height as usize);
    if d.noise.luminance > 0. || d.noise.color > 0. {
        let source = image.clone();
        for y in 0..h {
            cancel.check()?;
            for x in 0..w {
                let i = y * w + x;
                let center = source.pixels[i];
                let cy = luma(center);
                let mut sy = 0.;
                let mut sc = [0.; 3];
                let mut wy = 0.;
                let mut wc = 0.;
                for dy in -2isize..=2 {
                    for dx in -2isize..=2 {
                        let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                        let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                        let q = source.pixels[yy * w + xx];
                        let qy = luma(q);
                        let spatial = (-(dx * dx + dy * dy) as f32 / 4.).exp();
                        let ly = spatial
                            * (-(qy - cy).powi(2)
                                / (0.0004 + (100. - d.noise.luminance_detail) * 0.00008))
                                .exp();
                        let chroma = (0..3)
                            .map(|c| ((q[c] - qy) - (center[c] - cy)).powi(2))
                            .sum::<f32>();
                        let lc = spatial
                            * (-chroma / (0.001 + (100. - d.noise.color_detail) * 0.0003)).exp();
                        sy += qy * ly;
                        wy += ly;
                        wc += lc;
                        for c in 0..3 {
                            sc[c] += (q[c] - qy) * lc;
                        }
                    }
                }
                let ny = cy + (sy / wy - cy) * d.noise.luminance / 100.;
                image.pixels[i] = std::array::from_fn(|c| {
                    ny + (center[c] - cy) * (1. - d.noise.color / 100.)
                        + sc[c] / wc * d.noise.color / 100.
                });
            }
        }
    }
    if d.sharpening.amount > 0. {
        let y: Vec<_> = image.pixels.iter().copied().map(luma).collect();
        // Fractional radius mixes narrow/wide scales instead of rounding every UI value away.
        let base = radius(image, 1.);
        let narrow = blur(&y, w, h, base, cancel)?;
        let wide = blur(&y, w, h, (base * 3).max(2), cancel)?;
        let mix = (d.sharpening.radius - 0.5) / 2.5;
        for (i, pixel) in image.pixels.iter_mut().enumerate() {
            let low = narrow[i] * (1. - mix) + wide[i] * mix;
            let high = y[i] - low;
            let threshold = d.sharpening.masking / 100. * 0.04;
            let edge = (high.abs() / (threshold + 0.00001)).clamp(0., 1.);
            let fine = y[i] - narrow[i];
            let detail = d.sharpening.detail / 100.;
            let delta = (high * (1. - detail) + fine * detail * 1.5).clamp(-0.2, 0.2)
                * d.sharpening.amount
                / 100.
                * edge;
            *pixel = pixel.map(|v| v + delta);
        }
    }
    cancel.check()
}
/// Applied AFTER rotation/crop, so this is a creative post-crop effect, never optics.
pub fn vignette(
    image: &mut FloatImage,
    v: Vignette,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    if v.amount == 0. {
        return Ok(());
    }
    let exponent = 2f32.powf(v.roundness / 100.);
    let start = v.midpoint / 100.;
    let feather = v.feather / 100.;
    for y in 0..image.height {
        cancel.check()?;
        for x in 0..image.width {
            let nx = ((x as f32 + 0.5) / image.width as f32 * 2. - 1.).abs();
            let ny = ((y as f32 + 0.5) / image.height as f32 * 2. - 1.).abs();
            let r =
                ((nx.powf(2. * exponent) + ny.powf(2. * exponent)) / 2.).powf(1. / (2. * exponent));
            let t = ((r - start * (1. - feather)) / (feather.max(0.01))).clamp(0., 1.);
            let s = t * t * (3. - 2. * t);
            let gain = (v.amount / 50. * s).exp2();
            let p = &mut image.pixels[(y * image.width + x) as usize];
            *p = p.map(|c| c * gain);
        }
    }
    Ok(())
}
