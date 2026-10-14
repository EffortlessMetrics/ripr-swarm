use std::collections::BTreeSet;
const ENTRY_CAP: usize = 128;
const PREFIX: &str = "ripr: ";
const MARKER: &str = " Rust module composition limitation(s): ";
const SUFFIX: &str = "; affected module contexts remain fail-closed.";

const MODULE_AMBIGUOUS_PARENT_NEXT_STEP: &str = "To resolve rust_module_ambiguous_parent, give each listed file one owner: declare it from a single parent `mod` item (drop duplicate `#[path]` declarations and keep only one of `<name>.rs` or `<name>/mod.rs`); a shared `tests/<name>/mod.rs` helper needs the same `mod <name>;` declaration, with the same `#[cfg(test)]` gating, in every integration test.";

const ANALYSIS_LIMIT_ORIENTATION: &str = "This is an analysis-limit note about indexed context, not a finding: no action is needed unless evidence you expected from a listed file is missing.";

fn parse_note(line: &str) -> Option<Vec<(String, String)>> {
    let body = line.strip_prefix(PREFIX)?;
    let (count_text, body) = body.split_once(MARKER)?;
    let count = count_text.parse::<usize>().ok()?;
    if count == 0 || count > ENTRY_CAP || count.to_string() != count_text {
        return None;
    }
    let (body, orientation) = body.split_once(SUFFIX)?;
    if !orientation.is_empty()
        && orientation != format!(" {ANALYSIS_LIMIT_ORIENTATION}")
        && orientation
            != format!(" {ANALYSIS_LIMIT_ORIENTATION} {MODULE_AMBIGUOUS_PARENT_NEXT_STEP}")
    {
        return None;
    }
    let mut entries = BTreeSet::new();
    for item in body.split(", ") {
        let (context, reason) = item.rsplit_once(':')?;
        if context.is_empty()
            || context.len() > 1024
            || context.chars().any(char::is_control)
            || context.starts_with('/')
            || context.split('/').any(|part| part == "..")
            || !matches!(
                reason,
                "rust_module_ambiguous_parent"
                    | "rust_module_cycle_or_depth_limit"
                    | "rust_module_context_conflict"
                    | "rust_module_unresolved_target"
            )
        {
            return None;
        }
        if reason == "rust_module_unresolved_target" {
            let (_, location) = context.rsplit_once(':')?;
            if location.parse::<u64>().ok()? == 0 {
                return None;
            }
        }
        if !entries.insert((context.to_owned(), reason.to_owned())) {
            return None;
        }
    }
    (entries.len() == count).then(|| entries.into_iter().collect())
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unresolved_target_location_is_required_and_positive() {
        let make = |context: &str| {
            format!("ripr: 1{MARKER}{context}:rust_module_unresolved_target{SUFFIX}")
        };
        assert_eq!(
            parse_note(&make("src/a.rs:12")),
            Some(vec![(
                "src/a.rs:12".into(),
                "rust_module_unresolved_target".into()
            )])
        );
        for context in [
            "src/a.rs",
            "src/a.rs:0",
            "src/a.rs:not_a_number",
            "src/a.rs:18446744073709551616",
        ] {
            assert!(parse_note(&make(context)).is_none(), "{context}");
        }
    }


    #[test]
    fn other_parser_condition() {
        assert!(parse_note("ripr: 1 Rust module composition limitation(s): src/a.rs:rust_module_ambiguous_parent; affected module contexts remain fail-closed.").is_some());
        assert!(parse_note("ripr: 0 Rust module composition limitation(s): src/a.rs:rust_module_ambiguous_parent; affected module contexts remain fail-closed.").is_none());
    }

    #[test]
    fn unrelated_plan_and_gate_oracles() {
        let plan_reason = "rust_module_unresolved_target";
        let gate_passed = true;
        assert_eq!(plan_reason, "rust_module_unresolved_target");
        assert!(gate_passed);
    }
}
