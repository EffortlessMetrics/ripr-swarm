//! Matched static/runtime controls for a table loop's skip guard (#9409,
//! RIPR-SPEC-0240). The changed `"ftp" => 22` arm reads `exposed` exactly
//! when the compiled test still asserts the ftp row, so the wrong value
//! fails at runtime; every row the guard cannot be shown to skip, or that
//! it skips, must agree with that execution or stay conservative.

use super::{Scratch, source_runtime_control};
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};

const PRODUCTION: &str = "pub fn default_port(scheme: &str) -> u16 {
    match scheme {
        \"http\" => 80,
        \"https\" => 443,
        \"ftp\" => 21,
        \"gopher\" => 70,
        _ => 0,
    }
}
";

/// `(case, test prelude, guard, exposed, ftp row asserted at runtime)`.
/// `exposed` may fall short of the runtime truth (a conservative miss) and
/// must never exceed it.
const CASES: &[(&str, &str, &str, bool, bool)] = &[
    // The guard names a row, but not the changed one.
    (
        "listed_other_row",
        "const SKIP: &[&str] = &[\"gopher\"];",
        "if SKIP.contains(&scheme) { continue; }",
        true,
        true,
    ),
    // The changed row is on the list: its assertion never runs.
    (
        "listed_changed_row",
        "const SKIP: &[&str] = &[\"ftp\"];",
        "if SKIP.contains(&scheme) { continue; }",
        false,
        false,
    ),
    (
        "listed_among_several",
        "const SKIP: &[&str] = &[\"gopher\", \"http\", \"ftp\"];",
        "if SKIP.contains(&scheme) { continue; }",
        false,
        false,
    ),
    (
        "equality_other_row",
        "",
        "if scheme == \"gopher\" { continue; }",
        true,
        true,
    ),
    (
        "equality_changed_row",
        "",
        "if scheme == \"ftp\" { continue; }",
        false,
        false,
    ),
    (
        "every_row_listed",
        "const SKIP: &[&str] = &[\"http\", \"https\", \"ftp\", \"gopher\"];",
        "if SKIP.contains(&scheme) { continue; }",
        false,
        false,
    ),
    // Unresolved lists stay conservative: the ftp row does run.
    (
        "local_list",
        "",
        "let skip = [\"gopher\"]; if skip.contains(&scheme) { continue; }",
        false,
        true,
    ),
    (
        "list_shadowed_in_test",
        "const SKIP: &[&str] = &[\"ftp\"];",
        "const SKIP: &[&str] = &[\"http\"]; if SKIP.contains(&scheme) { continue; }",
        false,
        true,
    ),
    (
        "computed_list",
        "const SKIP: &[&str] = &[concat!(\"ft\", \"p\")];",
        "if SKIP.contains(&scheme) { continue; }",
        false,
        false,
    ),
    // A second binding of the guard's name could be the one it reads.
    (
        "binding_rebound_later",
        "const SKIP: &[&str] = &[\"gopher\"];",
        "if SKIP.contains(&scheme) { continue; } let _len = |scheme: &str| scheme.len();",
        false,
        true,
    ),
    // Work the loop does before the guard is not a leading guard.
    (
        "guard_after_other_statement",
        "const SKIP: &[&str] = &[\"gopher\"];",
        "let _ = scheme; if SKIP.contains(&scheme) { continue; }",
        false,
        true,
    ),
    (
        "guard_with_else",
        "const SKIP: &[&str] = &[\"gopher\"];",
        "if SKIP.contains(&scheme) { continue; } else { }",
        false,
        true,
    ),
];

fn subject(prelude: &str, guard: &str, production: &str) -> String {
    format!(
        "{production}
#[cfg(test)]
mod tests {{
    use super::*;
    {prelude}

    #[test]
    fn default_ports_match_the_reference_table() {{
        let table = [(\"http\", 80), (\"https\", 443), (\"ftp\", 21), (\"gopher\", 70)];
        for (scheme, want) in table {{
            {guard}
            assert_eq!(default_port(scheme), want, \"{{scheme}}\");
        }}
    }}
}}
"
    )
}

#[test]
fn a_skip_guard_credits_only_the_rows_it_leaves() -> Result<(), String> {
    let changed = PRODUCTION.replace("\"ftp\" => 21,", "\"ftp\" => 22,");
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 0000000..1111111 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,3 +3,3 @@
         \"http\" => 80,
         \"https\" => 443,
-        \"ftp\" => 21,
+        \"ftp\" => 22,
         \"gopher\" => 70,
";
    for &(case, prelude, guard, exposed, asserted) in CASES {
        assert!(!exposed || asserted, "{case}: static outruns runtime");
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"ports\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        // The analyzed tree is the changed one.
        let source = subject(prelude, guard, &changed);
        std::fs::write(scratch.0.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("diff.patch"), diff).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let arms = json["findings"]
            .as_array()
            .ok_or("missing findings")?
            .iter()
            .filter(|finding| finding["probe"]["family"] == "match_arm")
            .collect::<Vec<_>>();
        assert_eq!(arms.len(), 1, "{case}");
        assert_eq!(
            arms[0]["classification"] == "exposed",
            exposed,
            "{case}: {}",
            arms[0]["classification"]
        );
        // The unchanged library passes; the changed one fails exactly when
        // the compiled test still asserts the ftp row.
        source_runtime_control(
            &subject(prelude, guard, PRODUCTION),
            &format!("skip guard {case} original"),
            1,
            false,
        )?;
        source_runtime_control(&source, &format!("skip guard {case} changed"), 1, asserted)?;
    }
    Ok(())
}
