//! Small identity and location types shared across the domain: source
//! locations, probe ids and symbol ids.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// A position in a source file: the file path and a 1-based line and column.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    /// Path of the source file, as the producer reported it.
    pub file: PathBuf,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
}

impl SourceLocation {
    /// Builds a location from a path and a 1-based line and column.
    pub fn new(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

/// Identifier of one probe within a run. It displays as the inner string.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProbeId(pub String);

impl fmt::Display for ProbeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Name of the code symbol, such as a function, that owns a probe. It
/// displays as the inner string.
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
