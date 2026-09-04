//! Conservative Lensfun DATABASE adapter, not a binding or port of the LGPL library.
//! Hugin polynomials use half-short-side radii; PA uses half-diagonal radii.
use super::{internal, pixels::FloatImage};
use photo_contracts::*;
use std::path::Path;
pub const DATABASE_VERSION: &str = "lensfun-db-23e8cb8050d680c7a293edb3d48b600754665f05";
#[derive(Clone, Debug)]
struct Camera {
    make: String,
    model: String,
    crop: f32,
}
#[derive(Clone, Debug)]
struct Calibration {
    kind: String,
    model: String,
    focal: f32,
    aperture: f32,
    distance: f32,
    values: [f32; 6],
}
#[derive(Clone, Debug)]
struct Lens {
    make: String,
    model: String,
    crop: f32,
    aspect: f32,
    supported: bool,
    calibration: Vec<Calibration>,
}
#[derive(Default)]
pub struct LensProfileResolver {
    cameras: Vec<Camera>,
    lenses: Vec<Lens>,
    failure: Option<String>,
}
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn model_norm(make: &str, model: &str) -> String {
    let m = norm(make);
    let s = norm(model);
    s.strip_prefix(&m).unwrap_or(&s).to_string()
}
fn child<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|n| n.has_tag_name(name) && n.attribute("lang").is_none())
        .and_then(|n| n.text())
}
fn num(s: Option<&str>, default: f32) -> f32 {
    s.and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}
