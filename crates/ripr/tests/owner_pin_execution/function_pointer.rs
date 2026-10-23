//! Matched public-API and runtime controls for the #9478 function-pointer leaf.

use super::{Scratch, source_runtime_control};
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn call_through_pointer(x: u32) -> u32 {
-    x * 3
+    3 * x
 }
";

const CASES: &[(&str, &str, bool)] = &[
    (
        "typed",
        "let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);",
        true,
    ),
    (
        "qualified",
        "let f: fn(u32) -> u32 = crate::call_through_pointer; assert_eq!(f(2), 6);",
        true,
    ),
    (
        "symmetric",
        "let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(6, f(2));",
        true,
    ),
    (
        "direct_beside_a_pointer_type",
        "let f: fn(u32) -> u32 = other; let _got = f(2); assert_eq!(call_through_pointer(2), 6);",
        true,
    ),
    (
        "qualified_rival",
        "let _got = call_through_pointer(0); let f: fn(u32) -> u32 = decoy::call_through_pointer; assert_eq!(f(2), 6);",
        false,
    ),
    (
        "blind",
        "let f: fn(u32) -> u32 = call_through_pointer; let _got = f(2); assert_eq!(6, 6);",
        false,
    ),
    (
        "reassigned",
        "let mut f: fn(u32) -> u32 = call_through_pointer; let _got = f(2); f = other; assert_eq!(f(2), 6);",
        false,
    ),
    (
        "rebound",
        "let f: fn(u32) -> u32 = call_through_pointer; let _got = f(2); let f: fn(u32) -> u32 = other; assert_eq!(f(2), 6);",
        false,
    ),
    (
        "shadowed_owner",
        "let call_through_pointer: fn(u32) -> u32 = other; let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);",
        false,
    ),
    (
        "computed_expected",
        "let f: fn(u32) -> u32 = call_through_pointer; let expected = f(2); assert_eq!(f(2), expected);",
        false,
    ),
    (
        "dead_assertion",
        "let f: fn(u32) -> u32 = call_through_pointer; if false { assert_eq!(f(2), 6); }",
        false,
    ),
    (
        "scope_escape",
        "if false { let f: fn(u32) -> u32 = call_through_pointer; let _ = f(0); } assert_eq!(f(2), 6);",
        false,
    ),
    (
        "raw_shadow",
        "let f: fn(u32) -> u32 = crate::call_through_pointer; { fn r#f(_: u32) -> u32 { 6 } assert_eq!(f(2), 6); }",
        false,
    ),
    (
        "glob_shadow",
        "let f: fn(u32) -> u32 = crate::call_through_pointer; { use crate::tests::decoy::*; assert_eq!(f(2), 6); }",
        false,
    ),
    (
        "cfg_binding",
        "#[cfg(any())] let f: fn(u32) -> u32 = crate::call_through_pointer; assert_eq!(f(2), 6);",
        false,
    ),
];

fn source(body: &str) -> String {
    format!(
        "pub fn call_through_pointer(x: u32) -> u32 {{
    3 * x
}}
#[cfg(test)]
mod tests {{
    use super::*;
    fn other(_: u32) -> u32 {{ 6 }}
    fn f(_: u32) -> u32 {{ 6 }}
    mod decoy {{ pub fn call_through_pointer(_: u32) -> u32 {{ 6 }} pub fn f(_: u32) -> u32 {{ 6 }} }}
    #[test]
    fn owner_through_a_fn_pointer() {{
        {body}
    }}
}}
"
    )
}

#[test]
fn function_pointer_pins_only_the_observed_owner_value() -> Result<(), String> {
    for &(case, body, credited) in CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"pointer-owner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), source(body))
            .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("change.diff"), DIFF).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed return value");
        assert_eq!(
            changed[0].class == ExposureClass::Exposed,
            credited,
            "{case}: {:?}",
            changed[0]
        );
        if case == "blind" {
            assert_eq!(changed[0].class, ExposureClass::WeaklyExposed);
        }
    }
    Ok(())
}

