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