fn aspect_ratio(s: Option<&str>) -> f32 {
    match s {
        None => 1.5,
        Some(s) => {
            if let Some((a, b)) = s.split_once(':') {
                num(Some(a), f32::NAN) / num(Some(b), f32::NAN)
            } else {
                num(Some(s), f32::NAN)
            }
        }
    }
}
impl LensProfileResolver {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            failure: Some(reason.into()),
            ..Default::default()
        }
    }
    pub fn load(directory: &Path) -> Self {
        match Self::read(directory) {
            Ok(db) => db,
            Err(e) => Self::unavailable(e),
        }
    }
    fn read(directory: &Path) -> Result<Self, String> {
        let mut paths = std::fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .map(|e| e.map(|v| v.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        paths.sort();
        let mut db = Self::default();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        {
            if path.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
                return Err("Lens database file exceeds safety limit".into());
            }
            let xml = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let document = roxmltree::Document::parse_with_options(
                &xml,
                roxmltree::ParsingOptions {
                    allow_dtd: true,
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
            if document.root_element().attribute("version") != Some("2") {
                return Err("Unsupported lens database schema".into());
            }
            for n in document.root_element().children() {
                if n.has_tag_name("camera") {
                    db.cameras.push(Camera {
                        make: child(n, "maker").unwrap_or("").into(),
                        model: child(n, "model").unwrap_or("").into(),
                        crop: num(child(n, "cropfactor"), 0.),
                    });
                }
                if !n.has_tag_name("lens") {
                    continue;
                }
                let mut lens = Lens {
                    make: child(n, "maker").unwrap_or("").into(),
                    model: child(n, "model").unwrap_or("").into(),
                    crop: num(child(n, "cropfactor"), 0.),
                    aspect: aspect_ratio(child(n, "aspect-ratio")),
                    supported: child(n, "type").is_none_or(|s| s == "rectilinear")
                        && !n.children().any(|c| c.has_tag_name("center")),
                    calibration: vec![],
                };
                for c in n
                    .children()
                    .filter(|c| c.has_tag_name("calibration"))
                    .flat_map(|c| c.children())
                    .filter(|c| c.is_element())
                {
                    let kind = c.tag_name().name();
                    if !["distortion", "tca", "vignetting"].contains(&kind) {
                        continue;
                    }
                    let model = c.attribute("model").unwrap_or("");
                    let attrs = match (kind, model) {
                        ("distortion", "ptlens") => ["a", "b", "c", "", "", ""],
                        ("tca", "linear") => ["kr", "kb", "", "", "", ""],
                        ("tca", "poly3") => ["br", "cr", "vr", "bb", "cb", "vb"],
                        _ => ["k1", "k2", "k3", "", "", ""],
                    };
                    let values = std::array::from_fn(|i| {
                        if let Some(value) = c.attribute(attrs[i]) {
                            num(Some(value), f32::NAN)
                        } else {
                            if (kind == "tca" && model == "linear" && i < 2)
                                || (kind == "tca" && model == "poly3" && (i == 2 || i == 5))
                            {
                                1.
                            } else {
                                0.
                            }
                        }
                    });
                    lens.calibration.push(Calibration {
                        kind: kind.into(),
                        model: model.into(),
                        focal: num(c.attribute("focal"), 0.),
                        aperture: num(c.attribute("aperture"), 0.),
                        distance: num(c.attribute("distance"), 0.),
                        values,
                    });
                }
                db.lenses.push(lens);
            }
        }
        if db.lenses.is_empty() {
            return Err("Lens database contains no profiles".into());
        }
        Ok(db)
    }
    pub fn resolve(
        &self,
        meta: &OpticsMetadata,
        options: Optics,
        w: u32,
        h: u32,
    ) -> (OpticalMap, LensDiagnostic) {
        let mut map = OpticalMap {
            options,
            distortion: None,
            tca: None,
            vignette: None,
        };
        let mut d = LensDiagnostic {
            database_version: Some(DATABASE_VERSION.into()),
            ..Default::default()
        };
        if options.manual_distortion != 0. {
            d.applied.push("manual distortion".into());
        }
        if options.manual_vignette != 0. {
            d.applied.push("manual peripheral illumination".into());
        }
        if !options.enabled {
            return (map, d);
        }
        if let Some(e) = &self.failure {
            d.state = LensMatch::ProfileUnavailable;
            d.warnings.push(format!("Lens database unavailable: {e}"));
            return (map, d);
        }
        let Some(name) = &meta.lens_model else {
            d.state = LensMatch::NoProfile;
            d.warnings.push("No lens model in source metadata".into());
            return (map, d);
        };
        let candidates: Vec<_> = self
            .lenses
            .iter()
            .filter(|l| {
                meta.lens_make
                    .as_ref()
                    .is_none_or(|m| norm(m) == norm(&l.make))
                    && model_norm(&l.make, &l.model) == model_norm(&l.make, name)
            })
            .collect();
        if candidates.len() != 1 {
            d.state = if candidates.is_empty() {
                LensMatch::NoProfile
            } else {
                LensMatch::ApproximateMatch
            };
            d.warnings.push(
                "No unique exact lens identity; automatic coefficients were not applied".into(),
            );
            return (map, d);
        }
        let lens = candidates[0];
        d.profile = Some(format!("{} / {}", lens.make, lens.model));
        let camera = self.cameras.iter().find(|c| {
            meta.camera_make
                .as_ref()
                .is_some_and(|m| norm(m) == norm(&c.make))
                && meta
                    .camera_model
                    .as_ref()
                    .is_some_and(|m| model_norm(&c.make, m) == model_norm(&c.make, &c.model))
        });
        let aspect = w.max(h) as f32 / w.min(h).max(1) as f32;
        if !lens.supported
            || camera.is_none_or(|c| {
                c.crop <= 0. || lens.crop <= 0. || (c.crop / lens.crop - 1.).abs() > 0.001
            })
            || !lens.aspect.is_finite()
            || (aspect / lens.aspect - 1.).abs() > 0.02
        {
            d.state = LensMatch::ApproximateMatch;
            d.warnings.push("Profile identified but sensor crop/aspect or projection is unsupported; no automatic correction".into());
            return (map, d);
        }
        let Some(focal) = meta.focal_length.filter(|v| v.is_finite() && *v > 0.) else {
            d.state = LensMatch::ApproximateMatch;
            d.warnings
                .push("Focal length missing; automatic correction skipped".into());
            return (map, d);
        };
        d.state = LensMatch::ExactMatch;
        for (kind, enabled) in [
            (
                "distortion",
                options.distortion && options.distortion_strength > 0.,
            ),
            ("tca", options.chromatic_aberration),
            (
                "vignetting",
                options.vignette && options.vignette_strength > 0.,
            ),
        ] {
            if !enabled {
                continue;
            }
            // Deliberately no focal/aperture interpolation in this first database adapter.
            let matches: Vec<_> = lens
                .calibration
                .iter()
                .filter(|c| {
                    c.kind == kind
                        && (c.focal - focal).abs() < 0.05
                        && (kind != "vignetting"
                            || (meta.aperture.is_some_and(|a| (a - c.aperture).abs() < 0.06)
                                && meta.focus_distance.is_some_and(|v| {
                                    v.is_finite()
                                        && v > 0.
                                        && (v - c.distance).abs() <= 0.05 * c.distance.max(1.)
                                })))
                })
                .collect();
            let unique = matches.first().copied().filter(|first| {
                matches
                    .iter()
                    .all(|c| c.model == first.model && c.values == first.values)
            });
            if let Some(c) = unique {
                let supported = match kind {
                    "distortion" => ["ptlens", "poly3", "poly5"].contains(&c.model.as_str()),
                    "tca" => ["linear", "poly3"].contains(&c.model.as_str()),
                    _ => c.model == "pa",
                };
                if supported && c.values.iter().all(|v| v.abs() < 10.) {
                    match kind {
                        "distortion" => map.distortion = Some(c.clone()),
                        "tca" => map.tca = Some(c.clone()),
                        _ => map.vignette = Some(c.clone()),
                    };
                    d.applied.push(kind.into());
                    continue;
                }
            }
            d.state = LensMatch::ApproximateMatch;
            d.warnings.push(format!("{kind}: no exact supported calibration at recorded focal/aperture/distance; skipped"));
        }
        (map, d)
    }
}
#[derive(Clone, Default)]
pub struct OpticalMap {
    options: Optics,
    distortion: Option<Calibration>,
    tca: Option<Calibration>,
    vignette: Option<Calibration>,
}
impl OpticalMap {
    pub fn manual(options: Optics) -> Self {
        Self {
            options,
            ..Default::default()
        }
    }
    pub fn active(&self) -> bool {
        self.distortion.is_some()
            || self.tca.is_some()
            || self.vignette.is_some()
            || self.options.manual_distortion != 0.
            || self.options.manual_vignette != 0.
    }
    /// Returns source pixel coordinates for an output pixel. Green defines the mask geometry.
    pub fn source_coordinate(&self, x: f32, y: f32, w: u32, h: u32, channel: usize) -> (f32, f32) {
        let cx = (w as f32 - 1.) * 0.5;
        let cy = (h as f32 - 1.) * 0.5;
        let scale = w.min(h) as f32 * 0.5;
        let nx = (x - cx) / scale;
        let ny = (y - cy) / scale;
        let r = nx.hypot(ny);
        let mut factor = 1.;
        if let Some(c) = &self.distortion {
            let v = c.values;
            let f = match c.model.as_str() {
                "ptlens" => v[0] * r.powi(3) + v[1] * r * r + v[2] * r + 1. - v[0] - v[1] - v[2],
                "poly3" => 1. - v[0] + v[0] * r * r,
                _ => 1. + v[0] * r * r + v[1] * r.powi(4),
            };
            factor += self.options.distortion_strength * (f - 1.);
        }
        factor *= 1. + self.options.manual_distortion / 100. * 0.15 * r * r;
        let rd = r * factor;
        if channel != 1 {
            if let Some(c) = &self.tca {
                let v = c.values;
                factor *= if c.model == "linear" {
                    v[if channel == 0 { 0 } else { 1 }]
                } else {
                    let i = if channel == 0 { 0 } else { 3 };
                    v[i] * rd * rd + v[i + 1] * rd + v[i + 2]
                };
            }
        }
        if !factor.is_finite() || factor <= 0. || factor > 8. {
            return (-1e6, -1e6);
        }
        (cx + nx * factor * scale, cy + ny * factor * scale)
    }
    pub fn apply(
        &self,
        mut source: FloatImage,
        cancel: &CancellationToken,
    ) -> ProcessingResult<FloatImage> {
        if !self.active() {
            return Ok(source);
        }
        let (w, h) = (source.width, source.height);
        let diag = (w as f32).hypot(h as f32) * 0.5;
        // Lensfun PA is calibrated on the unwarped source: illuminate before coordinate lookup.
        for y in 0..h {
            cancel.check()?;
            for x in 0..w {
                let r = ((x as f32 - (w as f32 - 1.) * 0.5)
                    .hypot(y as f32 - (h as f32 - 1.) * 0.5)
                    / diag)
                    .min(1.);
                let r2 = r * r;
                let mut gain = 1.;
                if let Some(c) = &self.vignette {
                    let v = c.values;
                    let falloff = 1. + v[0] * r2 + v[1] * r2 * r2 + v[2] * r2 * r2 * r2;
                    gain = (1. / falloff.clamp(0.125, 8.)).powf(self.options.vignette_strength);
                }
                gain *= (self.options.manual_vignette / 50. * r2).exp2();
                let p = &mut source.pixels[(y * w + x) as usize];
                *p = p.map(|v| v * gain);
            }
        }
        if self.distortion.is_none() && self.tca.is_none() && self.options.manual_distortion == 0. {
            return Ok(source);
        }
        let mut out = FloatImage::blank(w, h, source.pixels.len() as u64)?;
        for y in 0..h {
            cancel.check()?;
            for x in 0..w {
                out.pixels[(y * w + x) as usize] = std::array::from_fn(|c| {
                    let (xx, yy) = self.source_coordinate(x as f32, y as f32, w, h, c);
                    source.sample(xx, yy)[c]
                });
            }
        }
        if out.pixels.iter().flatten().any(|v| !v.is_finite()) {
            return Err(internal("Nonfinite optical correction"));
        }
        Ok(out)
    }
}
