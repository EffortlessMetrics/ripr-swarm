//! Package scope of the helper-entry formatting route (#8999, RIPR-SPEC-0297).
//!
//! A changed helper whose only caller is a `Display::fmt` is reached by a test
//! that formats a value of that impl's type, but only a test in the owner's own
//! package may use the route. These fixtures are real multi-package workspaces:
//! the in-package test relates, a test in another package that formats the same
//! type does not (for a root-package owner and for a member-package owner), and
//! a same-named type with the same `Display` impl in an unrelated package makes
//! the formatted type ambiguous, so nothing relates.

use ripr::domain::RelationReason;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const OLD_BODY: &str = "    s.to_string()";
const NEW_BODY: &str = "    s.replace('\\n', \" \")";

/// A `Merge` type whose `Display` calls the one helper `sanitize_label`.
fn merge_lib(tests: &str) -> String {
    format!(
        "use std::fmt;

pub struct Merge {{
    labels: Vec<String>,
}}

fn sanitize_label(s: &str) -> String {{
{NEW_BODY}
}}

impl Merge {{
    pub fn new(labels: Vec<String>) -> Self {{
        Merge {{ labels }}
    }}
}}

impl fmt::Display for Merge {{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{
        for l in &self.labels {{
            writeln!(f, \"{{}}\", sanitize_label(l))?;
        }}
        Ok(())
    }}
}}
{tests}"
    )
}

