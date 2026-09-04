use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCategory {
    Metadata,
    Preview,
    Unreadable,
    Access,
    Traversal,
}

impl WarningCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Preview => "preview",
            Self::Unreadable => "unreadable",
            Self::Access => "access",
            Self::Traversal => "traversal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionWarning {
    pub category: WarningCategory,
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl IngestionWarning {
    pub fn new(
        category: WarningCategory,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            category,
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }
    pub fn at(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarningSummary {
    pub metadata: u64,
    pub preview: u64,
    pub unreadable: u64,
    pub access: u64,
    pub traversal: u64,
}

impl WarningSummary {
    pub fn total(&self) -> u64 {
        self.metadata + self.preview + self.unreadable + self.access + self.traversal
    }
}
