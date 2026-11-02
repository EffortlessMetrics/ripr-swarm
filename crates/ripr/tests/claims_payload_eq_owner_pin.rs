//! #12304: claims' `assert_ok_eq!(call, literal)` / `assert_some_eq!(call,
//! literal)` compare the call's `Ok`/`Some` payload with the literal and
//! panic otherwise, so on a verified `use claims::..` import they read as the
//! unwrapped `assert_eq!` pin (RIPR-SPEC-0197 "Claims payload equality
//! pins"). Extraction stays text-only: the line is `unknown` / none until
//! the owner pin admits it, and then reveal grades it `exact_value` / strong
//! (RIPR-SPEC-0231 rule 10). Every negative control is the credited twin with
//! one thing changed, and every refusal keeps the oracle `unknown`.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

const RESULT_OWNER: &str = "pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n    if b == 0 {\n        return Err(());\n    }\n    Ok(a / b)\n}\n";
const RESULT_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n     if b == 0 {\n         return Err(());\n     }\n-    Ok(a / b + 1)\n+    Ok(a / b)\n }\n";
const OPTION_OWNER: &str = "pub fn half(n: u32) -> Option<u32> {\n    if n % 2 == 1 {\n        return None;\n    }\n    Some(n / 2)\n}\n";
const OPTION_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn half(n: u32) -> Option<u32> {\n     if n % 2 == 1 {\n         return None;\n     }\n-    Some(n / 2 + 1)\n+    Some(n / 2)\n }\n";
const CLAIMS_DEP: &str = "claims = \"0.8\"";

struct Case {
    name: &'static str,
    owner: &'static str,
    diff: &'static str,
    /// Items between the owner and the test module.
    owner_extra: &'static str,
    /// Lines opening the test module (imports).
    imports: &'static str,
    body: &'static str,
    /// The dev-dependency line of the subject's manifest.
    dependency: &'static str,
    credited: bool,
}

const fn result_case(
    name: &'static str,
    imports: &'static str,
    body: &'static str,
    credited: bool,
) -> Case {
    Case {
        name,
        owner: RESULT_OWNER,
        diff: RESULT_DIFF,
        owner_extra: "",
        imports,
        body,
        dependency: CLAIMS_DEP,
        credited,
    }
}

const fn option_case(
    name: &'static str,
    imports: &'static str,
    body: &'static str,
    credited: bool,
) -> Case {
    Case {
        name,
        owner: OPTION_OWNER,
        diff: OPTION_DIFF,
        owner_extra: "",
        imports,
        body,
        dependency: CLAIMS_DEP,
        credited,
    }
}

