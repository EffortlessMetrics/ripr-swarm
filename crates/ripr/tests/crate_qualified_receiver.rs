//! A crate-qualified constructor receiver (`krate::Rng::new()`) keeps its call
//! path to the owner's own method, and only for the owner's own crate (#14036).

use ripr::domain::{RelationConfidence, RelationReason};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const OWNER: &str = "pub struct Rng(u32);\nimpl Rng {\n    pub fn new() -> Rng {\n        Rng(0)\n    }\n    pub fn digit(&mut self, base: u32) -> u32 {\n        if base > 9 { 2 } else { 1 }\n    }\n}\n";

/// The predicate finding of `digit` with `test` as the only integration test.
fn finding(test: &str) -> Result<Finding, String> {
    finding_at("tests/t.rs", test)
}

fn finding_at(test_path: &str, test: &str) -> Result<Finding, String> {
    finding_for(OWNER, test_path, test)
}

fn finding_for(owner_source: &str, test_path: &str, test: &str) -> Result<Finding, String> {
    finding_with_files(owner_source, &[(test_path, test)])
}

/// The predicate finding of `digit` with every `(path, text)` of `test_files`
/// written beside the owner.
fn finding_with_files(owner_source: &str, test_files: &[(&str, &str)]) -> Result<Finding, String> {
    finding_in("src/lib.rs", owner_source, &[], test_files)
}

/// The predicate finding of `digit` when the owner lives in `owner_path`, the
/// library also holds `extra` files, and every `(path, text)` of `test_files`
/// is written beside them.
fn finding_in(
    owner_path: &str,
    owner_source: &str,
    extra: &[(&str, &str)],
    test_files: &[(&str, &str)],
) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-crate-qualified-receiver-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("tests")).map_err(|error| format!("tests dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "Cargo.toml",
        "[package]\nname = \"rg\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write(owner_path, owner_source)?;
    for (path, text) in extra {
        write(path, text)?;
    }
    for (test_path, test) in test_files {
        if let Some(parent) = std::path::Path::new(test_path).parent() {
            std::fs::create_dir_all(root.join(parent))
                .map_err(|error| format!("test dir: {error}"))?;
        }
        write(test_path, test)?;
    }
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_line = lines
        .iter()
        .position(|line| line.contains("base > 9"))
        .ok_or("owner has no `base > 9` line")?
        + 1;
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!("-{}\n+{line}\n", line.replace("base > 9", "base >= 10"))
            } else {
                format!(" {line}\n")
            }
        })
        .collect();
    write(
        "diff.patch",
        &format!(
            "diff --git a/{owner_path} b/{owner_path}\nindex 0000000..1111111 100644\n--- a/{owner_path}\n+++ b/{owner_path}\n@@ -1,{n} +1,{n} @@\n{hunk}",
            n = lines.len()
        ),
    )?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check failed: {error}"))?;
    output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == changed_line
                && finding.probe.family == ripr::domain::ProbeFamily::Predicate
        })
        .ok_or_else(|| "no predicate finding on the changed line".to_string())
}

fn related_reason(
    found: &Finding,
    name: &str,
) -> Result<(Option<RelationReason>, Option<RelationConfidence>), String> {
    found
        .related_tests
        .iter()
        .find(|test| test.name == name)
        .map(|test| (test.relation_reason, test.relation_confidence))
        .ok_or_else(|| format!("missing related test {name}: {found:#?}"))
}

#[test]
fn own_crate_qualified_constructor_receiver_is_a_direct_owner_call() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n    assert_eq!(rng.digit(9), 1);\n}\n";
    let found = finding(test)?;
    assert_eq!(
        related_reason(&found, "rng_digit")?,
        (
            Some(RelationReason::DirectOwnerCall),
            Some(RelationConfidence::High)
        ),
        "{found:#?}"
    );
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn imported_constructor_receiver_is_the_unchanged_control() -> Result<(), String> {
    let test = "use rg::Rng;\n#[test]\nfn rng_digit() {\n    let mut rng = Rng::new();\n    assert_eq!(rng.digit(10), 2);\n    assert_eq!(rng.digit(9), 1);\n}\n";
    let found = finding(test)?;
    assert_eq!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn foreign_crate_qualified_receiver_keeps_no_direct_call() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = other_crate::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "a foreign crate's same-named type is not the owner: {found:#?}"
    );
    Ok(())
}

#[test]
fn local_module_shadowing_the_crate_name_keeps_no_direct_call() -> Result<(), String> {
    // The shadow adds no second inherent `Rng::new` (its constructor is a trait
    // impl), so the owner's `new` stays unique and only the shadow scan refuses.
    let test = "mod rg {\n    pub struct Rng;\n    pub trait Make {\n        fn new() -> Rng;\n    }\n    impl Make for Rng {\n        fn new() -> Rng { Rng }\n    }\n    impl Rng {\n        pub fn digit(&mut self, _base: u32) -> u32 { 0 }\n    }\n}\nuse rg::Make;\n#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 0);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "a local `mod rg` is not the owner's crate: {found:#?}"
    );
    Ok(())
}

