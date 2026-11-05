//! Pure help-audit controls; the guide targets separately exercise the real CLI.

#[path = "../../crates/ripr/tests/common/help_contract.rs"]
pub mod help_contract;

use std::collections::BTreeMap;
use std::process::Output;

use help_contract::undocumented_doc_flags;

#[test]
fn help_flag_uses_successful_lookup_without_hiding_other_flags() -> Result<(), String> {
    // Some successful help screens list their options but not --help itself.
    // Seed the same successful-lookup cache consumed by the live guide audits.
    let mut cache = BTreeMap::from([(
        vec!["agent".to_string(), "receipt".to_string()],
        Some("Usage: ripr agent receipt\nOptions:\n  --root PATH\n".to_string()),
    )]);
    let no_lookup = |_: &[&str]| -> Result<Output, String> {
        Err("cached help unexpectedly reran the command".to_string())
    };
    for doc in [
        "`ripr agent receipt --help`",
        "```text\nripr agent receipt --root . --help\n```",
    ] {
        let drift = undocumented_doc_flags(&[("fixture.md", doc)], &mut cache, &no_lookup)?;
        if !drift.is_empty() {
            return Err(format!("help route rejected: {drift:?}"));
        }
    }
    for wrong in ["--helpful", "--agent-doc-invalid", "--root-extra"] {
        let doc = format!("`ripr agent receipt --help {wrong}`");
        let drift = undocumented_doc_flags(&[("fixture.md", &doc)], &mut cache, &no_lookup)?;
        let expected = format!(
            "fixture.md:1 `ripr agent receipt {wrong}` is not an option of `ripr agent receipt --help`"
        );
        if drift != vec![expected] {
            return Err(format!("help hid or misreported `{wrong}`: {drift:?}"));
        }
    }
    Ok(())
}

#[test]
fn help_only_invocation_still_propagates_help_capture_failure() -> Result<(), String> {
    let mut cache = BTreeMap::new();
    let failed_lookup = |args: &[&str]| -> Result<Output, String> {
        if args != ["agent", "receipt", "--help"] {
            return Err(format!("unexpected help route: {args:?}"));
        }
        Err("help capture failed".to_string())
    };
    let result = undocumented_doc_flags(
        &[("fixture.md", "`ripr agent receipt --help`")],
        &mut cache,
        &failed_lookup,
    );
    if result != Err("help capture failed".to_string()) {
        return Err(format!("help bypassed capture failure: {result:?}"));
    }
    Ok(())
}
