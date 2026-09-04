//! Versioned, portable per-image editing intent. No pixels, file paths, UI or analysis.
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const RECIPE_SCHEMA_VERSION: u32 = 1;
pub const MAX_RECIPE_BYTES: usize = 256 * 1024;
pub const MASK_GEOMETRY_VERSION: &str = "oriented-source-optics-geometry-v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BasicAdjustments {
    pub exposure_ev: f32,
    pub temperature: f32,
    pub tint: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub vibrance: f32,
}
impl Default for BasicAdjustments {
    fn default() -> Self {
        let a = RenderAdjustments::default();
        Self {
            exposure_ev: a.exposure_ev,
            temperature: a.temperature,
            tint: a.tint,
            contrast: a.contrast,
            highlights: a.highlights,
            shadows: a.shadows,
            whites: a.whites,
            blacks: a.blacks,
            saturation: a.saturation,
            vibrance: a.vibrance,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorMixer {
    pub red: HslBand,
    pub orange: HslBand,
    pub yellow: HslBand,
    pub green: HslBand,
    pub aqua: HslBand,
    pub blue: HslBand,
    pub purple: HslBand,
    pub magenta: HslBand,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecipeDetail {
    pub sharpening: Sharpening,
    pub noise: NoiseReduction,
    /// Phase 2's earlier pipeline stages. Not aliases of the expanded detail controls.
    pub legacy_sharpening: f32,
    pub legacy_noise_reduction: f32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Geometry {
    pub rotation_degrees: f32,
    /// Fraction of the rotated canvas, not pixels.
    pub crop: Crop,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecipeGlobal {
    pub basic: BasicAdjustments,
    pub curve: ToneCurve,
    pub color_mixer: ColorMixer,
    pub presence: Presence,
    pub detail: RecipeDetail,
    /// Objective correction preferences; resolved profile belongs to render diagnostics.
    pub optics: Optics,
    pub effects: Effects,
    pub geometry: Geometry,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaskReference {
    pub content_id: String,
    #[serde(default)]
    pub source_fingerprint: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub model_version: Option<String>,
    #[serde(default)]
    pub geometry_version: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeLayer {
    pub id: String,
    pub mask_type: MaskType,
    pub enabled: bool,
    pub strength: f32,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub mask_reference: Option<MaskReference>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub adjustments: LocalAdjustments,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecipeMetadata {
    pub scene_cluster_id: Option<String>,
    pub sequence_id: Option<String>,
    pub reference_asset_id: Option<String>,
    pub consistency_group_id: Option<String>,
    pub consistency_note: Option<String>,
    pub confidence: Option<f32>,
    pub needs_review: Option<bool>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeOrigin {
    Manual,
    Imported,
    Migrated,
    #[default]
    System,
    TrainedStyle,
    AiGenerated,
    Correction,
    BatchConsistency,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecipeProvenance {
    pub origin: RecipeOrigin,
    pub created_by: Option<String>,
    pub source_recipe_id: Option<String>,
    pub style_id: Option<String>,
    pub model_id: Option<String>,
    pub model_version: Option<String>,
    pub analysis_id: Option<String>,
    pub manually_modified: bool,
    /// Reserved evidence, never inferred by the application.
    pub acceptance: Option<RecipeAcceptance>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeAcceptance {
    Accepted,
    Rejected,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRecipe {
    pub schema_version: u32,
    pub recipe_id: String,
    pub asset_id: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub global: RecipeGlobal,
    #[serde(default)]
    pub local_layers: Vec<RecipeLayer>,
    #[serde(default)]
    pub metadata: RecipeMetadata,
    #[serde(default)]
    pub provenance: RecipeProvenance,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct RecipeError {
    pub code: RecipeErrorCode,
    pub message: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeErrorCode {
    InvalidRecipe,
    UnsupportedVersion,
    CorruptStoredRecipe,
    Conflict,
    UnresolvedMask,
}
pub type RecipeResult<T> = Result<T, RecipeError>;
impl RecipeError {
    pub fn new(code: RecipeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl From<ProcessingError> for RecipeError {
    fn from(e: ProcessingError) -> Self {
        invalid(e.message)
    }
}
impl From<RecipeError> for ProcessingError {
    fn from(e: RecipeError) -> Self {
        Self::new(
            ProcessingErrorCode::InvalidAdjustments,
            format!("{:?}: {}", e.code, e.message),
        )
    }
}
fn invalid(message: impl Into<String>) -> RecipeError {
    RecipeError::new(RecipeErrorCode::InvalidRecipe, message)
}
fn json_error(e: serde_json::Error) -> RecipeError {
    invalid(e.to_string())
}
fn hex_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl EditRecipe {
    pub fn neutral(recipe_id: String, asset_id: String, timestamp: String) -> Self {
        Self {
            schema_version: RECIPE_SCHEMA_VERSION,
            recipe_id,
            asset_id,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            global: RecipeGlobal::default(),
            local_layers: Vec::new(),
            metadata: RecipeMetadata::default(),
            provenance: RecipeProvenance::default(),
        }
    }
    /// Lossless bridge for both Phase 2 and Phase 2.1 persisted adjustment payloads.
    pub fn with_adjustments(mut self, input: &RenderAdjustments) -> RecipeResult<Self> {
        let a = input.validated()?;
        self.global = RecipeGlobal {
            basic: BasicAdjustments {
                exposure_ev: a.exposure_ev,
                temperature: a.temperature,
                tint: a.tint,
                contrast: a.contrast,
                highlights: a.highlights,
                shadows: a.shadows,
                whites: a.whites,
                blacks: a.blacks,
                saturation: a.saturation,
                vibrance: a.vibrance,
            },
            curve: a.curve,
            color_mixer: ColorMixer {
                red: a.hsl[0],
                orange: a.hsl[1],
                yellow: a.hsl[2],
                green: a.hsl[3],
                aqua: a.hsl[4],
                blue: a.hsl[5],
                purple: a.hsl[6],
                magenta: a.hsl[7],
            },
            presence: a.presence,
            detail: RecipeDetail {
                sharpening: a.detail.sharpening,
                noise: a.detail.noise,
                legacy_sharpening: a.sharpening,
                legacy_noise_reduction: a.noise_reduction,
            },
            optics: a.optics,
            effects: a.effects,
            geometry: Geometry {
                rotation_degrees: a.rotation_degrees,
                crop: a.crop,
            },
        };
        self.local_layers = a
            .local_layers
            .into_iter()
            .map(|l| {
                // Retain richer binding metadata when adapting existing controls.
                let previous = self.local_layers.iter().find(|old| old.id == l.id);
                let binding = l.mask_reference.map(|id| {
                    previous
                        .and_then(|p| p.mask_reference.as_ref())
                        .filter(|m| m.content_id == id)
                        .cloned()
                        .unwrap_or(MaskReference {
                            content_id: id,
                            source_fingerprint: None,
                            model_id: None,
                            model_version: None,
                            geometry_version: None,
                        })
                });
                RecipeLayer {
                    id: l.id,
                    mask_type: l.mask_type,
                    enabled: l.enabled,
                    strength: l.strength,
                    invert: l.invert,
                    confidence: l.confidence,
                    mask_reference: binding,
                    adjustments: l.adjustments,
                }
            })
            .collect();
        if let Some(b) = a.batch_context {
            self.metadata.scene_cluster_id = b.scene_cluster_id;
            self.metadata.sequence_id = b.sequence_id;
            self.metadata.reference_asset_id = b.reference_asset_id;
            self.metadata.consistency_note = b.consistency_note;
        }
        Ok(self)
    }
    fn parameters(&self) -> RenderAdjustments {
        let g = &self.global;
        RenderAdjustments {
            exposure_ev: g.basic.exposure_ev,
            temperature: g.basic.temperature,
            tint: g.basic.tint,
            contrast: g.basic.contrast,
            highlights: g.basic.highlights,
            shadows: g.basic.shadows,
            whites: g.basic.whites,
            blacks: g.basic.blacks,
            saturation: g.basic.saturation,
            vibrance: g.basic.vibrance,
            curve: g.curve.clone(),
            hsl: [
                g.color_mixer.red,
                g.color_mixer.orange,
                g.color_mixer.yellow,
                g.color_mixer.green,
                g.color_mixer.aqua,
                g.color_mixer.blue,
                g.color_mixer.purple,
                g.color_mixer.magenta,
            ],
            presence: g.presence,
            detail: Detail {
                sharpening: g.detail.sharpening,
                noise: g.detail.noise,
            },
            sharpening: g.detail.legacy_sharpening,
            noise_reduction: g.detail.legacy_noise_reduction,
            optics: g.optics,
            effects: g.effects,
            crop: g.geometry.crop,
            rotation_degrees: g.geometry.rotation_degrees,
            local_layers: self
                .local_layers
                .iter()
                .map(|l| LocalAdjustmentLayer {
                    id: l.id.clone(),
                    mask_type: l.mask_type,
                    enabled: l.enabled,
                    strength: l.strength,
                    invert: l.invert,
                    confidence: l.confidence,
                    mask_reference: l.mask_reference.as_ref().map(|m| m.content_id.clone()),
                    adjustments: l.adjustments.clone(),
                })
                .collect(),
            batch_context: if self.metadata.scene_cluster_id.is_some()
                || self.metadata.sequence_id.is_some()
                || self.metadata.reference_asset_id.is_some()
                || self.metadata.consistency_note.is_some()
            {
                Some(BatchContext {
                    scene_cluster_id: self.metadata.scene_cluster_id.clone(),
                    sequence_id: self.metadata.sequence_id.clone(),
                    reference_asset_id: self.metadata.reference_asset_id.clone(),
                    consistency_note: self.metadata.consistency_note.clone(),
                })
            } else {
                None
            },
            ..Default::default()
        }
    }
    pub fn adjustments(&self) -> RecipeResult<RenderAdjustments> {
        Ok(self.validated()?.parameters())
    }
    pub fn validated(&self) -> RecipeResult<Self> {
        if self.schema_version != RECIPE_SCHEMA_VERSION {
            return Err(RecipeError::new(
                RecipeErrorCode::UnsupportedVersion,
                format!(
                    "Unsupported recipe schema {}; this application supports {}",
                    self.schema_version, RECIPE_SCHEMA_VERSION
                ),
            ));
        }
        for (name, s) in [("recipe_id", &self.recipe_id), ("asset_id", &self.asset_id)] {
            if s.is_empty() || s.len() > 128 || s.chars().any(char::is_control) {
                return Err(invalid(format!("{name} must be 1..128 printable bytes")));
            }
        }
        for time in [&self.created_at, &self.updated_at] {
            if chrono::DateTime::parse_from_rfc3339(time).is_err() {
                return Err(invalid("Recipe timestamps must be RFC 3339"));
            }
        }
        if self
            .metadata
            .confidence
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(invalid("Confidence must be finite and in 0..1"));
        }
        for l in &self.local_layers {
            if let Some(m) = &l.mask_reference {
                if !hex_id(&m.content_id)
                    || m.source_fingerprint.as_ref().is_some_and(|s| !hex_id(s))
                {
                    return Err(invalid(
                        "Mask content/source identities must be SHA-256, not paths",
                    ));
                }
            }
        }
        let a = self.parameters().validated()?;
        let mut normalized = self.clone();
        normalized.global.geometry.rotation_degrees = a.rotation_degrees;
        for points in [
            &mut normalized.global.curve.master,
            &mut normalized.global.curve.red,
            &mut normalized.global.curve.green,
            &mut normalized.global.curve.blue,
        ] {
            if points.iter().all(|p| p.x == p.y) {
                *points = ToneCurve::default().master;
            }
        }
        let mut value = serde_json::to_value(&normalized).map_err(json_error)?;
        normalize_json(&mut value)?;
        let bytes = serde_json::to_vec(&value).map_err(json_error)?;
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(invalid("Recipe exceeds 256 KiB"));
        }
        serde_json::from_value(value).map_err(json_error)
    }
    pub fn canonical_json(&self) -> RecipeResult<String> {
        // serde_json::Map uses sorted keys (preserve_order is deliberately not enabled).
        let mut value = serde_json::to_value(self.validated()?).map_err(json_error)?;
        normalize_json(&mut value)?;
        serde_json::to_string(&value).map_err(json_error)
    }
    /// Intent identity excludes asset/recipe/layer IDs, clocks, provenance and review metadata.
    /// Derived mask bytes and profile dependencies belong to effective render identity instead.
    pub fn content_hash(&self) -> RecipeResult<String> {
        let n = self.validated()?;
        let layers: Vec<_> = n
            .local_layers
            .iter()
            .filter(|l| l.enabled && l.strength > 0.)
            .map(|l| {
                serde_json::json!({"mask_type":l.mask_type,"strength":l.strength,"invert":l.invert,
                "mask_reference":l.mask_reference,"adjustments":l.adjustments})
            })
            .collect();
        let mut value = serde_json::json!({"contract":"photo-recipe-intent-v1","global":n.global,"local_layers":layers});
        normalize_json(&mut value)?;
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&value).map_err(json_error)?)
        ))
    }
}
fn normalize_json(v: &mut Value) -> RecipeResult<()> {
    match v {
        Value::Number(n) if n.as_f64() == Some(0.) => *v = serde_json::json!(0),
        Value::String(s) if s.len() > 1024 => {
            return Err(invalid("Recipe string exceeds 1024 bytes"))
        }
        Value::Object(map) => {
            map.sort_keys();
            for v in map.values_mut() {
                normalize_json(v)?;
            }
        }
        Value::Array(list) => {
            for v in list {
                normalize_json(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// v0 is an explicit interchange bridge around legacy adjustments, not the old unused operations placeholder.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRecipe {
    schema_version: u32,
    recipe_id: String,
    asset_id: String,
    created_at: String,
    updated_at: String,
    adjustments: RenderAdjustments,
}
pub fn parse_recipe(json: &str) -> RecipeResult<EditRecipe> {
    if json.len() > MAX_RECIPE_BYTES {
        return Err(invalid("Recipe exceeds 256 KiB"));
    }
    let value: Value = serde_json::from_str(json).map_err(json_error)?;
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("schema_version is required and must be an unsigned integer"))?;
    let recipe: EditRecipe = match version {
        0 => {
            let old: LegacyRecipe = serde_json::from_value(value).map_err(json_error)?;
            debug_assert_eq!(old.schema_version, 0);
            let mut recipe = EditRecipe::neutral(old.recipe_id, old.asset_id, old.created_at)
                .with_adjustments(&old.adjustments)?;
            recipe.updated_at = old.updated_at;
            recipe.provenance.origin = RecipeOrigin::Migrated;
            recipe
        }
        1 => serde_json::from_value(value).map_err(json_error)?,
        _ => {
            return Err(RecipeError::new(
                RecipeErrorCode::UnsupportedVersion,
                format!("Unsupported recipe schema {version}; supported: 0 (legacy bridge), 1"),
            ))
        }
    };
    recipe.validated()
}

/// Transferable concrete editing intent, not a trained style or image analysis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeTemplate {
    pub global: RecipeGlobal,
    pub local_layers: Vec<RecipeLayer>,
}
impl RecipeTemplate {
    pub fn from_recipe(r: &EditRecipe) -> RecipeResult<Self> {
        let r = r.validated()?;
        Ok(Self {
            global: r.global,
            local_layers: r
                .local_layers
                .into_iter()
                .map(|mut l| {
                    l.mask_reference = None;
                    l.confidence = None;
                    l
                })
                .collect(),
        })
    }
    pub fn instantiate(
        &self,
        recipe_id: String,
        asset_id: String,
        timestamp: String,
    ) -> RecipeResult<EditRecipe> {
        let mut r = EditRecipe::neutral(recipe_id, asset_id, timestamp);
        r.global = self.global.clone();
        r.local_layers = self.local_layers.clone();
        // Never trust asset-bound references in a deserialized template.
        for l in &mut r.local_layers {
            l.mask_reference = None;
            l.confidence = None;
        }
        r.validated()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecipeDifference {
    pub control: String,
    pub before: Value,
    pub after: Value,
}
/// Compares typed semantic groups, aligns locals by stable ID, reports ordering separately.
pub fn diff_recipes(
    before: &EditRecipe,
    after: &EditRecipe,
) -> RecipeResult<Vec<RecipeDifference>> {
    let a = before.validated()?;
    let b = after.validated()?;
    let mut changes = Vec::new();
    diff_values(
        "",
        &serde_json::to_value(a.global).map_err(json_error)?,
        &serde_json::to_value(b.global).map_err(json_error)?,
        &mut changes,
    );
    let order_a: Vec<_> = a.local_layers.iter().map(|l| &l.id).collect();
    let order_b: Vec<_> = b.local_layers.iter().map(|l| &l.id).collect();
    if order_a != order_b {
        changes.push(RecipeDifference {
            control: "Local layer order".into(),
            before: serde_json::json!(order_a),
            after: serde_json::json!(order_b),
        });
    }
    let ids: std::collections::BTreeSet<_> = a
        .local_layers
        .iter()
        .chain(&b.local_layers)
        .map(|l| &l.id)
        .collect();
    for id in ids {
        let aa = a.local_layers.iter().find(|l| &l.id == id);
        let bb = b.local_layers.iter().find(|l| &l.id == id);
        let label = format!(
            "{:?} [{}]",
            bb.or(aa).expect("union contains layer").mask_type,
            id
        );
        let mut av = serde_json::to_value(aa).map_err(json_error)?;
        let mut bv = serde_json::to_value(bb).map_err(json_error)?;
        for v in [&mut av, &mut bv] {
            if let Some(o) = v.as_object_mut() {
                o.remove("confidence");
            }
        }
        diff_values(&label, &av, &bv, &mut changes);
    }
    Ok(changes)
}
fn diff_values(label: &str, a: &Value, b: &Value, out: &mut Vec<RecipeDifference>) {
    if a == b {
        return;
    }
    if let (Some(aa), Some(bb)) = (a.as_object(), b.as_object()) {
        for key in aa
            .keys()
            .chain(bb.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            let name = match key.as_str() {
                "exposure_ev" => "Exposure (EV)".into(),
                "temperature" => "Temperature (relative K)".into(),
                "color_mixer" => "Color mixer".into(),
                "rotation_degrees" => "Rotation (degrees)".into(),
                _ => key.replace('_', " "),
            };
            diff_values(
                format!("{label} / {name}").trim_start_matches(" / "),
                aa.get(key).unwrap_or(&Value::Null),
                bb.get(key).unwrap_or(&Value::Null),
                out,
            );
        }
    } else {
        out.push(RecipeDifference {
            control: label.into(),
            before: a.clone(),
            after: b.clone(),
        });
    }
}
