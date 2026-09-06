use crate::models::Asset;
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn natural_cmp(left: &Path, right: &Path) -> Ordering {
    let a = left.to_string_lossy().to_lowercase();
    let b = right.to_string_lossy().to_lowercase();
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (start_a, start_b) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let mut x = start_a;
            let mut y = start_b;
            while x < i && a[x] == b'0' {
                x += 1;
            }
            while y < j && b[y] == b'0' {
                y += 1;
            }
            let order = (i - x).cmp(&(j - y)).then_with(|| a[x..i].cmp(&b[y..j]));
            if order != Ordering::Equal {
                return order;
            }
        } else {
            let order = a[i].cmp(&b[j]);
            if order != Ordering::Equal {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i)
        .cmp(&(b.len() - j))
        .then_with(|| left.cmp(right))
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairMatchCandidate {
    pub source_asset_id: String,
    pub source_filename: String,
    #[serde(default)]
    pub source_path: Option<PathBuf>,
    pub reference_path: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutoMatchResult {
    pub matched: Vec<PairMatchCandidate>,
    pub ambiguous_sources: Vec<String>,
    pub unmatched_references: Vec<PathBuf>,
    #[serde(default)]
    pub unmatched_sources: Vec<String>,
    #[serde(default)]
    pub before_count: usize,
    #[serde(default)]
    pub after_count: usize,
    #[serde(default)]
    pub start_aligned: bool,
    #[serde(default)]
    pub end_aligned: bool,
    #[serde(default)]
    pub order_fallback_used: bool,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

fn training_reference(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "tif" | "tiff" | "png"
            )
        })
}

pub fn normalized_stem(path: &Path) -> Option<String> {
    let mut stem = path.file_stem()?.to_str()?.trim().to_ascii_lowercase();
    for suffix in [
        "_edited", "-edited", " edited", "_edit", "-edit", " edit", "_final", "-final", " final",
    ] {
        if stem.ends_with(suffix) {
            stem.truncate(stem.len() - suffix.len());
            break;
        }
    }
    let normalized = stem
        .trim_end_matches(['_', '-', ' '])
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    (!normalized.is_empty()).then_some(normalized)
}

pub fn auto_match(assets: &[Asset], folder: &Path) -> AutoMatchResult {
    let mut references = walkdir::WalkDir::new(folder)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && training_reference(entry.path()))
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    references.sort_by(|a, b| natural_cmp(a, b));
    let mut by_stem = BTreeMap::<String, Vec<PathBuf>>::new();
    for reference in &references {
        if let Some(stem) = normalized_stem(reference) {
            by_stem.entry(stem).or_default().push(reference.clone());
        }
    }
    let mut source_counts = BTreeMap::<String, usize>::new();
    for asset in assets {
        if let Some(stem) = normalized_stem(Path::new(&asset.filename)) {
            *source_counts.entry(stem).or_default() += 1;
        }
    }
    let mut matched = Vec::new();
    let mut ambiguous_sources = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    for asset in assets {
        let Some(stem) = normalized_stem(Path::new(&asset.filename)) else {
            continue;
        };
        let Some(candidates) = by_stem.get(&stem) else {
            continue;
        };
        if candidates.len() == 1 && source_counts.get(&stem) == Some(&1) {
            used.insert(candidates[0].clone());
            matched.push(PairMatchCandidate {
                source_asset_id: asset.id.clone(),
                source_filename: asset.filename.clone(),
                source_path: Some(asset.original_path.clone()),
                reference_path: candidates[0].clone(),
            });
        } else {
            ambiguous_sources.push(asset.filename.clone());
        }
    }
    let unmatched_references = references
        .iter()
        .filter(|reference| !used.contains(*reference))
        .cloned()
        .collect();
    let unmatched_sources = assets
        .iter()
        .filter(|asset| {
            !matched
                .iter()
                .any(|candidate| candidate.source_asset_id == asset.id)
        })
        .map(|asset| asset.filename.clone())
        .collect();
    AutoMatchResult {
        matched,
        ambiguous_sources,
        unmatched_references,
        unmatched_sources,
        before_count: assets.len(),
        after_count: references.len(),
        start_aligned: false,
        end_aligned: false,
        order_fallback_used: false,
        diagnostics: Vec::new(),
    }
}

/// Match standalone before/after selections. Exact normalized identity is
/// authoritative; order fallback is offered only when there are no exact
/// matches and the file counts agree, so a missing middle file cannot shift
/// every later pair.
pub fn match_paths(before: &[PathBuf], after: &[PathBuf]) -> AutoMatchResult {
    let mut before = before.to_vec();
    let mut after = after.to_vec();
    before.sort_by(|a, b| natural_cmp(a, b));
    after.sort_by(|a, b| natural_cmp(a, b));
    let first_before = before.first().and_then(|path| normalized_stem(path));
    let first_after = after.first().and_then(|path| normalized_stem(path));
    let last_before = before.last().and_then(|path| normalized_stem(path));
    let last_after = after.last().and_then(|path| normalized_stem(path));
    let start_aligned = first_before.is_some() && first_before == first_after;
    let end_aligned = last_before.is_some() && last_before == last_after;
    let mut by_stem = BTreeMap::<String, Vec<PathBuf>>::new();
    for path in &after {
        if let Some(stem) = normalized_stem(path) {
            by_stem.entry(stem).or_default().push(path.clone());
        }
    }
    let mut before_counts = BTreeMap::<String, usize>::new();
    for path in &before {
        if let Some(stem) = normalized_stem(path) {
            *before_counts.entry(stem).or_default() += 1;
        }
    }
    let mut matched = Vec::new();
    let mut ambiguous_sources = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    for path in &before {
        let Some(stem) = normalized_stem(path) else {
            continue;
        };
        let Some(candidates) = by_stem.get(&stem) else {
            continue;
        };
        if candidates.len() == 1 && before_counts.get(&stem) == Some(&1) {
            used.insert(candidates[0].clone());
            matched.push(PairMatchCandidate {
                source_asset_id: String::new(),
                source_filename: path
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or_default()
                    .into(),
                source_path: Some(path.clone()),
                reference_path: candidates[0].clone(),
            });
        } else {
            ambiguous_sources.push(path.clone().to_string_lossy().into_owned());
        }
    }
    let mut order_fallback_used = false;
    let mut diagnostics = Vec::new();
    if matched.is_empty()
        && ambiguous_sources.is_empty()
        && before.len() == after.len()
        && !before.is_empty()
    {
        order_fallback_used = true;
        diagnostics.push("Filenames differ; ordered candidates require structural validation before becoming Ready".into());
        for (source, reference) in before.iter().zip(after.iter()) {
            matched.push(PairMatchCandidate {
                source_asset_id: String::new(),
                source_filename: source
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or_default()
                    .into(),
                source_path: Some(source.clone()),
                reference_path: reference.clone(),
            });
        }
    }
    let unmatched_sources = before
        .iter()
        .filter(|source| {
            !matched
                .iter()
                .any(|candidate| candidate.source_path.as_ref() == Some(source))
        })
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let unmatched_references = after
        .iter()
        .filter(|reference| {
            !used.contains(*reference)
                && !matched
                    .iter()
                    .any(|candidate| &candidate.reference_path == *reference)
        })
        .cloned()
        .collect::<Vec<_>>();
    AutoMatchResult {
        matched,
        ambiguous_sources,
        unmatched_references,
        unmatched_sources,
        before_count: before.len(),
        after_count: after.len(),
        start_aligned,
        end_aligned,
        order_fallback_used,
        diagnostics,
    }
}