const IN_PACKAGE_TESTS: &str = "
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_package_pin() {
        let merge: Merge = Merge::new(vec![\"a\\nb\".to_string()]);
        assert_eq!(merge.to_string(), \"a b\\n\");
    }
}
";

fn pin_test(krate: &str, name: &str) -> String {
    format!(
        "use {krate}::Merge;

#[test]
fn {name}() {{
    let merge: Merge = Merge::new(vec![\"a\\nb\".to_string()]);
    assert_eq!(merge.to_string(), \"a b\\n\");
}}
"
    )
}

fn manifest(name: &str, dependency: Option<&str>, members: &[&str]) -> String {
    let mut text =
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n");
    if let Some(dependency) = dependency {
        text.push_str(&format!(
            "\n[dependencies]\n{dependency} = {{ path = \"{}\" }}\n",
            if dependency == "merged" {
                ".."
            } else {
                "other"
            }
        ));
    }
    if !members.is_empty() {
        let list: Vec<String> = members.iter().map(|m| format!("\"{m}\"")).collect();
        text.push_str(&format!("\n[workspace]\nmembers = [{}]\n", list.join(", ")));
    }
    text
}

/// Writes `files` under a fresh root and returns the tests (by name) that the
/// changed helper line in `changed_file` relates to by `helper_owner_call`.
/// The changed helper line and the finding owner it belongs to.
struct Change {
    old: &'static str,
    new: &'static str,
    owner: &'static str,
}

const SANITIZE: Change = Change {
    old: OLD_BODY,
    new: NEW_BODY,
    owner: "::sanitize_label",
};

fn helper_related(files: &[(&str, String)], changed_file: &str) -> Result<Vec<String>, String> {
    related_for(files, changed_file, &SANITIZE)
}

fn related_for(
    files: &[(&str, String)],
    changed_file: &str,
    change: &Change,
) -> Result<Vec<String>, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-fmt-package-scope-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = helper_related_in(&root, files, changed_file, change);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn helper_related_in(
    root: &Path,
    files: &[(&str, String)],
    changed_file: &str,
    change: &Change,
) -> Result<Vec<String>, String> {
    for (path, content) in files {
        let full: PathBuf = root.join(path);
        let parent = full.parent().ok_or("premise: file has a parent")?;
        std::fs::create_dir_all(parent).map_err(|error| format!("mkdir failed: {error}"))?;
        std::fs::write(&full, content).map_err(|error| format!("write {path} failed: {error}"))?;
    }
    let source = files
        .iter()
        .find(|(path, _)| *path == changed_file)
        .map(|(_, content)| content.as_str())
        .ok_or("premise: the changed file is a fixture file")?;
    let line = source
        .lines()
        .position(|line| line == change.new)
        .map(|index| index + 1)
        .ok_or("premise: the changed helper body is in the source")?;
    let (old, new) = (change.old, change.new);
    std::fs::write(
        root.join("diff.patch"),
        format!(
            "diff --git a/{changed_file} b/{changed_file}
index 1111111..2222222 100644
--- a/{changed_file}
+++ b/{changed_file}
@@ -{line},1 +{line},1 @@
-{old}
+{new}
"
        ),
    )
    .map_err(|error| format!("write diff failed: {error}"))?;
    let output = check_workspace(CheckInput {
        root: root.to_path_buf(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|finding| {
            finding.probe.location.line == line
                && finding
                    .probe
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.0.ends_with(change.owner))
        })
        .collect();
    if findings.is_empty() {
        return Err("premise: the changed helper line yields a finding".to_string());
    }
    let mut names: Vec<String> = Vec::new();
    for finding in &findings {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "reach never raises a class: {finding:?}"
        );
        names.extend(
            finding
                .related_tests
                .iter()
                .filter(|test| test.relation_reason == Some(RelationReason::HelperOwnerCall))
                .map(|test| test.name.clone()),
        );
    }
    names.sort();
    names.dedup();
    Ok(names)
}

#[test]
fn a_root_owner_relates_its_own_package_test_but_not_a_dependent_package_test() -> Result<(), String>
{
    let files = [
        ("Cargo.toml", manifest("merged", None, &["other"])),
        ("src/lib.rs", merge_lib(IN_PACKAGE_TESTS)),
        ("other/Cargo.toml", manifest("other", Some("merged"), &[])),
        ("other/src/lib.rs", "pub fn unrelated() {}\n".to_string()),
        (
            "other/tests/pin.rs",
            pin_test("merged", "dependent_package_pin"),
        ),
    ];
    assert_eq!(
        helper_related(&files, "src/lib.rs")?,
        ["in_package_pin"],
        "only the owner's own package may reach the helper through fmt"
    );
    Ok(())
}

#[test]
fn a_root_owner_relates_its_own_integration_test() -> Result<(), String> {
    let files = [
        ("Cargo.toml", manifest("merged", None, &["other"])),
        ("src/lib.rs", merge_lib("")),
        ("tests/pin.rs", pin_test("merged", "root_integration_pin")),
        ("other/Cargo.toml", manifest("other", Some("merged"), &[])),
        ("other/src/lib.rs", "pub fn unrelated() {}\n".to_string()),
        (
            "other/tests/pin.rs",
            pin_test("merged", "dependent_package_pin"),
        ),
    ];
    assert_eq!(
        helper_related(&files, "src/lib.rs")?,
        ["root_integration_pin"]
    );
    Ok(())
}

#[test]
fn a_member_owner_relates_its_own_package_test_but_not_the_root_package_test() -> Result<(), String>
{
    let files = [
        ("Cargo.toml", manifest("merged", Some("other"), &["other"])),
        ("src/lib.rs", "pub fn unrelated() {}\n".to_string()),
        ("tests/pin.rs", pin_test("other", "root_package_pin")),
        ("other/Cargo.toml", manifest("other", None, &[])),
        ("other/src/lib.rs", merge_lib("")),
        (
            "other/tests/pin.rs",
            pin_test("other", "member_package_pin"),
        ),
    ];
    assert_eq!(
        helper_related(&files, "other/src/lib.rs")?,
        ["member_package_pin"]
    );
    Ok(())
}

#[test]
fn a_same_named_type_with_the_same_display_in_another_package_fails_closed() -> Result<(), String> {
    // The twin package has its own `Merge`, `Display` impl and test, and no
    // dependency on either side. The formatted type is declared twice, so the
    // route credits no test in either package.
    let files = [
        ("Cargo.toml", manifest("merged", None, &["twin"])),
        ("src/lib.rs", merge_lib(IN_PACKAGE_TESTS)),
        ("twin/Cargo.toml", manifest("twin", None, &[])),
        ("twin/src/lib.rs", merge_lib("")),
        ("twin/tests/pin.rs", pin_test("twin", "twin_package_pin")),
    ];
    assert_eq!(
        helper_related(&files, "src/lib.rs")?,
        Vec::<String>::new(),
        "an ambiguous formatted type relates nothing"
    );
    Ok(())
}

#[test]
fn a_single_crate_root_relates_tests_under_nested_tests_directories() -> Result<(), String> {
    // `src/tests/mod.rs` and `src/foo/tests/mod.rs` are the root package's own
    // files even though their paths contain a `tests/` segment.
    let pin = |name: &str| {
        format!(
            "use crate::Merge;\n\n#[test]\nfn {name}() {{\n    let merge: Merge = Merge::new(vec![\"a\\nb\".to_string()]);\n    assert_eq!(merge.to_string(), \"a b\\n\");\n}}\n"
        )
    };
    let files = [
        ("Cargo.toml", manifest("merged", None, &[])),
        (
            "src/lib.rs",
            merge_lib("\nmod foo;\n\n#[cfg(test)]\nmod tests;\n"),
        ),
        ("src/foo/mod.rs", "#[cfg(test)]\nmod tests;\n".to_string()),
        ("src/tests/mod.rs", pin("nested_pin")),
        ("src/foo/tests/mod.rs", pin("deep_pin")),
    ];
    assert_eq!(
        helper_related(&files, "src/lib.rs")?,
        ["deep_pin", "nested_pin"]
    );
    Ok(())
}

#[test]
fn a_member_prefix_is_a_directory_not_a_name_prefix() -> Result<(), String> {
    // `app-extra/` starts with the text `app` but is another package.
    let app_manifest = "[package]\nname = \"app\"\nversion = \"0.0.0\"\nedition = \"2024\"\n";
    let extra_manifest = "[package]\nname = \"app-extra\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\napp = { path = \"../app\" }\n";
    let files = [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"app\", \"app-extra\"]\n".to_string(),
        ),
        ("app/Cargo.toml", app_manifest.to_string()),
        ("app/src/lib.rs", merge_lib("")),
        ("app/tests/pin.rs", pin_test("app", "app_pin")),
        ("app-extra/Cargo.toml", extra_manifest.to_string()),
        (
            "app-extra/src/lib.rs",
            "pub fn unrelated() {}\n".to_string(),
        ),
        ("app-extra/tests/pin.rs", pin_test("app", "app_extra_pin")),
    ];
    assert_eq!(helper_related(&files, "app/src/lib.rs")?, ["app_pin"]);
    Ok(())
}

const CARET_OLD: &str = "    ver.major >= cmp.major";
const CARET_NEW: &str = "    ver.major >= cmp.major + 1";

const CARET: Change = Change {
    old: CARET_OLD,
    new: CARET_NEW,
    owner: "::matches_caret",
};

fn caret_lib() -> String {
    "mod eval;

pub struct Version {
    pub major: u64,
}

pub struct Comparator {
    pub major: u64,
}

pub struct VersionReq {
    pub comparators: Vec<Comparator>,
}

impl VersionReq {
    pub fn matches(&self, version: &Version) -> bool {
        eval::matches_req(&self.comparators, version)
    }
}
"
    .to_string()
}

fn caret_eval() -> String {
    format!(
        "use crate::{{Comparator, Version}};

pub(crate) fn matches_req(comparators: &[Comparator], ver: &Version) -> bool {{
    for cmp in comparators {{
        if !matches_impl(cmp, ver) {{
            return false;
        }}
    }}
    true
}}

fn matches_impl(cmp: &Comparator, ver: &Version) -> bool {{
    matches_caret(cmp, ver)
}}

fn matches_caret(cmp: &Comparator, ver: &Version) -> bool {{
{CARET_NEW}
}}
"
    )
}

fn caret_pin(name: &str) -> String {
    format!(
        "use merged::{{Comparator, Version, VersionReq}};

#[test]
fn {name}() {{
    let req = VersionReq {{
        comparators: vec![Comparator {{ major: 1 }}],
    }};
    assert!(req.matches(&Version {{ major: 5 }}));
}}
"
    )
}

#[test]
fn a_root_method_entry_relates_its_own_test_but_not_a_member_dir_test() -> Result<(), String> {
    // The inherent-method route shares the in-package rule: a member under
    // `examples/` (its own manifest) is another package.
    let demo_manifest = "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nmerged = { path = \"../..\" }\n";
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = \"merged\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\nmembers = [\"examples/demo\"]\n"
                .to_string(),
        ),
        ("src/lib.rs", caret_lib()),
        ("src/eval.rs", caret_eval()),
        ("tests/api.rs", caret_pin("root_api_pin")),
        ("examples/demo/Cargo.toml", demo_manifest.to_string()),
        ("examples/demo/src/lib.rs", "pub fn unrelated() {}\n".to_string()),
        ("examples/demo/tests/pin.rs", caret_pin("demo_api_pin")),
    ];
    assert_eq!(
        related_for(&files, "src/eval.rs", &CARET)?,
        ["root_api_pin"]
    );
    Ok(())
}

#[test]
fn members_under_examples_or_tests_directories_are_other_packages() -> Result<(), String> {
    // Root-relative `examples/` and `tests/` paths are not the root package
    // when a manifest of their own sits there.
    for member in ["examples/demo", "tests/harness"] {
        let member_manifest = "[package]\nname = \"member\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dependencies]\nmerged = { path = \"../..\" }\n".to_string();
        let manifest_path = format!("{member}/Cargo.toml");
        let lib_path = format!("{member}/src/lib.rs");
        let test_path = format!("{member}/tests/pin.rs");
        let files = [
            (
                "Cargo.toml",
                format!(
                    "[package]\nname = \"merged\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\nmembers = [\"{member}\"]\n"
                ),
            ),
            ("src/lib.rs", merge_lib(IN_PACKAGE_TESTS)),
            (manifest_path.as_str(), member_manifest),
            (lib_path.as_str(), "pub fn unrelated() {}\n".to_string()),
            (test_path.as_str(), pin_test("merged", "member_pin")),
        ];
        assert_eq!(
            helper_related(&files, "src/lib.rs")?,
            ["in_package_pin"],
            "member {member}"
        );
    }
    Ok(())
}
