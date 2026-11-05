use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LedgerLintEntry {
    pub(crate) name: String,
    pub(crate) level: String,
    pub(crate) activate_when_msrv: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) blocked_by: Option<String>,
    pub(crate) block_line: usize,
    pub(crate) is_planned: bool,
}

// Unknown fields intentionally remain documentary extensions (#3990).
// Known fields still have to match their declared types.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct LintLedgerFile {
    active: BTreeMap<String, Vec<toml::Spanned<LintLedgerRow>>>,
    planned: Vec<toml::Spanned<LintLedgerRow>>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct LintLedgerRow {
    name: String,
    level: String,
    activate_when_msrv: Option<String>,
    reason: Option<String>,
    blocked_by: Option<String>,
}

/// Read active and planned lint entries in source order.
///
/// TOML decoding retains commented headers, alternate table spellings and
/// multiline strings. Invalid known fields cannot silently become absent.
pub(crate) fn parse_clippy_lints_ledger(text: &str) -> (Vec<LedgerLintEntry>, Vec<String>) {
    let parsed: LintLedgerFile = match toml::from_str(text) {
        Ok(parsed) => parsed,
        Err(err) => {
            return (Vec::new(), vec![format!("policy/clippy-lints.toml: {err}")]);
        }
    };
    let mut rows: Vec<_> = parsed
        .active
        .into_values()
        .flatten()
        .map(|row| (false, row))
        .chain(parsed.planned.into_iter().map(|row| (true, row)))
        .collect();
    rows.sort_by_key(|(_, row)| row.span().start);

    let mut entries = Vec::new();
    let mut violations = Vec::new();
    let mut seen = BTreeSet::new();
    for (is_planned, spanned) in rows {
        let block_line = text
            .bytes()
            .take(spanned.span().start)
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let row = spanned.into_inner();
        if row.name.trim().is_empty() {
            violations.push(format!(
                "policy/clippy-lints.toml:{block_line} entry missing required `name`"
            ));
            continue;
        }
        if !valid_lint_identity(&row.name) {
            violations.push(format!(
                "policy/clippy-lints.toml:{block_line} invalid lint identity `{}`; expected a bare Rust lint name or `clippy::<name>`.",
                row.name
            ));
            continue;
        }
        if row.level.trim().is_empty() {
            violations.push(format!(
                "policy/clippy-lints.toml:{block_line} entry `{}` missing required `level`",
                row.name
            ));
            continue;
        }
        if !matches!(row.level.as_str(), "allow" | "warn" | "deny" | "forbid") {
            violations.push(format!(
                "policy/clippy-lints.toml:{block_line} `{}` has unsupported level `{}`; expected `allow`, `warn`, `deny`, or `forbid`.",
                row.name, row.level
            ));
            continue;
        }
        if !seen.insert(row.name.clone()) {
            violations.push(format!(
                "policy/clippy-lints.toml:{block_line} duplicate lint identity `{}`; each Rust or Clippy lint must have one active or planned entry.",
                row.name
            ));
            continue;
        }
        entries.push(LedgerLintEntry {
            name: row.name,
            level: row.level,
            activate_when_msrv: row.activate_when_msrv,
            reason: row.reason,
            blocked_by: row.blocked_by,
            block_line,
            is_planned,
        });
    }
    (entries, violations)
}

