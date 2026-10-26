//! An owner whose changed `Ok(..)`/`Some(..)` tail sits beside `Err`/`None`
//! exits is pinned by `owner(..).unwrap()` (or `.expect("..")`) compared with
//! the bare literal, directly or through a once-bound local (#9619): a passing
//! comparison can only be the wrapped literal, since any other exit panics.
//! Anything that lets the value move off the owner, or reads another
//! variant, keeps the finding below `exposed`.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

const RESULT_OWNER: &str = "pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n    if b == 0 {\n        return Err(());\n    }\n    Ok(a / b)\n}\npub fn other_div(a: u32, b: u32) -> Result<u32, ()> {\n    Ok(a / b.max(1))\n}\n";
const RESULT_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n     if b == 0 {\n         return Err(());\n     }\n-    Ok(a / b + 1)\n+    Ok(a / b)\n }\n";
const OPTION_OWNER: &str = "pub fn half(n: u32) -> Option<u32> {\n    if n % 2 == 1 {\n        return None;\n    }\n    Some(n / 2)\n}\n";
const OPTION_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn half(n: u32) -> Option<u32> {\n     if n % 2 == 1 {\n         return None;\n     }\n-    Some(n / 2 + 1)\n+    Some(n / 2)\n }\n";

struct Case {
    name: &'static str,
    owner: &'static str,
    diff: &'static str,
    /// Extra owner-file items after the owner.
    owner_extra: &'static str,
    body: &'static str,
    credited: bool,
}

const fn result_case(name: &'static str, body: &'static str, credited: bool) -> Case {
    Case {
        name,
        owner: RESULT_OWNER,
        diff: RESULT_DIFF,
        owner_extra: "",
        body,
        credited,
    }
}

#[test]
fn unwrapped_ok_pins_credit_only_when_the_value_stays_the_owners() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("unwrapped_ok_owner_pin");
    let cases = [
        result_case(
            "wrapped_literal_baseline",
            "assert_eq!(checked_div(17, 5), Ok(3));",
            true,
        ),
        result_case(
            "direct_unwrap",
            "assert_eq!(checked_div(17, 5).unwrap(), 3);",
            true,
        ),
        result_case(
            "direct_unwrap_reversed",
            "assert_eq!(3, checked_div(17, 5).unwrap());",
            true,
        ),
        result_case(
            "direct_expect",
            "assert_eq!(checked_div(17, 5).expect(\"divides\"), 3);",
            true,
        ),
        result_case(
            "bound_unwrap",
            "let q = checked_div(17, 5).unwrap();\nassert_eq!(q, 3);",
            true,
        ),
        result_case(
            "bound_expect",
            "let q = checked_div(17, 5).expect(\"divides\");\nassert_eq!(q, 3);",
            true,
        ),
        Case {
            name: "option_bound_unwrap",
            owner: OPTION_OWNER,
            diff: OPTION_DIFF,
            owner_extra: "",
            body: "let h = half(8).unwrap();\nassert_eq!(h, 4);",
            credited: true,
        },
        Case {
            name: "option_direct_unwrap",
            owner: OPTION_OWNER,
            diff: OPTION_DIFF,
            owner_extra: "",
            body: "assert_eq!(half(8).unwrap(), 4);",
            credited: true,
        },
        result_case(
            "mut_binding",
            "let mut q = checked_div(17, 5).unwrap();\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "second_binding_shadows",
            "let q = checked_div(17, 5).unwrap();\nassert_eq!(q, 3);\nlet q = 3;\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "binding_used_by_a_method",
            "let q = checked_div(17, 5).unwrap();\nassert_eq!(q.to_string(), \"3\");\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "expected_is_the_owner_again",
            "let q = checked_div(17, 5).unwrap();\nassert_eq!(q, checked_div(17, 5).unwrap());",
            false,
        ),
        result_case(
            "unwrap_or_default_value",
            "let q = checked_div(17, 5).unwrap_or(0);\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "unwrap_err_reads_the_other_variant",
            "assert_eq!(checked_div(17, 0).unwrap_err(), ());",
            false,
        ),
        result_case(
            "inequality_is_not_a_pin",
            "let q = checked_div(17, 5).unwrap();\nassert_ne!(q, 4);",
            false,
        ),
        result_case(
            "a_different_owner_is_unwrapped",
            "let q = other_div(17, 5).unwrap();\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "unwrap_discards_the_value",
            "checked_div(17, 5).unwrap();\nassert_eq!(3, 3);",
            false,
        ),
        Case {
            name: "owner_file_binds_ok_as_a_value",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "#[allow(non_upper_case_globals)]\nconst Ok: u32 = 0;\n",
            body: "assert_eq!(checked_div(17, 5).unwrap(), 3);",
            credited: false,
        },
    ];
    for case in cases {
        let source = format!(
            "{}{}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn pins_the_owner() {{\n{}\n}}\n}}\n",
            case.owner, case.owner_extra, case.body
        );
        let root = root_dir.join(case.name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"unwrapped_ok_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), &source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("diff.patch"), case.diff).map_err(|e| e.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join("diff.patch")),
            mode: Mode::Fast,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let finding = report
            .findings
            .iter()
            .find(|f| f.probe.family.as_str() == "return_value")
            .ok_or_else(|| format!("{}: no return_value finding", case.name))?;
        assert_eq!(
            finding.class.as_str() == "exposed",
            case.credited,
            "{}: {}",
            case.name,
            finding.class.as_str()
        );
    }
    Ok(())
}
