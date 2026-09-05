use super::{content::CONTENT_ALGORITHM, digest, score};
use photo_contracts::{culling::*, CancellationToken, ProcessingResult};
use std::collections::HashMap;
pub const SIMILARITY_VERSION: &str = "content-visual-time-anchor-v2";
fn timestamp(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|v| v.timestamp_millis())
        .ok()
        .or_else(|| {
            ["%Y:%m:%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"]
                .iter()
                .find_map(|f| {
                    chrono::NaiveDateTime::parse_from_str(s, f)
                        .ok()
                        .map(|t| t.and_utc().timestamp_millis())
                })
        })
}
#[derive(Clone, Copy, Debug)]
pub struct VisualMatch {
    pub kind: DuplicateKind,
    pub score: f64,
    pub confidence: f64,
}
pub fn classify(a: &SimilarityDescriptor, b: &SimilarityDescriptor) -> Option<VisualMatch> {
    if (a.aspect_ratio / b.aspect_ratio - 1.).abs() > 0.03 {
        return None;
    }
    let time = a
        .capture_timestamp
        .as_deref()
        .and_then(timestamp)
        .zip(b.capture_timestamp.as_deref().and_then(timestamp))
        .map(|(a, b)| (a - b).abs());
    let same_camera = a
        .camera
        .as_ref()
        .zip(b.camera.as_ref())
        .map(|(a, b)| a == b);
    let h = (u64::from_str_radix(&a.difference_hash, 16).ok()?
        ^ u64::from_str_radix(&b.difference_hash, 16).ok()?)
    .count_ones();
    let shape = a
        .luminance_grid
        .iter()
        .zip(&b.luminance_grid)
        .map(|(x, y)| ((x - a.mean_luminance) - (y - b.mean_luminance)).abs())
        .sum::<f64>()
        / 64.;
    let color = a
        .color_grid
        .iter()
        .zip(&b.color_grid)
        .map(|(x, y)| (x - y).abs())
        .sum::<f64>()
        / 48.;
    let contrast = |d: &SimilarityDescriptor| {
        d.luminance_grid
            .iter()
            .map(|v| (v - d.mean_luminance).abs())
            .sum::<f64>()
            / 64.
    };
    // Flat files may be exact byte copies, but a flat perceptual hash proves no visual relationship.
    if contrast(a).min(contrast(b)) < 0.008 || h > 14 || shape > 0.075 || color > 0.20 {
        return None;
    }
    let burst =
        time.is_some_and(|t| t <= 8000) && same_camera == Some(true) && h <= 10 && shape <= 0.055;
    let near = h <= 3
        && shape <= 0.02
        && color <= 0.04
        && time.is_none_or(|t| t <= 8000)
        && same_camera != Some(false);
    let kind = if burst {
        DuplicateKind::Burst
    } else if near {
        DuplicateKind::NearDuplicate
    } else {
        DuplicateKind::Similar
    };
    Some(VisualMatch {
        kind,
        score: (1. - (h as f64 / 64. + shape + color) / 3.).clamp(0., 1.),
        confidence: if burst {
            0.9
        } else if near {
            0.7
        } else {
            0.5
        },
    })
}
/// Compatibility helper for the existing strict visual/time comparison tests.
pub fn similarity(a: &SimilarityDescriptor, b: &SimilarityDescriptor) -> Option<f64> {
    classify(a, b)
        .filter(|m| m.kind != DuplicateKind::Similar)
        .map(|m| m.confidence)
}
pub fn exact_groups(
    entries: &[(String, DuplicateContent)],
    cancel: &CancellationToken,
) -> ProcessingResult<HashMap<String, ExactDuplicateRelationship>> {
    let mut buckets: HashMap<(String, u64), Vec<String>> = HashMap::new();
    for (id, c) in entries {
        cancel.check()?;
        c.validate().map_err(crate::rendering::internal)?;
        buckets
            .entry((c.sha256.clone(), c.byte_length))
            .or_default()
            .push(id.clone());
    }
    let mut results = HashMap::new();
    for ((sha256, byte_length), mut ids) in buckets.into_iter().filter(|(_, v)| v.len() > 1) {
        cancel.check()?;
        ids.sort();
        ids.dedup();
        if ids.len() < 2 {
            continue;
        }
        let e = ExactDuplicateRelationship {
            group_id: digest(&[
                CONTENT_ALGORITHM,
                &sha256,
                &byte_length.to_string(),
                &ids.join("|"),
            ]),
            group_size: ids.len() as u32,
            canonical_asset_id: ids[0].clone(),
            content: DuplicateContent {
                sha256,
                byte_length,
            },
        };
        for id in ids {
            results.insert(id, e.clone());
        }
    }
    Ok(results)
}
pub fn group(
    features: &[CullingFeatures],
    cancel: &CancellationToken,
) -> ProcessingResult<Vec<SimilarityContext>> {
    group_with_exact(features, &HashMap::new(), cancel)
}
/// Exact copies are collapsed to one representative for visual ranking, then expanded for comparison.
/// Global content buckets have no visual-window limitation; visual search is bounded to 64 anchors.
pub fn group_with_exact(
    features: &[CullingFeatures],
    exact: &HashMap<String, ExactDuplicateRelationship>,
    cancel: &CancellationToken,
) -> ProcessingResult<Vec<SimilarityContext>> {
    let mut families: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in features.iter().enumerate() {
        let id = exact
            .get(&f.asset_id)
            .map(|e| &e.canonical_asset_id)
            .unwrap_or(&f.asset_id);
        families.entry(id.clone()).or_default().push(i);
    }
    let mut order: Vec<usize> = (0..features.len())
        .filter(|&i| {
            exact
                .get(&features[i].asset_id)
                .is_none_or(|e| e.canonical_asset_id == features[i].asset_id)
        })
        .collect();
    order.sort_by(|&a, &b| {
        features[a]
            .descriptor
            .capture_timestamp
            .cmp(&features[b].descriptor.capture_timestamp)
            .then(features[a].asset_id.cmp(&features[b].asset_id))
    });
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in order {
        cancel.check()?;
        if let Some(g) = groups.iter_mut().rev().take(64).find(|g| {
            g.len() < 32 && classify(&features[g[0]].descriptor, &features[i].descriptor).is_some()
        }) {
            g.push(i);
        } else {
            groups.push(vec![i]);
        }
    }
    let mut contexts: Vec<_> = features
        .iter()
        .map(|f| SimilarityContext {
            exact: exact.get(&f.asset_id).cloned(),
            ..Default::default()
        })
        .collect();
    for representatives in groups.into_iter().filter(|g| g.len() > 1) {
        cancel.check()?;
        let members: Vec<usize> = representatives
            .iter()
            .flat_map(|&i| families[&features[i].asset_id].iter().copied())
            .collect();
        let mut ids: Vec<_> = members
            .iter()
            .map(|&i| {
                format!(
                    "{}:{}:{}",
                    features[i].asset_id,
                    features[i].source_fingerprint,
                    features[i].source_analysis_id
                )
            })
            .collect();
        ids.sort();
        let id = digest(&[SIMILARITY_VERSION, &ids.join("|")]);
        let matches: Vec<_> = representatives
            .iter()
            .skip(1)
            .filter_map(|&i| {
                classify(
                    &features[representatives[0]].descriptor,
                    &features[i].descriptor,
                )
            })
            .collect();
        let kind = if matches.iter().any(|m| m.kind == DuplicateKind::Similar) {
            DuplicateKind::Similar
        } else if matches.iter().all(|m| m.kind == DuplicateKind::Burst) {
            DuplicateKind::Burst
        } else {
            DuplicateKind::NearDuplicate
        };
        let scores: Vec<_> = representatives
            .iter()
            .map(|&i| {
                score::score(&features[i], &SimilarityContext::default()).map(|s| s.ranking_score)
            })
            .collect::<Result<_, _>>()
            .map_err(crate::rendering::internal)?;
        let best = scores.iter().copied().fold(0f64, f64::max);
        let preferred: Vec<_> = representatives
            .iter()
            .zip(&scores)
            .filter(|(_, s)| best - **s <= 1.)
            .map(|(&i, _)| features[i].asset_id.clone())
            .collect();
        let min = representatives
            .iter()
            .map(|&i| features[i].descriptor.mean_luminance)
            .fold(1f64, f64::min);
        let max = representatives
            .iter()
            .map(|&i| features[i].descriptor.mean_luminance)
            .fold(0f64, f64::max);
        let confidence = matches.iter().map(|m| m.confidence).fold(1f64, f64::min);
        let similarity_score = matches.iter().map(|m| m.score).fold(1f64, f64::min);
        for (&representative, s) in representatives.iter().zip(scores) {
            for &i in &families[&features[representative].asset_id] {
                contexts[i] = SimilarityContext {
                    group_id: Some(id.clone()),
                    group_size: members.len() as u32,
                    preferred: preferred.contains(&features[i].asset_id),
                    preferred_assets: preferred.clone(),
                    relative_score: Some(best - s),
                    confidence,
                    bracket_like: kind == DuplicateKind::Burst && max - min > 0.12,
                    kind,
                    similarity_score: Some(similarity_score),
                    exact: exact.get(&features[i].asset_id).cloned(),
                };
            }
        }
    }
    Ok(contexts)
}
