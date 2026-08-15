use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

use super::FindingSourceResolution;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    /// Producer-owned source identity for diff findings. Ordinary/non-diff
    /// locations retain the omitted unresolved default.
    #[serde(
        default,
        skip_serializing_if = "FindingSourceResolution::is_empty_unresolved"
    )]
    pub source_resolution: FindingSourceResolution,
}

impl SourceLocation {
    pub fn new(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            source_resolution: FindingSourceResolution::default(),
        }
    }

    pub fn with_source_resolution(mut self, source_resolution: FindingSourceResolution) -> Self {
        self.source_resolution = source_resolution;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProbeId(pub String);

impl fmt::Display for ProbeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub String);

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_location_new_accepts_path_like_inputs() {
        let location = SourceLocation::new("src/lib.rs", 12, 4);

        assert_eq!(location.file, PathBuf::from("src/lib.rs"));
        assert_eq!(location.line, 12);
        assert_eq!(location.column, 4);
        assert!(location.source_resolution.is_empty_unresolved());
    }

    #[test]
    fn probe_and_symbol_ids_display_inner_values() {
        assert_eq!(
            ProbeId("probe:src_lib:1:predicate".to_string()).to_string(),
            "probe:src_lib:1:predicate"
        );
        assert_eq!(
            SymbolId("symbol:checkout".to_string()).to_string(),
            "symbol:checkout"
        );
    }
}
