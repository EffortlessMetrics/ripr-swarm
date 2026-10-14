//! Source identity for values captured within one indexed test.
//!
//! Derived values without an originating-test witness remain unsourced.
//! This leaf assigns metadata only and makes no admission decision.

use crate::domain::ValueFact;
use std::path::Path;

pub(super) fn attach_test_source(facts: &mut [ValueFact], source_file: &Path) {
    if source_file.as_os_str().is_empty() {
        return;
    }
    let source_file = crate::analysis::stable_source_path_text(source_file);
    for fact in facts {
        fact.source_file = Some(source_file.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ValueContext;

    fn fact() -> ValueFact {
        ValueFact {
            source_file: None,
            line: 12,
            text: "assert_eq!(summary[\"status\"], \"not_collected\");".to_owned(),
            value: "\"not_collected\"".to_owned(),
            context: ValueContext::AssertionArgument,
        }
    }

    #[test]
    fn an_empty_origin_remains_unknown() {
        let mut facts = [fact()];
        attach_test_source(&mut facts, Path::new(""));
        assert!(facts[0].source_file.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn literal_backslash_directory_and_percent_spelling_keep_distinct_origins() {
        let mut facts = Vec::new();
        for (native, encoded) in [
            ("src/odd\\receipt.rs", "src/odd%5Creceipt.rs"),
            ("src/odd/receipt.rs", "src/odd/receipt.rs"),
            ("src/odd%5Creceipt.rs", "src/odd%255Creceipt.rs"),
        ] {
            let mut sourced = [fact()];
            attach_test_source(&mut sourced, Path::new(native));
            assert_eq!(sourced[0].source_file.as_deref(), Some(encoded));
            assert_eq!(
                crate::analysis::decode_stable_path_text(encoded),
                Path::new(native)
            );
            facts.extend([sourced[0].clone(), sourced[0].clone()]);
        }
        super::super::sort_value_facts(&mut facts);
        assert_eq!(
            facts.len(),
            3,
            "retain file identity while deduplicating same-file facts"
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_separators_keep_portable_source_identity() {
        let mut facts = [fact()];
        attach_test_source(&mut facts, Path::new(r"tests\receipt.rs"));
        assert_eq!(facts[0].source_file.as_deref(), Some("tests/receipt.rs"));
    }

    #[cfg(unix)]
    #[test]
    fn native_bytes_and_reserved_percent_keep_distinct_serializable_origins() -> Result<(), String>
    {
        use std::os::unix::ffi::OsStrExt;
        let mut native = [fact()];
        let mut spelled = [fact()];
        attach_test_source(
            &mut native,
            Path::new(std::ffi::OsStr::from_bytes(b"tests/receipt_\xff.rs")),
        );
        attach_test_source(&mut spelled, Path::new("tests/receipt_%FF.rs"));
        assert_eq!(
            native[0].source_file.as_deref(),
            Some("tests/receipt_%FF.rs")
        );
        assert_eq!(
            spelled[0].source_file.as_deref(),
            Some("tests/receipt_%25FF.rs")
        );
        let native = serde_json::to_value(&native[0]).map_err(|error| error.to_string())?;
        let spelled = serde_json::to_value(&spelled[0]).map_err(|error| error.to_string())?;
        assert_ne!(native["source_file"], spelled["source_file"]);
        Ok(())
    }
}
