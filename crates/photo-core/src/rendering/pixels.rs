use photo_contracts::{
    CancellationToken, ProcessingError, ProcessingErrorCode as Code, ProcessingResult,
    RenderAdjustments,
};

/// Scene/display-linear sRGB primaries, D65. Values above one survive creative stages.
#[derive(Clone, Debug)]
pub struct FloatImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
}
impl FloatImage {
    pub fn blank(width: u32, height: u32, max_pixels: u64) -> ProcessingResult<Self> {
        let count = u64::from(width) * u64::from(height);
        if count == 0 || count > max_pixels {
            return Err(ProcessingError::new(
                Code::InsufficientMemory,
                "Image dimensions exceed the render memory budget",
            ));
        }
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(count as usize).map_err(|_| {
            ProcessingError::new(Code::InsufficientMemory, "Unable to reserve working pixels")
        })?;
        pixels.resize(count as usize, [0.; 3]);
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        if x < -0.5 || y < -0.5 || x > self.width as f32 - 0.5 || y > self.height as f32 - 0.5 {
            return [0.; 3];
        }
        let x = x.clamp(0., self.width as f32 - 1.);
        let y = y.clamp(0., self.height as f32 - 1.);
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        std::array::from_fn(|c| {
            let p = |xx, yy| self.pixels[(yy * self.width + xx) as usize][c];
            (p(x0, y0) * (1. - fx) + p(x1, y0) * fx) * (1. - fy)
                + (p(x0, y1) * (1. - fx) + p(x1, y1) * fx) * fy
        })
    }
    /// Area averaging in linear light avoids aliasing in the reduced working proxy.
    pub fn reduced(&self, edge: u32, cancel: &CancellationToken) -> ProcessingResult<Self> {
        if self.width.max(self.height) <= edge {
            return Ok(self.clone());
        }
        let scale = edge as f64 / self.width.max(self.height) as f64;
        let w = (self.width as f64 * scale).round().max(1.) as u32;
        let h = (self.height as f64 * scale).round().max(1.) as u32;
        let mut out = Self::blank(w, h, u64::from(edge) * u64::from(edge))?;
        for y in 0..h {
            cancel.check()?;
            for x in 0..w {
                let x0 = x as f64 * self.width as f64 / w as f64;
                let x1 = (x + 1) as f64 * self.width as f64 / w as f64;
                let y0 = y as f64 * self.height as f64 / h as f64;
                let y1 = (y + 1) as f64 * self.height as f64 / h as f64;
                let mut sum = [0.; 3];
                let mut weight = 0.;
                for yy in y0.floor() as u32..(y1.ceil() as u32).min(self.height) {
                    for xx in x0.floor() as u32..(x1.ceil() as u32).min(self.width) {
                        let a = ((x1.min((xx + 1) as f64) - x0.max(xx as f64))
                            * (y1.min((yy + 1) as f64) - y0.max(yy as f64)))
                            as f32;
                        weight += a;
                        for (c, s) in sum.iter_mut().enumerate() {
                            *s += self.pixels[(yy * self.width + xx) as usize][c] * a;
                        }
                    }
                }
                out.pixels[(y * w + x) as usize] = sum.map(|v| v / weight);
            }
        }
        Ok(out)
    }
}
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
pub fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0., 1.);
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}
pub fn luma(p: [f32; 3]) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}
type Matrix = [[f32; 3]; 3];
fn mul(m: Matrix, p: [f32; 3]) -> [f32; 3] {
    m.map(|r| r[0] * p[0] + r[1] * p[1] + r[2] * p[2])
}
fn product(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}
fn white_xy(t: f32) -> (f32, f32) {
    let x = if t <= 4000. {
        -0.2661239e9 / t.powi(3) - 0.234358e6 / t.powi(2) + 0.8776956e3 / t + 0.17991
    } else {
        -3.025847e9 / t.powi(3) + 2.107038e6 / t.powi(2) + 0.2226347e3 / t + 0.24039
    };
    let y = if t <= 2222. {
        -1.1063814 * x.powi(3) - 1.3481102 * x * x + 2.1855583 * x - 0.20219684
    } else if t <= 4000. {
        -0.9549476 * x.powi(3) - 1.3741859 * x * x + 2.09137 * x - 0.16748866
    } else {
        3.081758 * x.powi(3) - 5.8733864 * x * x + 3.7511299 * x - 0.37001482
    };
    (x, y)
}
/// Bradford chromatic adaptation, offset from D65 so neutral is an exact identity.
fn white_balance(a: &RenderAdjustments) -> Matrix {
    let identity = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    if a.temperature == 6500. && a.tint == 0. {
        return identity;
    }
    let (x, y) = white_xy(a.temperature);
    let (bx, by) = white_xy(6500.);
    let x = x - bx + 0.3127;
    let y = y - by + 0.3290 + a.tint * 0.00015;
    let bradford = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    let inverse = [
        [0.986993, -0.147054, 0.159963],
        [0.432305, 0.51836, 0.049291],
        [-0.008529, 0.040043, 0.968487],
    ];
    let source = mul(bradford, [x / y, 1., (1. - x - y) / y]);
    let target = mul(bradford, [0.950456, 1., 1.089058]);
    let mut diagonal = identity;
    for i in 0..3 {
        diagonal[i][i] = target[i] / source[i];
    }
    let rgb_xyz = [
        [0.4124564, 0.3575761, 0.1804375],
        [0.2126729, 0.7151522, 0.072175],
        [0.0193339, 0.119192, 0.9503041],
    ];
    let xyz_rgb = [
        [3.2404542, -1.5371385, -0.4985314],
        [-0.969266, 1.8760108, 0.041556],
        [0.0556434, -0.2040259, 1.0572252],
    ];
    product(
        xyz_rgb,
        product(inverse, product(diagonal, product(bradford, rgb_xyz))),
    )
}
fn smooth(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn apply(
    image: &mut FloatImage,
    a: &RenderAdjustments,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    let a = a.validated()?;
    let wb = white_balance(&a);
    let exposure = 2f32.powf(a.exposure_ev);
    for row in image.pixels.chunks_mut(image.width as usize) {
        cancel.check()?;
        for p in row {
            *p = mul(wb, *p).map(|v| v * exposure);
            let y = luma(*p).max(0.);
            let zones = (a.shadows * (1. - smooth(0., 0.35, y))
                + a.highlights * smooth(0.25, 1., y)
                + a.blacks * (1. - smooth(0., 0.10, y))
                + a.whites * smooth(0.65, 1.5, y))
                / 100.;
            let mut gain = 2f32.powf(zones);
            if y > 1e-8 && a.contrast != 0. {
                gain *= ((y / 0.18).log2() * a.contrast / 200.).exp2();
            }
            *p = p.map(|v| v * gain);
            let y = luma(*p);
            let max = p.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let min = p.iter().copied().fold(f32::INFINITY, f32::min);
            let sat = if max > 1e-8 {
                ((max - min) / max).clamp(0., 1.)
            } else {
                0.
            };
            let factor = (1. + a.saturation / 100.) * (1. + a.vibrance / 100. * (1. - sat));
            *p = p.map(|v| y + (v - y) * factor);
        }
    }
    if a.noise_reduction > 0. {
        spatial(image, a.noise_reduction / 100., false, cancel)?;
    }
    if a.sharpening > 0. {
        spatial(image, a.sharpening / 100., true, cancel)?;
    }
    Ok(())
}
fn spatial(
    image: &mut FloatImage,
    amount: f32,
    sharpen: bool,
    cancel: &CancellationToken,
) -> ProcessingResult<()> {
    let mut copy = FloatImage::blank(image.width, image.height, image.pixels.len() as u64)?;
    copy.pixels.copy_from_slice(&image.pixels);
    for y in 0..image.height {
        cancel.check()?;
        for x in 0..image.width {
            let index = (y * image.width + x) as usize;
            let center = copy.pixels[index];
            let mut total = [0.; 3];
            let mut weights = 0.;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let xx = (x as i32 + dx).clamp(0, image.width as i32 - 1) as u32;
                    let yy = (y as i32 + dy).clamp(0, image.height as i32 - 1) as u32;
                    let p = copy.pixels[(yy * image.width + xx) as usize];
                    let base = if dx == 0 { 2. } else { 1. } * if dy == 0 { 2. } else { 1. };
                    let weight = if sharpen {
                        base
                    } else {
                        base * (-(luma(p) - luma(center)).powi(2) / 0.0025).exp()
                    };
                    for c in 0..3 {
                        total[c] += p[c] * weight;
                    }
                    weights += weight;
                }
            }
            image.pixels[index] = std::array::from_fn(|c| {
                let blur = total[c] / weights;
                if sharpen {
                    center[c] + (center[c] - blur).clamp(-0.1, 0.1) * amount
                } else {
                    center[c] + (blur - center[c]) * amount * 0.65
                }
            });
        }
    }
    Ok(())
}
/// Source orientation is already normalized by decoder. Crop refers to expanded rotated canvas.
pub fn geometry(
    image: FloatImage,
    a: &RenderAdjustments,
    max_pixels: u64,
    cancel: &CancellationToken,
) -> ProcessingResult<FloatImage> {
    let a = a.validated()?;
    if a.rotation_degrees == 0. && a.crop == Default::default() {
        return Ok(image);
    }
    let angle = a.rotation_degrees.to_radians();
    let (s, c) = angle.sin_cos();
    let w = (image.width as f32 * c.abs() + image.height as f32 * s.abs() - 1e-4)
        .ceil()
        .max(1.) as u32;
    let h = (image.width as f32 * s.abs() + image.height as f32 * c.abs() - 1e-4)
        .ceil()
        .max(1.) as u32;
    let x0 = (a.crop.x * w as f32).floor() as u32;
    let y0 = (a.crop.y * h as f32).floor() as u32;
    let x1 = (((a.crop.x + a.crop.width) * w as f32).ceil() as u32).min(w);
    let y1 = (((a.crop.y + a.crop.height) * h as f32).ceil() as u32).min(h);
    let mut out = FloatImage::blank(x1.saturating_sub(x0), y1.saturating_sub(y0), max_pixels)?;
    for y in 0..out.height {
        cancel.check()?;
        for x in 0..out.width {
            let dx = (x + x0) as f32 + 0.5 - w as f32 / 2.;
            let dy = (y + y0) as f32 + 0.5 - h as f32 / 2.;
            out.pixels[(y * out.width + x) as usize] = image.sample(
                c * dx + s * dy + image.width as f32 / 2. - 0.5,
                -s * dx + c * dy + image.height as f32 / 2. - 0.5,
            );
        }
    }
    Ok(out)
}
