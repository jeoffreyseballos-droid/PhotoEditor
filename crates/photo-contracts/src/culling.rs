//! Source-selection guidance, never editing intent or facial identity.
use crate::analysis::{BoundingBox, PhotoType, ProviderIdentity};
use serde::{Deserialize, Serialize};
pub const CULLING_SCHEMA_VERSION: u32 = 2;
pub const MAX_CULLING_BYTES: usize = 128 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Stars(u8);
impl Stars {
    pub fn new(v: u8) -> Result<Self, String> {
        if (1..=5).contains(&v) {
            Ok(Self(v))
        } else {
            Err("Stars must be 1 through 5".into())
        }
    }
    pub fn get(self) -> u8 {
        self.0
    }
}
impl TryFrom<u8> for Stars {
    type Error = String;
    fn try_from(v: u8) -> Result<Self, String> {
        Self::new(v)
    }
}
impl From<Stars> for u8 {
    fn from(v: Stars) -> Self {
        v.0
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Signal<T> {
    Available { value: T, confidence: f64 },
    Unavailable { reason: String },
    NotApplicable { reason: String },
    Failed { reason: String },
    Uncertain { reason: String },
}
impl<T> Signal<T> {
    pub fn available(value: T, confidence: f64) -> Self {
        Self::Available { value, confidence }
    }
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }
    pub fn value(&self) -> Option<&T> {
        if let Self::Available { value, .. } = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn confidence(&self) -> f64 {
        if let Self::Available { confidence, .. } = self {
            *confidence
        } else {
            0.
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EyeState {
    Open,
    Closed,
    Uncertain,
    NotVisible,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Positive,
    Info,
    Review,
    Issue,
    Major,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    TechnicalUsable,
    SourceUnavailable,
    InsufficientEvidence,
    FaceDetectorUnavailable,
    NoFacesDetected,
    EyesOpen,
    EyesClosed,
    EyesUncertain,
    GroupIntegrity,
    FaceSoft,
    FaceSharp,
    FaceNearEdge,
    FacePartlyClipped,
    SubjectNearEdge,
    LowTextureOrBlur,
    DirectionalDetail,
    ExposureReview,
    SevereClipping,
    NoiseReview,
    LevelReview,
    SimilarAlternative,
    PreferredCandidate,
    BracketLike,
    SelectionUnchanged,
    ExactDuplicate,
    PreferredCopy,
    NearDuplicate,
    BurstAlternative,
    SimilarComposition,
    BurstSequence,
    DuplicateIdentityUnavailable,
    SevereSubjectSoftness,
    GroupFocusReference,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateKind {
    Exact,
    NearDuplicate,
    Burst,
    Similar,
    Unique,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasonMeasurement {
    pub value: f64,
    pub unit: String,
    pub reference: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CullingReason {
    pub code: ReasonCode,
    pub severity: Severity,
    pub confidence: f64,
    pub subject_index: Option<u32>,
    pub measurement: Option<ReasonMeasurement>,
}
macro_rules! record{($name:ident{$($f:ident:$t:ty),*$(,)?})=>{
    #[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub struct $name{$(pub $f:$t),*}
    impl SafeLeaves for $name{fn safe(&self)->bool{$(self.$f.safe())&&*}}
}}
record!(FaceFeatures{index:u32,bbox:BoundingBox,detection_confidence:f64,sharpness:Signal<f64>,mean_luminance:f64,highlight_clip_fraction:f64,shadow_clip_fraction:f64,eyes:Signal<EyeState>,edge_distance:f64,visible_fraction:f64,relevant:bool});
record!(PeopleFeatures{faces:Signal<Vec<FaceFeatures>>,softest_subject:Option<u32>,face_sharpness_spread:Signal<f64>,outlier_subjects:Vec<u32>});
record!(TechnicalFeatures{global_sharpness:f64,global_edge_strength:f64,noise_severity:Signal<f64>,directional_detail:Signal<f64>,subject_sharpness:Signal<f64>});
record!(ExposureFeatures{median_luminance:f64,highlight_clip_fraction:f64,shadow_clip_fraction:f64,tonal_range:f64,subject_background_ev:Signal<f64>});
record!(FramingFeatures{subject_edge_distance:Signal<f64>,subject_occupancy:Signal<f64>});
record!(CompositionFeatures{level_angle:Signal<f64>,aspect_ratio:f64});
record!(SimilarityDescriptor{difference_hash:String,luminance_grid:Vec<f64>,color_grid:Vec<f64>,aspect_ratio:f64,capture_timestamp:Option<String>,camera:Option<String>,mean_luminance:f64});
record!(CullingFeatures{asset_id:String,photo_type:PhotoType,source_fingerprint:String,source_analysis_id:String,source_analysis_version:u32,feature_version:String,models:Vec<ProviderIdentity>,technical:TechnicalFeatures,people:PeopleFeatures,framing:FramingFeatures,composition:CompositionFeatures,exposure:ExposureFeatures,descriptor:SimilarityDescriptor});
record!(DuplicateContent {
    sha256: String,
    byte_length: u64
});
impl DuplicateContent {
    pub fn validate(&self) -> Result<(), CullingError> {
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CullingError::Invalid(
                "Invalid full-file SHA-256 identity".into(),
            ));
        }
        Ok(())
    }
}
record!(ExactDuplicateRelationship {
    group_id: String,
    group_size: u32,
    canonical_asset_id: String,
    content: DuplicateContent
});
record!(SimilarityContext{group_id:Option<String>,group_size:u32,preferred:bool,preferred_assets:Vec<String>,relative_score:Option<f64>,confidence:f64,bracket_like:bool,kind:DuplicateKind,similarity_score:Option<f64>,exact:Option<ExactDuplicateRelationship>});
impl Default for SimilarityContext {
    fn default() -> Self {
        Self {
            group_id: None,
            group_size: 1,
            preferred: false,
            preferred_assets: vec![],
            relative_score: None,
            confidence: 0.,
            bracket_like: false,
            kind: DuplicateKind::Unique,
            similarity_score: None,
            exact: None,
        }
    }
}
impl SimilarityContext {
    pub fn validate(&self) -> Result<(), CullingError> {
        let bad = || CullingError::Invalid("Invalid similarity context".into());
        if !self.safe()
            || !(0. ..=1.).contains(&self.confidence)
            || self
                .relative_score
                .is_some_and(|s| !(0. ..=100.).contains(&s))
        {
            return Err(bad());
        }
        if self.kind == DuplicateKind::Exact
            || self
                .similarity_score
                .is_some_and(|s| !(0. ..=1.).contains(&s))
        {
            return Err(bad());
        }
        if let Some(e) = &self.exact {
            e.content.validate()?;
            if e.group_id.len() != 64
                || !e.group_id.bytes().all(|b| b.is_ascii_hexdigit())
                || !(2..=5000).contains(&e.group_size)
                || e.canonical_asset_id.is_empty()
                || e.canonical_asset_id.len() > 128
            {
                return Err(bad());
            }
        }
        if let Some(id) = &self.group_id {
            let unique: std::collections::HashSet<_> = self.preferred_assets.iter().collect();
            if id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !(2..=5000).contains(&self.group_size)
                || self.kind == DuplicateKind::Unique
                || self.similarity_score.is_none()
                || self.preferred_assets.is_empty()
                || unique.len() != self.preferred_assets.len()
                || unique.len() > self.group_size as usize
                || self.relative_score.is_none()
            {
                return Err(bad());
            }
        } else if self.group_size != 1
            || self.preferred
            || !self.preferred_assets.is_empty()
            || self.relative_score.is_some()
            || self.bracket_like
            || self.kind != DuplicateKind::Unique
            || self.similarity_score.is_some()
        {
            return Err(bad());
        }
        Ok(())
    }
}
record!(CullingAssessment{schema_version:u32,assessment_id:String,asset_id:String,created_at:String,photo_type:PhotoType,ai_rating:Option<Stars>,confidence:f64,absolute_score:f64,final_score:f64,reasons:Vec<CullingReason>,features:Option<CullingFeatures>,similarity:SimilarityContext,culling_engine_version:String,model_versions:Vec<ProviderIdentity>,source_analysis_id:Option<String>,source_fingerprint:String,cache_key:String,duplicate_content:Option<DuplicateContent>,duplicate_stamp:Option<String>,membership_key:Option<String>});
// User-owned state is composed at read time, never embedded into the immutable AI snapshot.
record!(CullingState{assessment:Option<CullingAssessment>,user_rating:Option<Stars>,effective_rating:Option<Stars>,selected_for_editing:bool,stale:bool,updated_at:Option<String>});
impl CullingState {
    pub fn effective(ai: Option<Stars>, user: Option<Stars>) -> Option<Stars> {
        user.or(ai)
    }
}
record!(SimilarityGroup{group_id:String,asset_ids:Vec<String>,preferred_assets:Vec<String>,confidence:f64,bracket_like:bool,engine_version:String});
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum CullingError {
    #[error("Unsupported culling version: {0}")]
    UnsupportedVersion(u32),
    #[error("Invalid culling data: {0}")]
    Invalid(String),
}
impl CullingAssessment {
    pub fn validate(&self) -> Result<(), CullingError> {
        let bad = |s: &str| CullingError::Invalid(s.into());
        if self.schema_version != CULLING_SCHEMA_VERSION {
            return Err(CullingError::UnsupportedVersion(self.schema_version));
        }
        if self.asset_id.is_empty()
            || self.assessment_id.is_empty()
            || self.asset_id.len() > 128
            || self.assessment_id.len() > 128
            || chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err()
        {
            return Err(bad("Invalid envelope"));
        }
        for s in [&self.source_fingerprint, &self.cache_key] {
            if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(bad("Invalid fingerprint/cache identity"));
            }
        }
        if self.reasons.is_empty() || self.reasons.len() > 512 {
            return Err(bad("A rating needs bounded structured reasons"));
        }
        if let Some(f) = &self.features {
            if f.asset_id != self.asset_id
                || f.photo_type != self.photo_type
                || Some(&f.source_analysis_id) != self.source_analysis_id.as_ref()
                || f.source_fingerprint != self.source_fingerprint
                || f.models != self.model_versions
            {
                return Err(bad("Feature binding mismatch"));
            }
            f.validate()?;
        }
        let redundant = self
            .similarity
            .exact
            .as_ref()
            .is_some_and(|e| e.canonical_asset_id != self.asset_id);
        if self.ai_rating.is_some()
            && self.features.is_none()
            && !(redundant
                && self.ai_rating == Stars::new(1).ok()
                && self
                    .reasons
                    .iter()
                    .any(|r| r.code == ReasonCode::ExactDuplicate))
        {
            return Err(bad(
                "Unavailable source cannot be rated as a defective photograph",
            ));
        }
        self.similarity.validate()?;
        if let Some(content) = &self.duplicate_content {
            content.validate()?;
        }
        if self
            .similarity
            .exact
            .as_ref()
            .is_some_and(|e| Some(&e.content) != self.duplicate_content.as_ref())
        {
            return Err(bad("Exact relationship does not match content identity"));
        }
        for key in [&self.duplicate_stamp, &self.membership_key]
            .into_iter()
            .flatten()
        {
            if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(bad("Invalid duplicate generation identity"));
            }
        }
        if self.similarity.group_id.is_some()
            && self.similarity.preferred
                != self.similarity.preferred_assets.contains(&self.asset_id)
        {
            return Err(bad("Preferred membership mismatch"));
        }
        for r in &self.reasons {
            if r.subject_index.is_some_and(|i| {
                self.features
                    .as_ref()
                    .and_then(|f| f.people.faces.value())
                    .is_none_or(|faces| !faces.iter().any(|f| f.index == i))
            }) {
                return Err(bad("Reason refers to an unknown subject"));
            }
        }
        if !self.safe() {
            return Err(bad("Non-finite or oversized field"));
        }
        check_serialized(self)?;
        Ok(())
    }
    pub fn canonical_json(&self) -> Result<String, CullingError> {
        self.validate()?;
        serde_json::to_string(
            &serde_json::to_value(self).map_err(|e| CullingError::Invalid(e.to_string()))?,
        )
        .map_err(|e| CullingError::Invalid(e.to_string()))
    }
    pub fn parse(json: &str) -> Result<Self, CullingError> {
        if json.len() > MAX_CULLING_BYTES {
            return Err(CullingError::Invalid("Payload too large".into()));
        }
        let mut v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| CullingError::Invalid(e.to_string()))?;
        let version = v
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| CullingError::Invalid("Missing version".into()))?;
        if version == 1 {
            // Keep immutable v1 evidence on disk; upgrade only the read representation.
            // Legacy visual groups are not proof of exact or near duplication.
            v["schema_version"] = CULLING_SCHEMA_VERSION.into();
            for field in ["duplicate_content", "duplicate_stamp", "membership_key"] {
                v[field] = serde_json::Value::Null;
            }
            let s = v
                .get_mut("similarity")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or_else(|| {
                    CullingError::Invalid("Missing or malformed legacy similarity context".into())
                })?;
            let grouped = s.get("group_id").is_some_and(serde_json::Value::is_string);
            let similarity_score = if grouped {
                s.get("confidence")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            };
            s.insert(
                "kind".into(),
                if grouped { "similar" } else { "unique" }.into(),
            );
            s.insert("similarity_score".into(), similarity_score);
            s.insert("exact".into(), serde_json::Value::Null);
        } else if version != CULLING_SCHEMA_VERSION as u64 {
            return Err(CullingError::UnsupportedVersion(
                version.min(u32::MAX as u64) as u32,
            ));
        }
        let a: Self =
            serde_json::from_value(v).map_err(|e| CullingError::Invalid(e.to_string()))?;
        a.validate()?;
        Ok(a)
    }
}
impl CullingFeatures {
    pub fn validate(&self) -> Result<(), CullingError> {
        let bad = |s: &str| CullingError::Invalid(s.into());
        if self.asset_id.is_empty()
            || self.source_analysis_id.is_empty()
            || self.feature_version.is_empty()
            || self.source_analysis_version == 0
            || self.source_fingerprint.len() != 64
            || !self
                .source_fingerprint
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || self.descriptor.aspect_ratio <= 0.
            || self.composition.aspect_ratio <= 0.
        {
            return Err(bad("Invalid feature identity or aspect ratio"));
        }
        if self.descriptor.difference_hash.len() != 16
            || !self
                .descriptor
                .difference_hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || self.descriptor.luminance_grid.len() != 64
            || self.descriptor.color_grid.len() != 48
        {
            return Err(bad("Invalid descriptor"));
        }
        if let Some(faces) = self.people.faces.value() {
            if faces.len() > 64 {
                return Err(bad("Too many detected faces"));
            }
            let mut ids = std::collections::HashSet::new();
            for f in faces {
                if !ids.insert(f.index) {
                    return Err(bad("Duplicate face index"));
                }
                let b = &f.bbox;
                if b.width <= 0.
                    || b.height <= 0.
                    || b.x < 0.
                    || b.y < 0.
                    || b.x + b.width > 1.000001
                    || b.y + b.height > 1.000001
                {
                    return Err(bad("Invalid normalized face box"));
                }
            }
        }
        if !self.safe() {
            return Err(bad("Non-finite or oversized field"));
        }
        check_serialized(self)
    }
}
fn check_serialized<T: Serialize + for<'a> Deserialize<'a>>(item: &T) -> Result<(), CullingError> {
    let json = serde_json::to_string(item).map_err(|e| CullingError::Invalid(e.to_string()))?;
    // Non-finite required floats become null; roundtrip rejects them. Optional numeric leaves
    // are separately checked by the value walker where they carry confidence/reference values.
    let _: T = serde_json::from_str(&json)
        .map_err(|_| CullingError::Invalid("Non-finite or invalid numeric value".into()))?;
    if json.len() > MAX_CULLING_BYTES {
        return Err(CullingError::Invalid("Payload too large".into()));
    }
    let value = serde_json::to_value(item).map_err(|e| CullingError::Invalid(e.to_string()))?;
    walk(&value, "")
}
trait SafeLeaves {
    fn safe(&self) -> bool;
}
impl SafeLeaves for f64 {
    fn safe(&self) -> bool {
        self.is_finite()
    }
}
impl SafeLeaves for String {
    fn safe(&self) -> bool {
        self.len() <= 4096
    }
}
macro_rules! safe_primitive{($($t:ty),*)=>{$(impl SafeLeaves for $t{fn safe(&self)->bool{true}})*}}
safe_primitive!(
    u32,
    u64,
    bool,
    Stars,
    PhotoType,
    EyeState,
    ReasonCode,
    Severity,
    DuplicateKind
);
impl<T: SafeLeaves> SafeLeaves for Option<T> {
    fn safe(&self) -> bool {
        self.as_ref().is_none_or(SafeLeaves::safe)
    }
}
impl<T: SafeLeaves> SafeLeaves for Vec<T> {
    fn safe(&self) -> bool {
        self.len() <= 512 && self.iter().all(SafeLeaves::safe)
    }
}
impl<T: SafeLeaves> SafeLeaves for Signal<T> {
    fn safe(&self) -> bool {
        match self {
            Self::Available { value, confidence } => {
                value.safe() && confidence.is_finite() && (0. ..=1.).contains(confidence)
            }
            Self::Unavailable { reason }
            | Self::NotApplicable { reason }
            | Self::Failed { reason }
            | Self::Uncertain { reason } => !reason.is_empty() && reason.safe(),
        }
    }
}
impl SafeLeaves for BoundingBox {
    fn safe(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
    }
}
impl SafeLeaves for ProviderIdentity {
    fn safe(&self) -> bool {
        self.provider.safe() && self.model.safe() && self.version.safe()
    }
}
impl SafeLeaves for ReasonMeasurement {
    fn safe(&self) -> bool {
        self.value.safe() && self.unit.safe() && self.reference.safe()
    }
}
impl SafeLeaves for CullingReason {
    fn safe(&self) -> bool {
        self.confidence.safe() && self.measurement.safe()
    }
}
fn walk(v: &serde_json::Value, key: &str) -> Result<(), CullingError> {
    let bad = || CullingError::Invalid(format!("Invalid {key}"));
    match v {
        serde_json::Value::Number(n) => {
            let n = n.as_f64().ok_or_else(bad)?;
            if !n.is_finite() {
                return Err(bad());
            }
            if (key.contains("confidence")
                || key.ends_with("fraction")
                || matches!(
                    key,
                    "median_luminance"
                        | "mean_luminance"
                        | "tonal_range"
                        | "edge_distance"
                        | "luminance_grid"
                        | "color_grid"
                ))
                && !(0. ..=1.).contains(&n)
            {
                return Err(bad());
            }
            if matches!(key, "absolute_score" | "final_score") && !(0. ..=100.).contains(&n) {
                return Err(bad());
            }
        }
        serde_json::Value::String(s) => {
            if s.len() > 4096 {
                return Err(bad());
            }
        }
        serde_json::Value::Array(a) => {
            if a.len() > 512 {
                return Err(bad());
            }
            for v in a {
                walk(v, key)?;
            }
        }
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                walk(v, k)?;
            }
        }
        _ => (),
    }
    Ok(())
}
