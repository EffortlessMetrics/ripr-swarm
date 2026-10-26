//! `let q = assert_ok!(owner(..)); assert_eq!(q, literal)` pins an `Ok`/`Some`
//! tail like `owner(..).unwrap()` does (#9619): claims' `assert_ok!` and
//! `assert_some!` evaluate their operand once, panic on the other variant and
//! yield the payload. The macro is trusted only through a manifest-verified
//! `use claims::..` import under its own name; every rebinding, other crate,
//! rename, derived value or wrapped operand keeps the finding below `exposed`.
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

#[test]
fn claims_payload_pins_credit_only_on_a_verified_claims_import() -> Result<(), String> {
    let root_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("claims_payload_owner_pin");
    let bound = "let q = assert_ok!(checked_div(17, 5));\nassert_eq!(q, 3);";
    let cases = [
        result_case(
            "single_import_bound_assert_ok",
            "use claims::assert_ok;",
            bound,
            true,
        ),
        result_case(
            "grouped_import_bound_assert_ok",
            "use claims::{assert_err, assert_ok};",
            bound,
            true,
        ),
        result_case(
            "a_later_assert_err_on_another_call_is_inert",
            "use claims::{assert_err, assert_ok};",
            "let q = assert_ok!(checked_div(17, 5));\nassert_eq!(q, 3);\nassert_err!(checked_div(1, 0));",
            true,
        ),
        Case {
            name: "bound_assert_some",
            owner: OPTION_OWNER,
            diff: OPTION_DIFF,
            owner_extra: "",
            imports: "use claims::assert_some;",
            body: "let h = assert_some!(half(8));\nassert_eq!(h, 4);",
            dependency: CLAIMS_DEP,
            credited: true,
        },
        result_case("no_import_at_all", "", bound, false),
        result_case(
            "another_crates_import",
            "use other::assert_ok;",
            bound,
            false,
        ),
        result_case(
            "renamed_import",
            "use claims::assert_ok as check_ok;",
            "let q = check_ok!(checked_div(17, 5));\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "a_local_macro_rebinds_the_name",
            "use claims::assert_ok;\nmacro_rules! assert_ok { ($e:expr) => { 3 }; }",
            bound,
            false,
        ),
        result_case(
            "mut_binding",
            "use claims::assert_ok;",
            "let mut q = assert_ok!(checked_div(17, 5));\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "expected_is_the_owner_again",
            "use claims::assert_ok;",
            "let q = assert_ok!(checked_div(17, 5));\nassert_eq!(q, checked_div(17, 5).unwrap());",
            false,
        ),
        result_case(
            "operand_is_wrapped",
            "use claims::assert_ok;",
            "let q = assert_ok!(checked_div(17, 5)) + 0;\nassert_eq!(q, 3);",
            false,
        ),
        result_case(
            "binding_used_by_a_method",
            "use claims::assert_ok;",
            "let q = assert_ok!(checked_div(17, 5));\nassert_eq!(q.to_string(), \"3\");\nassert_eq!(q, 3);",
            false,
        ),
        Case {
            name: "assert_some_on_a_converted_operand",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: "use claims::assert_some;",
            body: "let q = assert_some!(checked_div(17, 5).ok());\nassert_eq!(q, 3);",
            dependency: CLAIMS_DEP,
            credited: false,
        },
        Case {
            name: "claims_bound_to_a_path_package",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: "use claims::assert_ok;",
            body: bound,
            dependency: "claims = { path = \"../claims\" }",
            credited: false,
        },
        Case {
            name: "claims_renamed_to_another_package",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: "use claims::assert_ok;",
            body: bound,
            dependency: "claims = { package = \"other\", version = \"1\" }",
            credited: false,
        },
        Case {
            name: "claims_is_not_a_dependency",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "",
            imports: "use claims::assert_ok;",
            body: bound,
            dependency: "",
            credited: false,
        },
        Case {
            name: "owner_file_binds_err_as_a_value",
            owner: RESULT_OWNER,
            diff: RESULT_DIFF,
            owner_extra: "#[allow(non_snake_case)]\nfn Err(_: ()) -> Result<u32, ()> {\n    Ok(3)\n}\n",
            imports: "use claims::assert_ok;",
            body: bound,
            dependency: CLAIMS_DEP,
            credited: false,
        },
    ];
    for case in cases {
        let source = format!(
            "{}{}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n{}\n#[test]\nfn pins_the_owner() {{\n{}\n}}\n}}\n",
            case.owner, case.owner_extra, case.imports, case.body
        );
        let root = root_dir.join(case.name);
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"claims_payload_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dev-dependencies]\n{}\n[workspace]\n",
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