fn valid_lint_identity(name: &str) -> bool {
    let bare = name.strip_prefix("clippy::").unwrap_or(name);
    let mut bytes = bare.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::parse_clippy_lints_ledger;
    use crate::{collect_clippy_debt_violations, collect_lint_policy_violations};

    const CARGO_MSRV: &str = r#"
[workspace.package]
rust-version = "1.95"
"#;

    const FUTURE_ROW: &str = r#"
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "2.0"
reason = "deferred review"
blocked_by = "per-call receipts"
"#;

    const DEBT: &str = r#"
[[debt]]
id = "clippy-debt-test"
lint = "clippy::indexing_slicing"
level = "deny"
owner = "core/rust"
reason = "fixture for ledger collision"
blocked_by = "fixture review"
target = "2027-03-31"
"#;

    #[test]
    fn commented_planned_header_keeps_overdue_msrv_obligation() {
        for header in ["[[planned]]", "[[planned]] # reviewed rationale follows"] {
            let ledger = format!(
                r#"{header}
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.95"
"#
            );
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations.iter().any(|row| {
                    row.contains("clippy::indexing_slicing")
                        && row.contains("already met by workspace rust-version")
                        && row.contains("blocked_by")
                }),
                "{header}: {violations:?}"
            );
        }
    }

    #[test]
    fn inline_planned_array_keeps_overdue_msrv_obligation() {
        let ledger = r#"
planned = [{ name = "clippy::indexing_slicing", level = "deny", activate_when_msrv = "1.95" }]
"#;
        let violations = collect_lint_policy_violations(CARGO_MSRV, ledger);
        assert!(
            violations.iter().any(|row| {
                row.contains("clippy::indexing_slicing")
                    && row.contains("already met by workspace rust-version")
            }),
            "{violations:?}"
        );
    }

    #[test]
    fn known_field_types_cannot_silently_become_absent() {
        for (valid, invalid) in [
            (r#"name = "clippy::indexing_slicing""#, "name = 42"),
            (r#"level = "deny""#, "level = true"),
            (r#"activate_when_msrv = "2.0""#, "activate_when_msrv = 1.95"),
            (r#"reason = "deferred review""#, "reason = false"),
            (r#"blocked_by = "per-call receipts""#, "blocked_by = []"),
        ] {
            assert_eq!(FUTURE_ROW.matches(valid).count(), 1);
            let ledger = format!("[[planned]]\n{}", FUTURE_ROW.replace(valid, invalid));
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations
                    .iter()
                    .any(|row| row.contains("policy/clippy-lints.toml:")),
                "{invalid}: {violations:?}"
            );
        }
    }

    #[test]
    fn invalid_msrv_strings_reach_the_existing_version_check() {
        for msrv in ["", "1", "1.95-beta", "not-a-version"] {
            let ledger = format!(
                r#"[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "{msrv}"
"#
            );
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations.iter().any(|row| {
                    row.contains("clippy::indexing_slicing")
                        && row.contains("unparsable")
                        && row.contains("activate_when_msrv")
                }),
                "{msrv}: {violations:?}"
            );
        }
    }

    #[test]
    fn malformed_and_duplicate_key_toml_fail_closed() {
        for ledger in [
            format!("[[planned]]\n{FUTURE_ROW}\ntrailing garbage"),
            format!("[[planned]]\n{FUTURE_ROW}\nactivate_when_msrv = \"3.0\""),
            "planned = \"not an array\"".to_string(),
            "[active.panic_family]\nname = \"clippy::panic\"\nlevel = \"deny\"".to_string(),
        ] {
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations
                    .iter()
                    .any(|row| row.contains("policy/clippy-lints.toml:")),
                "{ledger}: {violations:?}"
            );
        }
    }

    #[test]
    fn duplicate_identities_across_families_and_states_fail() {
        for headers in [
            ("[[planned]]", "[[planned]]"),
            ("[[active.first]]", "[[active.second]]"),
            ("[[active.first]]", "[[planned]]"),
        ] {
            let ledger = format!("{}\n{FUTURE_ROW}\n{}\n{FUTURE_ROW}", headers.0, headers.1);
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations.iter().any(|row| {
                    row.contains("duplicate lint identity")
                        && row.contains("clippy::indexing_slicing")
                }),
                "{headers:?}: {violations:?}"
            );
        }
    }

    #[test]
    fn documentary_extension_fields_remain_supported() {
        let ledger = r#"
schema_version = "0.1"
owner = "core/rust"

[policy]
unsafe = "forbid"
future_documentation = { enabled = true }

[[planned]] # future MSRV does not require a remaining blocker
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "2.0"
note = { text = "reviewed later", references = [3990, 13217] }
tracking_issue = 13217
"#;
        let violations = collect_lint_policy_violations(CARGO_MSRV, ledger);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn multiline_non_msrv_blocker_remains_valid() {
        let ledger = r#"
[[planned]] # the typed blocker is still required
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.95"
reason = "narrative"
blocked_by = """
per-call receipts
and reviewed bounds
"""
"#;
        let violations = collect_lint_policy_violations(CARGO_MSRV, ledger);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn source_order_and_distinct_namespaces_are_preserved() {
        let ledger = r#"
[[active.zeta]]
name = "clippy::panic"
level = "deny"

[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
reason = "future work"

[[active.alpha]]
name = "panic"
level = "deny"
"#;
        let (entries, violations) = parse_clippy_lints_ledger(ledger);
        assert!(violations.is_empty(), "{violations:?}");
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["clippy::panic", "clippy::indexing_slicing", "panic"]);
        assert_eq!(
            entries
                .iter()
                .find(|entry| entry.is_planned)
                .and_then(|entry| entry.reason.as_deref()),
            Some("future work")
        );
    }

    #[test]
    fn source_spans_report_entry_lines_for_each_toml_form() {
        let array_table = r#"# prologue
[[planned]] # reviewed header
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.95"
"#;
        let inline_array = r#"# prologue
planned = [
    { name = "clippy::indexing_slicing", level = "deny", activate_when_msrv = "1.95" }
]
"#;
        for (ledger, expected_line) in [(array_table, 2), (inline_array, 3)] {
            let (entries, parse_violations) = parse_clippy_lints_ledger(ledger);
            assert!(parse_violations.is_empty(), "{parse_violations:?}");
            let lines: Vec<_> = entries.iter().map(|entry| entry.block_line).collect();
            assert_eq!(lines, [expected_line]);
            let prefix = format!("policy/clippy-lints.toml:{expected_line} ");
            let violations = collect_lint_policy_violations(CARGO_MSRV, ledger);
            assert!(
                violations.iter().any(|row| {
                    row.starts_with(&prefix)
                        && row.contains("clippy::indexing_slicing")
                        && row.contains("already met by workspace rust-version")
                }),
                "{violations:?}"
            );
        }
    }

    #[test]
    fn invalid_identities_and_levels_do_not_create_planned_entries() {
        for name in [
            "clippy::",
            "clippy::nested::panic",
            "rust::unsafe_code",
            "rustdoc::broken_intra_doc_links",
            " clippy::panic",
            "not a lint",
        ] {
            let ledger = format!("[[planned]]\nname = \"{name}\"\nlevel = \"deny\"\n");
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations
                    .iter()
                    .any(|row| row.contains("invalid lint identity")),
                "{name}: {violations:?}"
            );
        }
        for level in ["warning", "force-warn", "expect", " deny "] {
            let ledger = format!("[[planned]]\nname = \"clippy::panic\"\nlevel = \"{level}\"\n");
            let violations = collect_lint_policy_violations(CARGO_MSRV, &ledger);
            assert!(
                violations
                    .iter()
                    .any(|row| row.contains("unsupported level")),
                "{level}: {violations:?}"
            );
        }
    }

    #[test]
    fn equal_bare_names_require_both_namespace_receipts() {
        // This checks ledger identity, independently of compiler lint availability.
        let cargo = r#"
[workspace.lints.clippy]
panic = "deny"
[workspace.lints.rust]
panic = "deny"
"#;
        let clippy_only = r#"
[[active.panic_family]]
name = "clippy::panic"
level = "deny"
"#;
        let violations = collect_lint_policy_violations(cargo, clippy_only);
        assert!(
            violations.iter().any(|row| {
                row.contains("[workspace.lints.rust]")
                    && row.contains("panic")
                    && row.contains("no matching")
            }),
            "{violations:?}"
        );
        let both = format!(
            r#"{clippy_only}
[[active.rust]]
name = "panic"
level = "deny"
"#
        );
        let violations = collect_lint_policy_violations(cargo, &both);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn debt_consumer_retains_lint_ledger_parse_errors() {
        let violations = collect_clippy_debt_violations(
            DEBT,
            "",
            "[[planned]]\nname = 42\nlevel = \"deny\"",
            "2026-11-05",
        );
        assert!(
            violations
                .iter()
                .any(|row| row.contains("policy/clippy-lints.toml:")),
            "{violations:?}"
        );
    }

    #[test]
    fn commented_planned_row_still_collides_with_debt() {
        let ledger = format!("[[planned]] # reviewed future lint\n{FUTURE_ROW}");
        let violations = collect_clippy_debt_violations(DEBT, "", &ledger, "2026-11-05");
        assert!(
            violations.iter().any(|row| {
                row.contains("clippy::indexing_slicing") && row.contains("already `[[planned]]`")
            }),
            "{violations:?}"
        );
    }

    #[test]
    fn repository_ledger_has_valid_unique_rows() {
        let (entries, violations) =
            parse_clippy_lints_ledger(include_str!("../../../policy/clippy-lints.toml"));
        assert!(violations.is_empty(), "{violations:?}");
        assert!(!entries.is_empty());
        assert!(entries.iter().any(|entry| entry.is_planned));
    }
}