#[test]
fn claims_payload_equality_credits_only_on_a_verified_claims_import() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("claims_payload_eq_owner_pin");
    let ok = "assert_ok_eq!(checked_div(17, 5), 3);";
    let import = "use claims::assert_ok_eq;";
    let cases = [
        result_case("assert_ok_eq_literal", import, ok, true),
        result_case(
            "grouped_import_and_a_message_argument",
            "use claims::{assert_ok, assert_ok_eq};",
            "assert_ok_eq!(checked_div(17, 5), 3, \"quotient\");",
            true,
        ),
        result_case(
            "a_unreachable_before_the_assertion_never_reaches_it",
            import,
            "unreachable!();\nassert_ok_eq!(checked_div(17, 5), 3);",
            false,
        ),
        result_case(
            "a_panic_before_the_assertion_never_reaches_it",
            import,
            "panic!(\"stop\");\nassert_ok_eq!(checked_div(17, 5), 3);",
            false,
        ),
        result_case(
            "a_todo_before_the_assertion_never_reaches_it",
            import,
            "todo!();\nassert_ok_eq!(checked_div(17, 5), 3);",
            false,
        ),
        result_case(
            "a_unimplemented_before_the_assertion_never_reaches_it",
            import,
            "unimplemented!();\nassert_ok_eq!(checked_div(17, 5), 3);",
            false,
        ),
        option_case(
            "assert_some_eq_literal",
            "use claims::assert_some_eq;",
            "assert_some_eq!(half(8), 4);",
            true,
        ),
        // One thing changed from the credited twin, below.
        result_case("no_import_at_all", "", ok, false),
        result_case(
            "another_crates_import",
            "use other::assert_ok_eq;",
            ok,
            false,
        ),
        result_case(
            "renamed_import",
            "use claims::assert_ok_eq as check_eq;",
            "check_eq!(checked_div(17, 5), 3);",
            false,
        ),
        result_case(
            "qualified_path_invocation_is_not_admitted",
            "use claims;",
            "claims::assert_ok_eq!(checked_div(17, 5), 3);",
            false,
        ),
        result_case(
            "a_local_macro_rebinds_the_name",
            "use claims::assert_ok_eq;\nmacro_rules! assert_ok_eq { ($e:expr, $v:expr) => {}; }",
            ok,
            false,
        ),
        result_case(
            "expected_is_a_binding",
            import,
            "let want = 3;\nassert_ok_eq!(checked_div(17, 5), want);",
            false,
        ),
        result_case(
            "expected_is_the_owner_again",
            import,
            "assert_ok_eq!(checked_div(17, 5), checked_div(17, 5).unwrap());",
            false,
        ),
        result_case(
            "expected_is_a_non_literal_expression",
            import,
            "assert_ok_eq!(checked_div(17, 5), 1 + 2);",
            false,
        ),
        result_case(
            "owner_call_is_the_expected_operand",
            import,
            "assert_ok_eq!(3, checked_div(17, 5));",
            false,
        ),
        result_case(
            "owner_call_is_wrapped",
            import,
            "assert_ok_eq!(checked_div(17, 5).map(|q| q + 1), 4);",
            false,
        ),
        // Wrong direction: the `Some` macro on a `Result` tail and the `Ok`
        // macro on an `Option` tail panic before any comparison.
        result_case(
            "assert_some_eq_on_an_ok_tail",
            "use claims::assert_some_eq;",
            "assert_some_eq!(checked_div(17, 5), 3);",
            false,
        ),
        option_case(
            "assert_ok_eq_on_a_some_tail",
            import,
            "assert_ok_eq!(half(8), 4);",
            false,
        ),
        Case {
            name: "claims_bound_to_a_path_package",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: import,
            body: ok,
            dependency: "claims = { path = \"../claims\" }",
            credited: false,
        },
        Case {
            name: "claims_renamed_to_another_package",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: import,
            body: ok,
            dependency: "claims = { package = \"other\", version = \"1\" }",
            credited: false,
        },
        Case {
            name: "claims_is_not_a_dependency",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: import,
            body: ok,
            dependency: "",
            credited: false,
        },
        Case {
            name: "owner_file_binds_err_as_a_value",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "#[allow(non_snake_case)]\nfn Err(_: ()) -> Result<u32, ()> {\n    Ok(3)\n}\n",
            imports: import,
            body: ok,
            dependency: CLAIMS_DEP,
            credited: false,
        },
        Case {
            name: "owner_file_binds_ok_as_a_value",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "#[allow(non_snake_case)]\nfn Ok(_: u32) -> u32 {\n    3\n}\n",
            imports: import,
            body: ok,
            dependency: CLAIMS_DEP,
            credited: false,
        },
    ];
    for case in cases {
        let outcome = run(&root_dir, &case, "return_value")?;
        assert_eq!(
            outcome.class == "exposed",
            case.credited,
            "{}: {}",
            case.name,
            outcome.class
        );
        // The reported oracle never disagrees with the verdict: the grade the
        // pin earns is the grade the finding shows, and a refusal reads
        // `unknown` / none, never a stronger oracle than the credit.
        let expected = if case.credited {
            ("exact_value".to_string(), "strong".to_string())
        } else {
            ("unknown".to_string(), "none".to_string())
        };
        assert_eq!(
            outcome.oracle, expected,
            "{}: related-test oracle",
            case.name
        );
        // RIPR-SPEC-0231 example 42: a refusal keeps the macro limitation, so
        // the unverified macro is disclosed rather than silently dropped.
        // A renamed macro is not a claims name at all, so it names no limitation.
        if !case.credited && case.name != "renamed_import" {
            assert_eq!(
                outcome.limit.as_deref(),
                Some("rust_macro_wrapped_assertion_unresolved"),
                "{}: limitation",
                case.name
            );
        }
    }
    Ok(())
}

