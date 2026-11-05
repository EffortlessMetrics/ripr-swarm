use std::collections::BTreeMap;

/// Read the Rust or Clippy lint levels from a Cargo manifest.
///
/// Cargo accepts a level string or a table containing a level. Other table
/// fields, such as priority and check-cfg, remain Cargo's validation responsibility.
pub(crate) fn parse_workspace_lints_section(
    text: &str,
    section: &str,
) -> Result<BTreeMap<String, String>, String> {
    let document: toml::Table =
        toml::from_str(text).map_err(|err| format!("Cargo.toml is not valid TOML: {err}"))?;
    let mut table = &document;
    let mut path = String::new();
    for key in ["workspace", "lints", section] {
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(key);
        match table.get(key) {
            Some(toml::Value::Table(nested)) => table = nested,
            None => return Ok(BTreeMap::new()),
            Some(_) => return Err(format!("Cargo.toml `{path}` must be a table.")),
        }
    }

    let mut levels = BTreeMap::new();
    for (name, value) in table {
        let level = match value {
            toml::Value::String(level) => Some(level.as_str()),
            toml::Value::Table(config) => config.get("level").and_then(toml::Value::as_str),
            _ => None,
        }
        .ok_or_else(|| {
            format!(
                "Cargo.toml [workspace.lints.{section}] `{name}`: expected a level string or a table with a string `level`."
            )
        })?;
        if !matches!(level, "allow" | "warn" | "deny" | "forbid") {
            return Err(format!(
                "Cargo.toml [workspace.lints.{section}] `{name}` has unsupported level `{level}`; expected `allow`, `warn`, `deny`, or `forbid`."
            ));
        }
        levels.insert(name.clone(), level.to_string());
    }
    Ok(levels)
}

#[cfg(test)]
mod tests {
    use crate::{collect_clippy_debt_violations, collect_lint_policy_violations};

