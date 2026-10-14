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
    for fact in facts {
        fact.source_file = Some(crate::analysis::stable_path_text(source_file).replace('\\', "/"));
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