#[test]
fn renamed_import_of_the_crate_name_keeps_no_direct_call() -> Result<(), String> {
    let test = "use other_crate as rg;\n#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "`rg` is another crate here: {found:#?}"
    );
    Ok(())
}

#[test]
fn test_module_child_file_keeps_no_direct_call() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n}\n";
    // `tests/sub/main.rs` is the test-crate root and declares `t`, so the
    // child is a linked module rather than an orphan file. The test must be
    // related for the guard to be what withholds the direct call: an unrelated
    // orphan passed with the guard removed (#14083 T8).
    let found = finding_with_files(
        OWNER,
        &[("tests/sub/main.rs", "mod t;\n"), ("tests/sub/t.rs", test)],
    )?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "a module child of a test crate is not a test-crate root: {found:#?}"
    );
    // Control: the same body in the test-crate root itself is a direct call.
    let root = finding_at("tests/sub/main.rs", test)?;
    assert_eq!(
        related_reason(&root, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "{root:#?}"
    );
    Ok(())
}

#[test]
fn enum_variant_spelling_is_not_a_constructor_receiver() -> Result<(), String> {
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Mode::Rng(1);\n    assert_eq!(rng.digit(10), 2);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn function_local_namesake_type_method_gets_no_credit() -> Result<(), String> {
    // The changed `digit` belongs to a struct declared inside `helper`, not to
    // the exported `Rng`; `rg::Rng::new()` cannot reach it.
    let owner = "pub struct Rng(u32);\nimpl Rng {\n    pub fn new() -> Rng {\n        Rng(0)\n    }\n}\npub fn helper() -> u32 {\n    struct Rng;\n    impl Rng {\n        fn digit(&mut self, base: u32) -> u32 {\n            if base > 9 { 2 } else { 1 }\n        }\n    }\n    Rng.digit(3)\n}\n";
    let test = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n}\n";
    let found = finding_for(owner, "tests/t.rs", test)?;
    assert!(
        !found.related_tests.iter().any(|related| {
            related.name == "rng_digit"
                && related.relation_reason == Some(RelationReason::DirectOwnerCall)
        }),
        "a function-local type's method is not reachable through the exported type: {found:#?}"
    );
    Ok(())
}

/// The owner's type without a constructor, as a library module.
const OWNER_WITHOUT_NEW: &str = "pub struct Rng(u32);\nimpl Rng {\n    pub fn digit(&mut self, base: u32) -> u32 {\n        if base > 9 { 2 } else { 1 }\n    }\n}\n";

/// A second library module's same-named type, with the constructor.
const RIVAL_WITH_NEW: &str = "pub struct Rng(u32);\nimpl Rng {\n    pub fn new() -> Rng {\n        Rng(0)\n    }\n    pub fn digit(&mut self, base: u32) -> u32 {\n        if base >= 10 { 2 } else { 1 }\n    }\n}\n";

const CRATE_QUALIFIED_TEST: &str = "#[test]\nfn rng_digit() {\n    let mut rng = rg::Rng::new();\n    assert_eq!(rng.digit(10), 2);\n    assert_eq!(rng.digit(9), 1);\n}\n";

/// #14083 A2: `rg::Rng` is the re-exported `b::Rng`, whose `new` and `digit`
/// run; the changed `a::Rng::digit` is another type's method and never does.
#[test]
fn crate_qualified_receiver_of_a_same_named_type_in_another_module_gets_no_direct_call()
-> Result<(), String> {
    let found = finding_in(
        "src/a.rs",
        OWNER_WITHOUT_NEW,
        &[
            ("src/lib.rs", "mod a;\nmod b;\npub use b::Rng;\n"),
            ("src/b.rs", RIVAL_WITH_NEW),
        ],
        &[("tests/t.rs", CRATE_QUALIFIED_TEST)],
    )?;
    assert!(
        found
            .related_tests
            .iter()
            .all(|test| test.relation_reason != Some(RelationReason::DirectOwnerCall)),
        "{found:#?}"
    );
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// Control for the case above: the same module layout with one `Rng`, which
/// the crate-qualified path can only name, keeps the direct owner call.
#[test]
fn crate_qualified_receiver_of_the_only_type_in_a_module_stays_a_direct_owner_call()
-> Result<(), String> {
    let found = finding_in(
        "src/a.rs",
        OWNER,
        &[("src/lib.rs", "mod a;\npub use a::Rng;\n")],
        &[("tests/t.rs", CRATE_QUALIFIED_TEST)],
    )?;
    assert_eq!(
        related_reason(&found, "rng_digit")?.0,
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}