    const RUST_TABLE: &str = r#"
[workspace.lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(kani)'] }
"#;

    const RUST_LEDGER: &str = r#"
[[active.rust]]
name = "unexpected_cfgs"
level = "warn"
"#;

    const CLIPPY_LEDGER: &str = r#"
[[active.panic_family]]
name = "clippy::panic"
level = "deny"
"#;

    const DEBT: &str = r#"
[[debt]]
id = "clippy-debt-test"
lint = "clippy::panic"
level = "deny"
owner = "core/rust"
reason = "fixture for configured-lint collision"
blocked_by = "fixture review"
target = "2027-03-31"
"#;

    #[test]
    fn table_form_lint_requires_a_ledger_entry() {
        let violations = collect_lint_policy_violations(RUST_TABLE, "");
        assert!(
            violations.iter().any(|row| {
                row.contains("unexpected_cfgs") && row.contains("no matching")
            }),
            "{violations:?}"
        );
    }

    #[test]
    fn table_form_lint_accepts_a_matching_ledger_with_cargo_metadata() {
        let violations = collect_lint_policy_violations(RUST_TABLE, RUST_LEDGER);
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn table_form_lint_reports_level_drift() {
        let cargo = r#"
[workspace.lints.clippy]
panic = { level = "warn", priority = -1 }
"#;
        let violations = collect_lint_policy_violations(cargo, CLIPPY_LEDGER);
        assert!(
            violations.iter().any(|row| {
                row.contains("clippy::panic")
                    && row.contains("at level `deny`")
                    && row.contains("at level `warn`")
            }),
            "{violations:?}"
        );
    }

    #[test]
    fn table_form_lint_reports_planned_activation() {
        let cargo = r#"
[workspace.lints.clippy]
indexing_slicing = { level = "deny" }
"#;
        let ledger = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
"#;
        let violations = collect_lint_policy_violations(cargo, ledger);
        assert!(
            violations.iter().any(|row| {
                row.contains("clippy::indexing_slicing") && row.contains("already activates it")
            }),
            "{violations:?}"
        );
    }

    #[test]
    fn table_form_lint_reports_debt_collision() {
        let cargo = r#"
[workspace.lints.clippy]
panic = { level = "deny" }
"#;
        let violations = collect_clippy_debt_violations(DEBT, cargo, "", "2026-11-05");
        assert!(
            violations.iter().any(|row| {
                row.contains("clippy::panic")
                    && row.contains("Cargo.toml")
                    && row.contains("already activates")
            }),
            "{violations:?}"
        );
    }

    #[test]
    fn cargo_lint_levels_accept_standard_toml_spellings() {
        for cargo in [
            r#"
["workspace"."lints"."clippy"] # table comment
'panic' = 'deny' # value comment
"#,
            r#"
[workspace.lints.clippy.panic]
level = "deny"
priority = -1
"#,
        ] {
            let violations = collect_lint_policy_violations(cargo, CLIPPY_LEDGER);
            assert!(violations.is_empty(), "{cargo}: {violations:?}");
        }
    }

    #[test]
    fn table_form_lint_accepts_all_cargo_levels() {
        for level in ["allow", "warn", "deny", "forbid"] {
            let cargo = format!(
                r#"
[workspace.lints.clippy]
panic = {{ level = "{level}", priority = -1 }}
"#
            );
            let ledger = format!(
                r#"
[[active.panic_family]]
name = "clippy::panic"
level = "{level}"
"#
            );
            let violations = collect_lint_policy_violations(&cargo, &ledger);
            assert!(violations.is_empty(), "{level}: {violations:?}");
        }
    }

    #[test]
    fn malformed_cargo_toml_is_a_policy_violation() {
        for cargo in [
            "[workspace.lints.clippy]\npanic = { level = \"deny\"\n",
            "[workspace.lints.clippy]\npanic = \"deny\"\npanic = \"warn\"\n",
            "[workspace.lints.clippy]\npanic = \"deny\"\ntrailing garbage\n",
        ] {
            let violations = collect_lint_policy_violations(cargo, "");
            assert!(
                violations
                    .iter()
                    .any(|row| row.contains("Cargo.toml is not valid TOML")),
                "{cargo}: {violations:?}"
            );
        }
    }

    #[test]
    fn malformed_cargo_toml_is_a_debt_policy_violation() {
        let violations = collect_clippy_debt_violations(
            DEBT,
            "[workspace.lints.clippy]\npanic = { level = \"deny\"\n",
            "",
            "2026-11-05",
        );
        assert!(
            violations
                .iter()
                .any(|row| row.contains("Cargo.toml is not valid TOML")),
            "{violations:?}"
        );
    }

    #[test]
    fn invalid_lint_level_forms_are_policy_violations() {
        for section in ["rust", "clippy"] {
            for value in [
                "true",
                "1",
                "[]",
                "{ priority = -1 }",
                "{ level = 1 }",
                r#"{ level = "warning" }"#,
                r#""warning""#,
                r#""force-warn""#,
                r#""expect""#,
                r#"" deny ""#,
            ] {
                let cargo = format!("[workspace.lints.{section}]\npanic = {value}\n");
                let violations = collect_lint_policy_violations(&cargo, "");
                assert!(
                    violations.iter().any(|row| {
                        row.contains("Cargo.toml")
                            && row.contains("panic")
                            && row.contains("expected")
                    }),
                    "{cargo}: {violations:?}"
                );
            }
        }
    }

    #[test]
    fn invalid_present_lint_sections_are_policy_violations() {
        for cargo in [
            "workspace = false",
            "[workspace]\nlints = false",
            "[workspace.lints]\nclippy = false",
            "[workspace.lints]\nrust = []",
        ] {
            let violations = collect_lint_policy_violations(cargo, "");
            assert!(
                violations.iter().any(|row| row.contains("must be a table")),
                "{cargo}: {violations:?}"
            );
        }
    }

    #[test]
    fn absent_lint_sections_remain_valid() {
        for cargo in ["", "[workspace.package]\nrust-version = \"1.95\""] {
            let violations = collect_lint_policy_violations(cargo, "");
            assert!(violations.is_empty(), "{cargo}: {violations:?}");
        }
    }

    #[test]
    fn repository_lint_ledger_matches_workspace_levels() {
        let violations = collect_lint_policy_violations(
            include_str!("../../../Cargo.toml"),
            include_str!("../../../policy/clippy-lints.toml"),
        );
        assert!(violations.is_empty(), "{violations:?}");
    }
}