/// The same verified-import twin under probes that are not `return_value`
/// (RIPR-SPEC-0197 rule 5): the macro is graded for the return-value family
/// only, so every other family keeps `unknown` / none and no credit.
#[test]
fn claims_payload_equality_grades_only_the_return_value_family() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("claims_payload_eq_families");
    let import = "use claims::assert_ok_eq;";
    let ok = "assert_ok_eq!(checked_div(17, 5), 3);";
    // The early `Err(..)` exit changes: both macros panic on that exit before
    // any comparison, so the opposite-direction pin must not credit (rule 4).
    let err_exit = Case {
        name: "early_err_exit_changed",
        owner: "pub fn checked_div(a: u32, b: u32) -> Result<u32, u32> {\n    if b == 0 {\n        return Err(2);\n    }\n    Ok(a / b)\n}\n",
        diff: "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn checked_div(a: u32, b: u32) -> Result<u32, u32> {\n     if b == 0 {\n-        return Err(1);\n+        return Err(2);\n     }\n     Ok(a / b)\n }\n",
        owner_extra: "",
        imports: import,
        body: ok,
        dependency: CLAIMS_DEP,
        credited: false,
    };
    let outcome = run(&root_dir, &err_exit, "error_path")?;
    assert_ne!(outcome.class, "exposed", "error_path: {}", outcome.class);
    assert_eq!(outcome.oracle.0, "unknown", "error_path oracle");
    // A `None` exit change beside `assert_some_eq!` (example 44).
    let none_exit = Case {
        name: "none_exit_changed",
        owner: "pub fn half(n: u32) -> Option<u32> {\n    if n % 2 == 1 {\n        return None;\n    }\n    Some(n / 2)\n}\n",
        diff: "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,6 +1,6 @@\n pub fn half(n: u32) -> Option<u32> {\n-    if n % 2 == 0 {\n+    if n % 2 == 1 {\n         return None;\n     }\n     Some(n / 2)\n }\n",
        owner_extra: "",
        imports: "use claims::assert_some_eq;",
        body: "assert_some_eq!(half(8), 4);",
        dependency: CLAIMS_DEP,
        credited: false,
    };
    let outcome = run(&root_dir, &none_exit, "predicate")?;
    assert_ne!(outcome.class, "exposed", "predicate: {}", outcome.class);
    assert_eq!(outcome.oracle.0, "unknown", "predicate oracle");
    // A multi-line invocation is not nominated (fail closed).
    let multi = Case {
        name: "multi_line_invocation",
        owner: RESULT_OWNER,
        diff: RESULT_DIFF,
        owner_extra: "",
        imports: import,
        body: "assert_ok_eq!(\n    checked_div(17, 5),\n    3\n);",
        dependency: CLAIMS_DEP,
        credited: false,
    };
    let outcome = run(&root_dir, &multi, "return_value")?;
    assert_ne!(outcome.class, "exposed", "multi-line: {}", outcome.class);
    assert_eq!(outcome.oracle.0, "unknown", "multi-line oracle");
    Ok(())
}

struct Outcome {
    class: String,
    oracle: (String, String),
    limit: Option<String>,
}

fn run(root_dir: &std::path::Path, case: &Case, family: &str) -> Result<Outcome, String> {
    let source = format!(
        "{}{}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n{}\n#[test]\nfn pins_the_owner() {{\n{}\n}}\n}}\n",
        case.owner, case.owner_extra, case.imports, case.body
    );
    let root = root_dir.join(case.name);
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"claims_payload_eq_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dev-dependencies]\n{}\n[workspace]\n",
            case.dependency
        ),
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
        .find(|f| f.probe.family.as_str() == family)
        .ok_or_else(|| format!("{}: no {family} finding", case.name))?;
    let test = finding
        .related_tests
        .first()
        .ok_or_else(|| format!("{}: no related test", case.name))?;
    Ok(Outcome {
        class: finding.class.as_str().to_string(),
        oracle: (
            test.oracle_kind.as_str().to_string(),
            test.oracle_strength.as_str().to_string(),
        ),
        limit: finding
            .static_limit_kind
            .as_ref()
            .map(|kind| kind.as_str().to_string()),
    })
}
