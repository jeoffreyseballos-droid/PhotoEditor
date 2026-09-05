//! Central, versioned policy. No SQL, pixel decoding, UI, editing or identity recognition.
use photo_contracts::{analysis::PhotoType, culling::*};
pub const CULLING_ENGINE_VERSION: &str = "photo-culling-real-photo-calibration-v3";
const FACE_STRONG_DETAIL: f64 = 0.18;
const FACE_EXCEPTIONAL_DETAIL: f64 = 0.45;
const IMPORTANT_FACE_AREA: f64 = 0.008;
const FACE_SEVERE_SOFTNESS: f64 = 0.20;
const SEVERE_DEFECT_CAP: f64 = 19.;
pub struct Scored {
    pub rating: Stars,
    pub absolute_score: f64,
    pub ranking_score: f64,
    pub score: f64,
    pub confidence: f64,
    pub reasons: Vec<CullingReason>,
}
struct RatingGate {
    cap: f64,
    reason: CullingReason,
}
fn severe_portrait_focus_gate(face: &FaceFeatures) -> Option<RatingGate> {
    let detail = *face.sharpness.value()?;
    let confidence = face.sharpness.confidence();
    let area = face.bbox.width * face.bbox.height;
    (face.relevant
        && face.detection_confidence >= 0.85
        && confidence >= 0.7
        && area >= IMPORTANT_FACE_AREA
        && detail < FACE_SEVERE_SOFTNESS)
        .then(|| RatingGate {
            cap: SEVERE_DEFECT_CAP,
            reason: reason(
                ReasonCode::SevereSubjectSoftness,
                Severity::Major,
                confidence,
                Some(face.index),
                Some((detail, "normalized_detail", Some(FACE_SEVERE_SOFTNESS))),
            ),
        })
}
fn apply_rating_gates(value: f64, gates: &[RatingGate]) -> f64 {
    gates.iter().fold(value, |score, gate| score.min(gate.cap))
}
#[derive(Clone, Copy)]
pub struct Weights {
    pub blink: f64,
    pub soft_face: f64,
    pub framing: f64,
    pub clipping: f64,
    pub detail: f64,
    pub level: f64,
}
pub fn weights(kind: PhotoType) -> Weights {
    match kind {
        PhotoType::Portrait => Weights {
            blink: 32.,
            soft_face: 24.,
            framing: 6.,
            clipping: 10.,
            detail: 0.,
            level: 0.,
        },
        PhotoType::RealEstate => Weights {
            blink: 0.,
            soft_face: 0.,
            framing: 0.,
            clipping: 14.,
            detail: 8.,
            level: 3.,
        },
        PhotoType::Landscape => Weights {
            blink: 0.,
            soft_face: 0.,
            framing: 0.,
            clipping: 10.,
            detail: 4.,
            level: 2.,
        },
    }
}
pub fn star_mapping(score: f64) -> Stars {
    Stars::new(if score < 20. {
        1
    } else if score < 40. {
        2
    } else if score < 65. {
        3
    } else if score < 88. {
        4
    } else {
        5
    })
    .expect("bounded stars")
}
fn reason(
    code: ReasonCode,
    severity: Severity,
    confidence: f64,
    index: Option<u32>,
    metric: Option<(f64, &str, Option<f64>)>,
) -> CullingReason {
    CullingReason {
        code,
        severity,
        confidence,
        subject_index: index,
        measurement: metric.map(|(value, unit, reference)| ReasonMeasurement {
            value,
            unit: unit.into(),
            reference,
        }),
    }
}
pub fn score(
    features: &CullingFeatures,
    similarity: &SimilarityContext,
) -> Result<Scored, CullingError> {
    features.validate()?;
    similarity.validate()?;
    let w = weights(features.photo_type);
    let mut value = 82f64;
    let mut confidence = 0.72f64;
    let mut reasons = Vec::new();
    let mut portrait_verified = false;
    let mut portrait_focus_verified = false;
    let mut rating_gates = Vec::new();
    if features.photo_type == PhotoType::Portrait {
        if let Some(faces) = features.people.faces.value() {
            let relevant: Vec<_> = faces
                .iter()
                .filter(|f| f.relevant && f.detection_confidence >= 0.85)
                .collect();
            if relevant.is_empty() {
                confidence = 0.45;
                reasons.push(reason(
                    ReasonCode::NoFacesDetected,
                    Severity::Review,
                    features.people.faces.confidence(),
                    None,
                    None,
                ));
            }
            let mut blink = 0usize;
            let mut soft = 0usize;
            let mut eye_known = 0usize;
            let mut sharp_known = 0usize;
            let mut exceptional_focus = 0usize;
            let mut framing_penalty = 0f64;
            let mut clipping_penalty = 0f64;
            let mut low_detail = false;
            let mut sharpness: Vec<f64> = relevant
                .iter()
                .filter_map(|f| {
                    f.sharpness
                        .value()
                        .filter(|_| f.sharpness.confidence() >= 0.7)
                        .copied()
                })
                .collect();
            sharpness.sort_by(f64::total_cmp);
            let median = sharpness.get(sharpness.len() / 2).copied().unwrap_or(0.);
            for f in &relevant {
                match &f.eyes {
                    Signal::Available {
                        value: EyeState::Open,
                        confidence: c,
                    } if *c >= 0.85 => {
                        eye_known += 1;
                        reasons.push(reason(
                            ReasonCode::EyesOpen,
                            Severity::Positive,
                            *c,
                            Some(f.index),
                            None,
                        ));
                    }
                    Signal::Available {
                        value: EyeState::Closed,
                        confidence: c,
                    } if *c >= 0.85 => {
                        blink += 1;
                        eye_known += 1;
                        reasons.push(reason(
                            ReasonCode::EyesClosed,
                            Severity::Major,
                            *c,
                            Some(f.index),
                            None,
                        ));
                    }
                    _ => {
                        reasons.push(reason(
                            ReasonCode::EyesUncertain,
                            Severity::Review,
                            f.eyes.confidence(),
                            Some(f.index),
                            None,
                        ));
                    }
                }
                if let Some(sharp) = f.sharpness.value() {
                    let severe_gate = severe_portrait_focus_gate(f);
                    let severe_focus = severe_gate.is_some();
                    if let Some(gate) = severe_gate {
                        reasons.push(gate.reason.clone());
                        rating_gates.push(gate);
                    } else if f.sharpness.confidence() >= 0.7 && *sharp >= FACE_STRONG_DETAIL {
                        sharp_known += 1;
                        if *sharp >= FACE_EXCEPTIONAL_DETAIL {
                            exceptional_focus += 1;
                        }
                    }
                    if relevant.len() >= 2
                        && median > 0.05
                        && *sharp < median * 0.45
                        && f.sharpness.confidence() >= 0.75
                    {
                        soft += 1;
                        reasons.push(reason(
                            ReasonCode::FaceSoft,
                            Severity::Issue,
                            f.sharpness.confidence(),
                            Some(f.index),
                            Some((*sharp, "normalized_detail", Some(median))),
                        ));
                    } else if !severe_focus
                        && *sharp >= FACE_STRONG_DETAIL
                        && f.sharpness.confidence() >= 0.7
                    {
                        reasons.push(reason(
                            ReasonCode::FaceSharp,
                            Severity::Positive,
                            f.sharpness.confidence(),
                            Some(f.index),
                            Some((*sharp, "normalized_detail", None)),
                        ));
                    } else {
                        low_detail |= *sharp < 0.03 && f.sharpness.confidence() >= 0.7;
                        reasons.push(reason(
                            ReasonCode::LowTextureOrBlur,
                            Severity::Review,
                            0.45,
                            Some(f.index),
                            Some((*sharp, "normalized_detail", None)),
                        ));
                    }
                }
                if f.visible_fraction < 0.85 {
                    framing_penalty = w.framing;
                    reasons.push(reason(
                        ReasonCode::FacePartlyClipped,
                        Severity::Review,
                        0.8,
                        Some(f.index),
                        Some((f.visible_fraction, "fraction", None)),
                    ));
                } else if f.edge_distance < 0.01 {
                    reasons.push(reason(
                        ReasonCode::FaceNearEdge,
                        Severity::Review,
                        0.7,
                        Some(f.index),
                        Some((f.edge_distance, "frame_fraction", None)),
                    ));
                }
                if f.highlight_clip_fraction.max(f.shadow_clip_fraction) > 0.65 {
                    clipping_penalty = w.clipping;
                    reasons.push(reason(
                        ReasonCode::SevereClipping,
                        Severity::Issue,
                        0.8,
                        Some(f.index),
                        Some((
                            f.highlight_clip_fraction.max(f.shadow_clip_fraction),
                            "fraction",
                            None,
                        )),
                    ));
                }
            }
            value -= framing_penalty + clipping_penalty;
            if low_detail && soft == 0 {
                value -= 18.;
            }
            if blink > 0 {
                value -= w.blink + (blink.saturating_sub(1) as f64 * 4.).min(8.);
            }
            if soft > 0 {
                value -= w.soft_face + (soft.saturating_sub(1) as f64 * 3.).min(6.);
            }
            if !rating_gates.is_empty() {
                confidence = confidence.max(0.8);
            }
            if relevant.len() > 1 && (blink > 0 || soft > 0) {
                reasons.push(reason(
                    ReasonCode::GroupIntegrity,
                    Severity::Issue,
                    0.85,
                    None,
                    Some(((blink + soft) as f64, "issues", Some(relevant.len() as f64))),
                ));
            }
            portrait_verified = !relevant.is_empty()
                && eye_known == relevant.len()
                && sharp_known == relevant.len();
            portrait_focus_verified = !relevant.is_empty() && sharp_known == relevant.len();
            let portrait_exceptional_focus =
                !relevant.is_empty() && exceptional_focus == relevant.len();
            if portrait_verified {
                confidence = 0.86;
                if blink == 0 && soft == 0 {
                    value += 10.;
                }
            } else {
                confidence = confidence.min(0.6);
                if portrait_focus_verified && blink == 0 && soft == 0 {
                    // Strong measured focus can earn a small technical bonus without
                    // pretending that unavailable eye state was observed as open.
                    value += if portrait_exceptional_focus { 6. } else { 3. };
                }
            }
            // Strong per-person evidence is not erased merely because a different person's eyes are unavailable.
            if blink > 0 || soft > 0 {
                confidence = confidence.max(0.78);
            }
        } else {
            confidence = 0.4;
            reasons.push(reason(
                ReasonCode::FaceDetectorUnavailable,
                Severity::Review,
                0.,
                None,
                None,
            ));
        }
        if let Some(d) = features.framing.subject_edge_distance.value() {
            if *d < 0.005 {
                reasons.push(reason(
                    ReasonCode::SubjectNearEdge,
                    Severity::Review,
                    0.4,
                    None,
                    Some((*d, "frame_fraction", None)),
                ));
            }
        }
    } else {
        if features.technical.global_sharpness > 0.04
            && features
                .technical
                .noise_severity
                .value()
                .is_some_and(|n| *n < 0.3)
            && features
                .exposure
                .highlight_clip_fraction
                .max(features.exposure.shadow_clip_fraction)
                < 0.02
        {
            value += 7.;
            reasons.push(reason(
                ReasonCode::TechnicalUsable,
                Severity::Positive,
                0.72,
                None,
                None,
            ));
        }
        if features.technical.global_sharpness < 0.008 {
            value -= w.detail;
            confidence = 0.55;
            reasons.push(reason(
                ReasonCode::LowTextureOrBlur,
                Severity::Review,
                0.4,
                None,
                Some((features.technical.global_sharpness, "laplacian_rms", None)),
            ));
        }
        if let Some(angle) = features.composition.level_angle.value() {
            if angle.abs() > 3. {
                value -= w.level;
                reasons.push(reason(
                    ReasonCode::LevelReview,
                    Severity::Review,
                    features.composition.level_angle.confidence().min(0.6),
                    None,
                    Some((*angle, "degrees", None)),
                ));
            }
        }
    }
    let exposure = &features.exposure;
    let clip = exposure
        .highlight_clip_fraction
        .max(exposure.shadow_clip_fraction);
    if clip > 0.995 && exposure.tonal_range < 0.003 {
        value = value.min(30.);
        confidence = 0.85;
        reasons.push(reason(
            ReasonCode::SevereClipping,
            Severity::Major,
            0.85,
            None,
            Some((clip, "fraction", None)),
        ));
    } else if clip > 0.20 {
        value -= w.clipping.min(8.);
        reasons.push(reason(
            ReasonCode::ExposureReview,
            Severity::Review,
            0.55,
            None,
            Some((clip, "fraction", None)),
        ));
    } else if exposure.median_luminance < 0.025 || exposure.median_luminance > 0.80 {
        reasons.push(reason(
            ReasonCode::ExposureReview,
            Severity::Review,
            0.35,
            None,
            Some((exposure.median_luminance, "linear_luminance", None)),
        ));
    }
    if let Some(noise) = features.technical.noise_severity.value() {
        if *noise > 0.7 {
            reasons.push(reason(
                ReasonCode::NoiseReview,
                Severity::Review,
                features.technical.noise_severity.confidence(),
                None,
                Some((*noise, "severity", None)),
            ));
        }
    }
    if let Some(d) = features.technical.directional_detail.value() {
        if *d > 0.85 {
            reasons.push(reason(
                ReasonCode::DirectionalDetail,
                Severity::Review,
                0.35,
                None,
                Some((*d, "anisotropy", None)),
            ));
        }
    }
    if confidence < 0.7 && rating_gates.is_empty() {
        value = value.max(45.);
        reasons.push(reason(
            ReasonCode::InsufficientEvidence,
            Severity::Review,
            confidence,
            None,
            None,
        ));
    }
    if features.photo_type == PhotoType::Portrait && !portrait_verified {
        value = value.min(if portrait_focus_verified { 90. } else { 87. });
    }
    let ranking_score = value.clamp(0., 100.);
    let absolute = apply_rating_gates(ranking_score, &rating_gates);
    if similarity.group_id.is_some() {
        reasons.push(reason(
            match similarity.kind {
                DuplicateKind::NearDuplicate => ReasonCode::NearDuplicate,
                DuplicateKind::Burst if similarity.preferred => ReasonCode::BurstSequence,
                DuplicateKind::Burst => ReasonCode::BurstAlternative,
                _ => ReasonCode::SimilarComposition,
            },
            Severity::Info,
            similarity.confidence,
            None,
            Some((similarity.group_size as f64, "photographs", None)),
        ));
        if similarity.preferred {
            if similarity.kind != DuplicateKind::Similar {
                value += 3.;
            }
            reasons.push(reason(
                ReasonCode::PreferredCandidate,
                Severity::Positive,
                similarity.confidence,
                None,
                None,
            ));
        } else if similarity
            .exact
            .as_ref()
            .is_none_or(|e| e.canonical_asset_id == features.asset_id)
        {
            // Identical copies are redundant, not evidence of inferior measured image quality.
            if similarity.kind != DuplicateKind::Similar {
                value -= 3.;
            }
            reasons.push(reason(
                ReasonCode::SimilarAlternative,
                Severity::Info,
                similarity.confidence,
                None,
                similarity.relative_score.map(|v| (v, "score_gap", None)),
            ));
        }
        if similarity.bracket_like {
            reasons.push(reason(
                ReasonCode::BracketLike,
                Severity::Review,
                0.6,
                None,
                None,
            ));
        }
    }
    if confidence < 0.7 && rating_gates.is_empty() {
        value = value.max(45.);
    }
    if features.photo_type == PhotoType::Portrait && !portrait_verified {
        value = value.min(if portrait_focus_verified { 90. } else { 87. });
    }
    value = apply_rating_gates(value, &rating_gates);
    value = value.clamp(0., 100.);
    if let Some(exact) = &similarity.exact {
        if exact.canonical_asset_id == features.asset_id {
            reasons.push(reason(
                ReasonCode::PreferredCopy,
                Severity::Positive,
                1.,
                None,
                None,
            ));
        } else {
            value = 5.;
            confidence = 1.;
            reasons.push(reason(
                ReasonCode::ExactDuplicate,
                Severity::Major,
                1.,
                None,
                None,
            ));
        }
    }
    if reasons.is_empty() {
        reasons.push(reason(
            ReasonCode::TechnicalUsable,
            Severity::Positive,
            confidence,
            None,
            None,
        ));
    }
    Ok(Scored {
        rating: star_mapping(value),
        absolute_score: absolute,
        ranking_score,
        score: value,
        confidence,
        reasons,
    })
}