#[test]
fn function_pointer_runtime_controls_observe_the_same_value() -> Result<(), String> {
    for &(case, body, catches_mutant) in CASES {
        for (label, replacement, should_fail) in [
            ("neutral", "3 * x", false),
            ("wrong_value", "4 * x", catches_mutant),
        ] {
            source_runtime_control(
                &source(body).replacen("3 * x", replacement, 1),
                &format!("function_pointer_{case}_{label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

#[test]
fn function_pointer_standard_glob_keeps_observation_and_macro_refusal() -> Result<(), String> {
    // The inline facade mirrors serde_core: its own module name does not
    // shadow the standard crate in the nested glob's resolution scope.
    let facade = "mod lib { mod core { pub use core::*; } }";
    for (case, body, observes) in [
        (
            "standard_facade",
            "let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);",
            true,
        ),
        (
            "shadowed_assertion",
            "macro_rules! assert_eq { ($($ignored:tt)*) => {} } let f: fn(u32) -> u32 = call_through_pointer; assert_eq!(f(2), 6);",
            false,
        ),
    ] {
        let subject = format!("{}\n{facade}\n", source(body));
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"pointer-glob\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), &subject)
            .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("change.diff"), DIFF).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed return value");
        assert_eq!(
            changed[0].class == ExposureClass::Exposed,
            observes,
            "{case}"
        );
        for (label, replacement, should_fail) in [
            ("neutral", "3 * x", false),
            ("wrong_value", "4 * x", observes),
        ] {
            source_runtime_control(
                &subject.replacen("3 * x", replacement, 1),
                &format!("pointer_glob_{case}_{label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

const MASK_CASES: &[(&str, &str, bool)] = &[
    (
        "partial",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(f(4660), 52);",
        false,
    ),
    (
        "qualified_partial",
        "let f: fn(u32) -> u8 = crate::low_byte; assert_eq!(f(4660), 52);",
        false,
    ),
    (
        "reversed_partial",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(52, f(4660));",
        false,
    ),
    (
        "unread_other_alias",
        "let unused: fn(u32) -> u8 = low_byte; let _ = unused(0); let f: fn(u32) -> u8 = low_byte; assert_eq!(f(4660), 52);",
        false,
    ),
    (
        "later_unread_partial",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(f(4660), 52); let _ = f(0);",
        false,
    ),
    (
        "later_partial_assertion",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(f(4660), 52); assert_eq!(f(0), 0);",
        false,
    ),
    (
        "two_partial_aliases",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(f(1), 1); let g: fn(u32) -> u8 = low_byte; assert_eq!(g(16), 16);",
        false,
    ),
    (
        "nonliteral",
        "let word = 4660; let f: fn(u32) -> u8 = low_byte; assert_eq!(f(word), 52);",
        false,
    ),
    (
        "all_bits",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(f(255), 255);",
        true,
    ),
    (
        "qualified_all_bits",
        "let f: fn(u32) -> u8 = crate::low_byte; assert_eq!(f(255), 255);",
        true,
    ),
    (
        "reversed_all_bits",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(255, f(255));",
        true,
    ),
    (
        "later_unread_all_bits",
        "let f: fn(u32) -> u8 = low_byte; assert_eq!(f(255), 255); let _ = f(0);",
        true,
    ),
];

fn masked_source(body: &str) -> String {
    format!(
        "pub fn low_byte(n: u32) -> u8 {{\n    (n & 0xff) as u8\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pointer_mask() {{\n        {body}\n    }}\n}}\n"
    )
}

#[test]
fn function_pointer_mask_pins_require_an_individually_discriminating_input() -> Result<(), String> {
    for &(case, body, credited) in MASK_CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"pointer-mask\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), masked_source(body))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("change.diff"),
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn low_byte(n: u32) -> u8 {\n-    n as u8\n+    (n & 0xff) as u8\n }\n",
        )
        .map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed masked return");
        assert_eq!(
            changed[0].class == ExposureClass::Exposed,
            credited,
            "{case}: {:?}",
            changed[0]
        );
    }
    Ok(())
}

#[test]
fn function_pointer_mask_runtime_controls_match_the_selected_input() -> Result<(), String> {
    for &(case, body, catches_mutant) in MASK_CASES {
        for (label, expression, should_fail) in [
            ("neutral", "(n & 0xff) as u8", false),
            ("narrower", "(n & 0x7f) as u8", catches_mutant),
        ] {
            source_runtime_control(
                &masked_source(body).replacen("(n & 0xff) as u8", expression, 1),
                &format!("function_pointer_mask_{case}_{label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

const REBOUND_MASK_CASES: &[(&str, &str)] = &[
    ("constant_shadow", "let n = 52;"),
    ("partial_shadow", "let n = n & 0x34;"),
];

fn rebound_mask_source(statement: &str) -> String {
    format!(
        "pub fn low_byte(n: u32) -> u8 {{\n    {statement}\n    (n & 0xff) as u8\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pointer_mask() {{\n        let f: fn(u32) -> u8 = low_byte;\n        assert_eq!(f(255), 52);\n    }}\n}}\n"
    )
}

#[test]
fn function_pointer_rebound_mask_inputs_do_not_pin_the_changed_mask() -> Result<(), String> {
    for &(case, statement) in REBOUND_MASK_CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"pointer-mask-rebound\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), rebound_mask_source(statement))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("change.diff"),
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,4 +1,4 @@\n pub fn low_byte(n: u32) -> u8 {{\n     {statement}\n-    n as u8\n+    (n & 0xff) as u8\n }}\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 3
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed masked return");
        assert_eq!(changed[0].class, ExposureClass::WeaklyExposed, "{case}");
    }
    Ok(())
}

#[test]
fn function_pointer_rebound_mask_runtime_controls_miss_the_narrower_mask() -> Result<(), String> {
    for &(case, statement) in REBOUND_MASK_CASES {
        for (label, expression) in [
            ("neutral", "(n & 0xff) as u8"),
            ("narrower", "(n & 0x7f) as u8"),
        ] {
            source_runtime_control(
                &rebound_mask_source(statement).replacen("(n & 0xff) as u8", expression, 1),
                &format!("function_pointer_rebound_mask_{case}_{label}"),
                1,
                false,
            )?;
        }
    }
    Ok(())
}

const INFECTION_CASES: &[(&str, &str, bool)] = &[
    (
        "bare_blind",
        "let f: fn(u32) -> u32 = capped; assert_eq!(f(3), 3);",
        false,
    ),
    (
        "qualified_blind",
        "let f: fn(u32) -> u32 = crate::capped; assert_eq!(f(3), 3);",
        false,
    ),
    (
        "reverse_blind",
        "let f: fn(u32) -> u32 = capped; assert_eq!(3, f(3));",
        false,
    ),
    ("direct_blind", "assert_eq!(capped(3), 3);", false),
    (
        "bare_observed",
        "let f: fn(u32) -> u32 = capped; assert_eq!(f(9), 6);",
        true,
    ),
    (
        "qualified_observed",
        "let f: fn(u32) -> u32 = crate::capped; assert_eq!(f(9), 6);",
        true,
    ),
    (
        "reverse_observed",
        "let f: fn(u32) -> u32 = capped; assert_eq!(6, f(9));",
        true,
    ),
    ("direct_observed", "assert_eq!(capped(9), 6);", true),
];

fn infection_source(body: &str) -> String {
    format!(
        "pub fn capped(n: u32) -> u32 {{
    n.min(6)
}}
#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn pin() {{
        {body}
    }}
}}
"
    )
}

#[test]
fn function_pointer_invisible_inputs_withhold_exposure() -> Result<(), String> {
    for &(case, body, observes_change) in INFECTION_CASES {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"pointer-infection\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), infection_source(body))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("change.diff"),
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn capped(n: u32) -> u32 {\n-    n.min(5)\n+    n.min(6)\n }\n",
        )
        .map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(changed.len(), 1, "{case}: one changed return");
        assert_eq!(
            changed[0].class,
            if observes_change {
                ExposureClass::Exposed
            } else {
                ExposureClass::WeaklyExposed
            },
            "{case}: the asserted input must reach the changed value"
        );
    }
    Ok(())
}

#[test]
fn function_pointer_invisible_runtime_controls_match_the_input() -> Result<(), String> {
    for &(case, body, observes_change) in INFECTION_CASES {
        for (label, expression, should_fail) in [
            ("current", "n.min(6)", false),
            ("baseline", "n.min(5)", observes_change),
        ] {
            source_runtime_control(
                &infection_source(body).replacen("n.min(6)", expression, 1),
                &format!("function_pointer_invisible_{case}_{label}"),
                1,
                should_fail,
            )?;
        }
    }
    Ok(())
}

#[test]
fn function_pointer_shared_targets_require_binary_assertion_trust() -> Result<(), String> {
    let rustc = std::env::var_os("RUSTC")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("rustc"));
    for shadowed in [false, true] {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname=\"shared_pointer\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\ntest=false\ndoctest=false\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(scratch.0.join("src/lib.rs"), "mod shared;\n")
            .map_err(|error| error.to_string())?;
        let shadow = if shadowed {
            "macro_rules! assert_eq { ($($ignored:tt)*) => {} }\n"
        } else {
            ""
        };
        let main = scratch.0.join("src/main.rs");
        std::fs::write(&main, format!("{shadow}mod shared;\nfn main() {{}}\n"))
            .map_err(|error| error.to_string())?;
        let good = "pub fn call_through_pointer(x: u32) -> u32 {\n    3 * x\n}\n";
        let wrong = "pub fn call_through_pointer(x: u32) -> u32 {\n    4 * x\n}\n";
        let tail = "#[cfg(test)]\nmod tests {\nuse super::*;\n#[test]\nfn pin() {\nlet f: fn(u32) -> u32 = call_through_pointer;\nassert_eq!(f(2), 6);\n}\n}\n";
        let source = scratch.0.join("src/shared.rs");
        std::fs::write(&source, format!("{good}{tail}")).map_err(|error| error.to_string())?;
        let diff = scratch.0.join("change.diff");
        std::fs::write(
            &diff,
            "--- a/src/shared.rs\n+++ b/src/shared.rs\n@@ -1,3 +1,3 @@\n pub fn call_through_pointer(x: u32) -> u32 {\n-    4 * x\n+    3 * x\n }\n",
        )
        .map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(diff),
            mode: Mode::Fast,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let selected: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ProbeFamily::ReturnValue && finding.probe.location.line == 2
            })
            .collect();
        assert_eq!(selected.len(), 1, "one intended shared return subject");
        let finding = selected[0];
        assert_eq!(finding.ripr.reach.state, ripr::domain::StageState::Yes);
        assert_eq!(finding.related_tests.len(), 1);
        assert_eq!(
            finding.class,
            if shadowed {
                ExposureClass::ReachableUnrevealed
            } else {
                ExposureClass::Exposed
            },
            "shadowed={shadowed}: {finding:?}"
        );
        assert_eq!(
            finding.ripr.reveal.observe.state,
            if shadowed {
                ripr::domain::StageState::No
            } else {
                ripr::domain::StageState::Yes
            }
        );
        assert_eq!(
            finding.related_tests[0].oracle_strength,
            if shadowed {
                ripr::domain::OracleStrength::None
            } else {
                ripr::domain::OracleStrength::Strong
            }
        );
        if shadowed {
            assert!(
                finding
                    .ripr
                    .reveal
                    .observe
                    .summary
                    .contains("rust_assertion_context_unestablished")
            );
        }
        // Compile the binary root, whose macro binding differs from the lib
        // root. Compiling only the shared file would miss the refused context.
        for (label, production, expected_exit) in [
            ("correct", good, 0),
            ("wrong_value", wrong, if shadowed { 0 } else { 101 }),
        ] {
            std::fs::write(&source, format!("{production}{tail}"))
                .map_err(|error| error.to_string())?;
            let executable = scratch
                .0
                .join(format!("{label}{}", std::env::consts::EXE_SUFFIX));
            super::compiles(
                super::run(
                    &rustc,
                    &[
                        "--edition=2024".as_ref(),
                        "--test".as_ref(),
                        main.as_os_str(),
                        "-o".as_ref(),
                        executable.as_os_str(),
                    ],
                )?,
                label,
            )?;
            let result = super::run(&executable, &["--nocapture".as_ref()])?;
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert!(stdout.contains("running 1 test"));
            assert_eq!(
                result.status.code(),
                Some(expected_exit),
                "shadowed={shadowed}; {label}: {stdout}"
            );
        }
    }
    Ok(())
}

/// Alias reach to a constant reader must not become an owner-return pin.
/// These are static relation controls, not mutation-runtime evidence.
#[test]
fn function_pointer_constant_readers_relate_without_promoting() -> Result<(), String> {
    for (body, reads_constant) in [("x + LIMIT", true), ("x + 4", false)] {
        let scratch = Scratch::create()?;
        std::fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("Cargo.toml"),
            "[package]\nname = \"pointer-const-reader\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("src/lib.rs"),
            format!(
                "pub const LIMIT: u32 = 4;\npub fn score(x: u32) -> u32 {{ {body} }}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn pointer_const_case() {{\n        let f: fn(u32) -> u32 = score;\n        assert_eq!(f(2), 6);\n    }}\n}}\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            scratch.0.join("change.diff"),
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-pub const LIMIT: u32 = 3;\n+pub const LIMIT: u32 = 4;\n",
        )
        .map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(scratch.0.join("change.diff")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let changed: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.probe.location.line == 1)
            .collect();
        assert_eq!(
            changed.len(),
            1,
            "one changed constant: {body}: {changed:?}"
        );
        let finding = changed[0];
        assert_eq!(finding.probe.family, ProbeFamily::StaticUnknown, "{body}");
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{body}");
        assert_eq!(
            finding
                .ripr
                .reach
                .summary
                .contains("reads the changed constant"),
            reads_constant,
            "{body}: {finding:?}"
        );
        assert_eq!(
            finding
                .related_tests
                .iter()
                .any(|test| test.name == "pointer_const_case"),
            reads_constant,
            "{body}: {finding:?}"
        );
        if reads_constant {
            assert_eq!(finding.ripr.reach.state, ripr::domain::StageState::Weak);
            assert_eq!(
                finding.ripr.reveal.observe.state,
                ripr::domain::StageState::Unknown
            );
            assert_eq!(
                finding.ripr.reveal.discriminate.state,
                ripr::domain::StageState::Unknown
            );
        }
    }
    Ok(())
}
