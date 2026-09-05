//! Measurements on the unedited proxy. No face identity, crops or model tensors are persisted.
use crate::rendering::{
    internal, io_error,
    pixels::{linear_to_srgb, luma, FloatImage},
};
use photo_contracts::{analysis::*, culling::*, CancellationToken, ProcessingResult};
use serde::Deserialize;
use std::{path::PathBuf, process::Command, time::Duration};
pub const FEATURE_VERSION: &str = "local-face-detail-descriptor-v1";
pub const YUNET_VERSION: &str =
    "2023mar-8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4";

#[derive(Clone, Debug, Deserialize)]
pub struct Detection {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub confidence: f64,
}
pub trait FaceDetector: Send + Sync {
    fn identity(&self) -> ProviderIdentity;
    /// Coordinates are normalized to the oriented source; boxes may cross its boundary.
    fn detect(
        &self,
        image: &FloatImage,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Signal<Vec<Detection>>>;
}
pub trait EyeStateDetector: Send + Sync {
    fn identity(&self) -> ProviderIdentity;
    fn detect(
        &self,
        image: &FloatImage,
        face: &BoundingBox,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Signal<EyeState>>;
}
pub struct UnavailableEyes;
impl EyeStateDetector for UnavailableEyes {
    fn identity(&self) -> ProviderIdentity {
        ProviderIdentity {
            provider: "EyeStateDetector".into(),
            model: "none".into(),
            version: "unavailable-v1".into(),
        }
    }
    fn detect(
        &self,
        _: &FloatImage,
        _: &BoundingBox,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Signal<EyeState>> {
        cancel.check()?;
        Ok(Signal::unavailable(
            "YuNet does not measure eyelid state; no eye-state model installed",
        ))
    }
}
pub struct YuNetDetector {
    pub toolkit: PathBuf,
    pub scratch: PathBuf,
}
impl YuNetDetector {
    fn paths(&self) -> (PathBuf, PathBuf, PathBuf) {
        (
            self.toolkit.join(if cfg!(windows) {
                "photo-face-helper.exe"
            } else {
                "photo-face-helper"
            }),
            self.toolkit.join("yunet-2023mar.onnx"),
            self.toolkit.join(if cfg!(windows) {
                "onnxruntime.dll"
            } else {
                "libonnxruntime.dylib"
            }),
        )
    }
}
impl FaceDetector for YuNetDetector {
    fn identity(&self) -> ProviderIdentity {
        let (h, m, r) = self.paths();
        ProviderIdentity {
            provider: "FaceDetector".into(),
            model: "YuNet".into(),
            version: format!(
                "{YUNET_VERSION};ort-1.29.0;{}",
                if h.is_file() && m.is_file() && r.is_file() {
                    "ready"
                } else {
                    "missing"
                }
            ),
        }
    }
    fn detect(
        &self,
        image: &FloatImage,
        cancel: &CancellationToken,
    ) -> ProcessingResult<Signal<Vec<Detection>>> {
        cancel.check()?;
        let (helper, model, runtime) = self.paths();
        if !helper.is_file() || !model.is_file() || !runtime.is_file() {
            return Ok(Signal::unavailable(
                "Prepare the optional local YuNet model/runtime to detect faces",
            ));
        }
        use sha2::{Digest, Sha256};
        let checksum = format!(
            "{:x}",
            Sha256::digest(std::fs::read(&model).map_err(io_error)?)
        );
        if checksum != "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4" {
            return Ok(Signal::Failed {
                reason: "YuNet model checksum mismatch; prepare pinned resources again".into(),
            });
        }
        std::fs::create_dir_all(&self.scratch).map_err(io_error)?;
        let temp = tempfile::tempdir_in(&self.scratch).map_err(io_error)?;
        let scale = 640f64 / image.width.max(image.height) as f64;
        let w = (image.width as f64 * scale).round() as usize;
        let h = (image.height as f64 * scale).round() as usize;
        let mut bgr = vec![0u8; 640 * 640 * 3];
        for y in 0..h {
            cancel.check()?;
            for x in 0..w {
                let p = image.sample(
                    ((x as f64 + 0.5) / scale - 0.5) as f32,
                    ((y as f64 + 0.5) / scale - 0.5) as f32,
                );
                for c in 0..3 {
                    bgr[(y * 640 + x) * 3 + c] = (linear_to_srgb(p[2 - c]) * 255.).round() as u8;
                }
            }
        }
        let input = temp.path().join("input.bgr");
        let output = temp.path().join("detections.json");
        std::fs::write(&input, bgr).map_err(io_error)?;
        let request = serde_json::to_vec(
            &serde_json::json!({"runtime":runtime,"model":model,"input":input,"output":output}),
        )
        .map_err(internal)?;
        let result = crate::process::output_cancellable(
            &mut Command::new(helper),
            &request,
            4096,
            Duration::from_secs(60),
            cancel,
        );
        cancel.check()?;
        if let Err(e) = result {
            return Ok(Signal::Failed { reason: e });
        }
        if !output.is_file() {
            return Ok(Signal::Failed {
                reason: "Local face helper did not return detections".into(),
            });
        }
        if std::fs::metadata(&output).map_err(io_error)?.len() > 64 * 1024 {
            return Err(internal("Face result exceeds budget"));
        }
        let mut faces: Vec<Detection> =
            serde_json::from_slice(&std::fs::read(output).map_err(io_error)?).map_err(internal)?;
        if faces.len() > 64 {
            return Err(internal("Too many face detections"));
        }
        for f in &mut faces {
            f.x /= w as f64;
            f.width /= w as f64;
            f.y /= h as f64;
            f.height /= h as f64;
        }
        Ok(Signal::available(faces, 0.9))
    }
}
fn observation<T, U>(v: &Observation<T>, map: impl FnOnce(&T) -> U) -> Signal<U> {
    match v {
        Observation::Available { value, confidence } => {
            Signal::available(map(value), confidence.unwrap_or(0.65))
        }
        Observation::Unavailable { reason } => Signal::Unavailable {
            reason: reason.clone(),
        },
        Observation::NotApplicable { reason } => Signal::NotApplicable {
            reason: reason.clone(),
        },
        Observation::Failed { reason } => Signal::Failed {
            reason: reason.clone(),
        },
    }
}
pub fn normalized_box(d: &Detection) -> Option<(BoundingBox, f64, f64)> {
    if ![d.x, d.y, d.width, d.height, d.confidence]
        .iter()
        .all(|v| v.is_finite())
        || d.width <= 0.
        || d.height <= 0.
        || !(0. ..=1.).contains(&d.confidence)
    {
        return None;
    }
    let x = d.x.clamp(0., 1.);
    let y = d.y.clamp(0., 1.);
    let right = (d.x + d.width).clamp(0., 1.);
    let bottom = (d.y + d.height).clamp(0., 1.);
    if right <= x || bottom <= y {
        return None;
    }
    Some((
        BoundingBox {
            x,
            y,
            width: right - x,
            height: bottom - y,
        },
        ((right - x) * (bottom - y) / (d.width * d.height)).clamp(0., 1.),
        x.min(y).min(1. - right).min(1. - bottom),
    ))
}
/// Fixed 64px ROI reduces face-size dependence. Contrast-normalized detail is not a focus classifier.
pub fn local_metrics(image: &FloatImage, b: &BoundingBox) -> (f64, f64, f64, f64, f64) {
    let mut values = [0f64; 4096];
    for y in 0..64 {
        for x in 0..64 {
            values[y * 64 + x] = f64::from(luma(image.sample(
                ((b.x + b.width * (x as f64 + 0.5) / 64.) * image.width as f64 - 0.5) as f32,
                ((b.y + b.height * (y as f64 + 0.5) / 64.) * image.height as f64 - 0.5) as f32,
            )))
            .clamp(0., 1.);
        }
    }
    let mean = values.iter().sum::<f64>() / 4096.;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / 4096.;
    let mut lap = 0.;
    let mut gx = 0.;
    let mut gy = 0.;
    for y in 1..63 {
        for x in 1..63 {
            let i = y * 64 + x;
            lap += (values[i - 1] + values[i + 1] + values[i - 64] + values[i + 64]
                - 4. * values[i])
                .powi(2);
            gx += (values[i + 1] - values[i - 1]).abs();
            gy += (values[i + 64] - values[i - 64]).abs();
        }
    }
    (
        (lap / (62. * 62.)).sqrt() / (variance.sqrt() + 0.03),
        mean,
        values.iter().filter(|v| **v >= 0.999).count() as f64 / 4096.,
        values.iter().filter(|v| **v <= 0.001).count() as f64 / 4096.,
        (gx - gy).abs() / (gx + gy + 1e-9),
    )
}
pub fn descriptor(image: &FloatImage, source: &AnalysisSource) -> SimilarityDescriptor {
    let sample = |x: usize, y: usize, w: usize, h: usize| {
        // Average each cell rather than point-sampling repeating texture.
        let x0 = x * image.width as usize / w;
        let x1 = ((x + 1) * image.width as usize / w)
            .max(x0 + 1)
            .min(image.width as usize);
        let y0 = y * image.height as usize / h;
        let y1 = ((y + 1) * image.height as usize / h)
            .max(y0 + 1)
            .min(image.height as usize);
        let mut sum = [0f64; 3];
        let mut n = 0f64;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let p = image.pixels[yy * image.width as usize + xx];
                for c in 0..3 {
                    sum[c] += p[c] as f64;
                }
                n += 1.;
            }
        }
        sum.map(|v| (v / n.max(1.)) as f32)
    };
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash = (hash << 1) | u64::from(luma(sample(x, y, 9, 8)) > luma(sample(x + 1, y, 9, 8)));
        }
    }
    let luminance_grid: Vec<f64> = (0..64)
        .map(|i| f64::from(luma(sample(i % 8, i / 8, 8, 8))).clamp(0., 1.))
        .collect();
    let color_grid = (0..16)
        .flat_map(|i| sample(i % 4, i / 4, 4, 4).map(|v| f64::from(linear_to_srgb(v))))
        .collect();
    SimilarityDescriptor {
        difference_hash: format!("{hash:016x}"),
        mean_luminance: luminance_grid.iter().sum::<f64>() / 64.,
        luminance_grid,
        color_grid,
        aspect_ratio: image.width as f64 / image.height as f64,
        capture_timestamp: source.capture_timestamp.clone(),
        camera: source
            .camera_make
            .as_ref()
            .or(source.camera_model.as_ref())
            .map(|_| {
                format!(
                    "{} {}",
                    source.camera_make.as_deref().unwrap_or(""),
                    source.camera_model.as_deref().unwrap_or("")
                )
            }),
    }
}
pub fn extract(
    image: &FloatImage,
    a: &PhotoAnalysis,
    faces: &dyn FaceDetector,
    eyes: &dyn EyeStateDetector,
    cancel: &CancellationToken,
) -> ProcessingResult<CullingFeatures> {
    cancel.check()?;
    let people = if a.photo_type == PhotoType::Portrait {
        let detected = optional_detection(|| faces.detect(image, cancel), cancel)?;
        match detected {
            Signal::Available {
                mut value,
                confidence,
            } => {
                value.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
                let mut found = Vec::new();
                for (index, d) in value.iter().take(64).enumerate() {
                    cancel.check()?;
                    let Some((bbox, visible_fraction, edge_distance)) = normalized_box(d) else {
                        return Err(internal("Invalid face geometry from provider"));
                    };
                    let pixels =
                        (bbox.width * image.width as f64).min(bbox.height * image.height as f64);
                    let (detail, mean, hi, lo, _) = local_metrics(image, &bbox);
                    let sharpness = if pixels < 32. {
                        Signal::Uncertain {
                            reason: "Face too small for reliable local detail".into(),
                        }
                    } else {
                        Signal::available(detail, if pixels >= 64. { 0.8 } else { 0.55 })
                    };
                    let eye = if pixels < 32. {
                        Signal::Uncertain {
                            reason: "Face too small for eye state".into(),
                        }
                    } else {
                        optional_detection(|| eyes.detect(image, &bbox, cancel), cancel)?
                    };
                    found.push(FaceFeatures {
                        index: index as u32,
                        bbox,
                        detection_confidence: d.confidence,
                        sharpness,
                        mean_luminance: mean,
                        highlight_clip_fraction: hi,
                        shadow_clip_fraction: lo,
                        eyes: eye,
                        edge_distance,
                        visible_fraction,
                        relevant: pixels >= 24. && d.confidence >= 0.9,
                    });
                }
                Signal::available(found, confidence)
            }
            Signal::Unavailable { reason } => Signal::Unavailable { reason },
            Signal::Failed { reason } => Signal::Failed { reason },
            Signal::Uncertain { reason } => Signal::Uncertain { reason },
            Signal::NotApplicable { reason } => Signal::NotApplicable { reason },
        }
    } else {
        Signal::NotApplicable {
            reason: "Face rules only apply to Portrait".into(),
        }
    };
    let mut reliable: Vec<_> = people
        .value()
        .into_iter()
        .flatten()
        .filter(|f| f.relevant && f.sharpness.confidence() >= 0.7)
        .filter_map(|f| f.sharpness.value().map(|v| (f.index, *v)))
        .collect();
    reliable.sort_by(|a, b| a.1.total_cmp(&b.1));
    let median = reliable.get(reliable.len() / 2).map(|v| v.1).unwrap_or(0.);
    let outlier_subjects = reliable
        .iter()
        .filter(|(_, v)| reliable.len() > 1 && median > 0.05 && *v < median * 0.45)
        .map(|(i, _)| *i)
        .collect();
    let spread = if reliable.len() > 1 {
        Signal::available(reliable.last().unwrap().1 - reliable[0].1, 0.75)
    } else {
        Signal::unavailable("Requires two reliable faces")
    };
    let subject = &a.subjects.measurements;
    let full = BoundingBox {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };
    let (_, _, _, _, directional) = local_metrics(image, &full);
    let f = CullingFeatures {
        asset_id: a.asset_id.clone(),
        photo_type: a.photo_type,
        source_fingerprint: a.source_fingerprint.clone(),
        source_analysis_id: a.analysis_id.clone(),
        source_analysis_version: a.schema_version,
        feature_version: FEATURE_VERSION.into(),
        models: if a.photo_type == PhotoType::Portrait {
            vec![faces.identity(), eyes.identity()]
        } else {
            vec![]
        },
        technical: TechnicalFeatures {
            global_sharpness: a.common.detail.laplacian_rms,
            global_edge_strength: a.common.detail.edge_strength,
            noise_severity: observation(&a.common.detail.noise, |n| n.severity),
            directional_detail: Signal::available(directional, 0.35),
            subject_sharpness: observation(subject, |s| s.subject.edge_strength),
        },
        people: PeopleFeatures {
            faces: people,
            softest_subject: reliable.first().map(|v| v.0),
            face_sharpness_spread: spread,
            outlier_subjects,
        },
        framing: FramingFeatures {
            subject_edge_distance: observation(subject, |s| s.geometry.edge_proximity),
            subject_occupancy: observation(subject, |s| s.geometry.area_fraction),
        },
        composition: CompositionFeatures {
            level_angle: observation(&a.common.composition.horizontal_line, |v| v.angle_degrees),
            aspect_ratio: a.common.composition.aspect_ratio,
        },
        exposure: ExposureFeatures {
            median_luminance: a.common.exposure.median_luminance,
            highlight_clip_fraction: a.common.exposure.highlight_clip_fraction,
            shadow_clip_fraction: a.common.exposure.shadow_clip_fraction,
            tonal_range: a.common.dynamic_range.percentile_range,
            subject_background_ev: observation(&a.lighting.subject_background_ev_difference, |v| {
                *v
            }),
        },
        descriptor: descriptor(image, &a.common.source),
    };
    f.validate().map_err(internal)?;
    Ok(f)
}

fn optional_detection<T>(
    work: impl FnOnce() -> ProcessingResult<Signal<T>>,
    cancel: &CancellationToken,
) -> ProcessingResult<Signal<T>> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err(internal("Optional culling detector stopped unexpectedly")));
    cancel.check()?;
    Ok(result.unwrap_or_else(|e| Signal::Failed {
        reason: e.message.chars().take(2048).collect(),
    }))
}
