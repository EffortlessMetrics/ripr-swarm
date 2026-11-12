//! A `&mut self` method that assigns a field (`self.n = 1`) is exposed by a
//! test that calls it on a receiver binding and then reads that binding's
//! field exactly, including through a crate-qualified struct literal receiver
//! (`let mut c = krate::Counter { n: 5 }`) and a field name too short to be a
//! token match (#14407). Every weaker shape keeps the gap open.

use ripr::domain::RelationReason;
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

const OWNER: &str = "pub struct Counter {\n    pub n: u32,\n    pub m: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n";

/// The `field_construction` finding of the changed `self.n = 1` (base
/// `self.n = 0`) with `test` written at `test_path`.
fn finding_at(owner_source: &str, test_path: &str, test: &str) -> Result<Finding, String> {
    finding_with_files(owner_source, test_path, test, &[])
}

fn finding_with_files(
    owner_source: &str,
    test_path: &str,
    test: &str,
    extra: &[(&str, &str)],
) -> Result<Finding, String> {
    finding_in("src/lib.rs", owner_source, test_path, test, extra)
}

/// [`finding_at`] with the owner written at `owner_path` (the changed file)
/// and `extra` files written last (so one may replace the default
/// `Cargo.toml` or `src/lib.rs`).
fn finding_in(
    owner_path: &str,
    owner_source: &str,
    test_path: &str,
    test: &str,
    extra: &[(&str, &str)],
) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-method-field-assignment-{}-{sequence}",
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
        "[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )?;
    write(owner_path, owner_source)?;
    if test_path != owner_path {
        write(test_path, test)?;
    }
    for (path, text) in extra {
        if let Some(parent) = root.join(path).parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("dir {path}: {error}"))?;
        }
        write(path, text)?;
    }
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_line = lines
        .iter()
        .position(|line| line.contains("self.n = 1;"))
        .ok_or("owner has no `self.n = 1;` line")?
        + 1;
    let hunk: String = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index + 1 == changed_line {
                format!("-{}\n+{line}\n", line.replace("self.n = 1;", "self.n = 0;"))
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
                && finding.probe.family == ripr::domain::ProbeFamily::FieldConstruction
        })
        .ok_or_else(|| "no field_construction finding on the changed line".to_string())
}

fn finding(test: &str) -> Result<Finding, String> {
    finding_at(OWNER, "tests/t.rs", test)
}

fn relation(found: &Finding) -> Option<RelationReason> {
    found
        .related_tests
        .iter()
        .find(|test| test.name == "reference")
        .and_then(|test| test.relation_reason)
}

fn test_body(lines: &str) -> String {
    format!("#[test]\nfn reference() {{\n{lines}\n}}\n")
}

#[test]
fn crate_qualified_literal_receiver_field_read_is_exposed() -> Result<(), String> {
    let found = finding(&test_body(
        "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
    ))?;
    assert_eq!(
        relation(&found),
        Some(RelationReason::DirectOwnerCall),
        "{found:#?}"
    );
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn imported_literal_receiver_short_field_read_is_exposed() -> Result<(), String> {
    // The unqualified spelling already related the test; only the short field
    // name kept the exact read from confirming observation.
    let found = finding(&format!(
        "use probe::Counter;\n{}",
        test_body(
            "    let mut c = Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);"
        )
    ))?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn in_crate_unit_test_short_field_read_is_exposed() -> Result<(), String> {
    let owner = format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5, m: 0 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&owner, "src/lib.rs", "")?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

#[test]
fn weaker_reads_keep_the_field_write_gap_open() -> Result<(), String> {
    for (case, lines) in [
        (
            "never calls the owner",
            "    let c = probe::Counter { n: 1, m: 0 };\n    assert_eq!(c.n, 1);",
        ),
        (
            "reads another binding",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    let d = probe::Counter { n: 1, m: 0 };\n    c.reset();\n    assert_eq!(d.n, 1);",
        ),
        (
            "reads another field",
            "    let mut c = probe::Counter { n: 5, m: 1 };\n    c.reset();\n    assert_eq!(c.m, 1);",
        ),
        (
            "rewrites the field before the read",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    c.n = 1;\n    assert_eq!(c.n, 1);",
        ),
        (
            "rebinds the receiver before the read",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    let c = probe::Counter { n: 1, m: 0 };\n    assert_eq!(c.n, 1);",
        ),
        (
            "builds a foreign crate's namesake",
            "    let mut c = other::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        // #14407 review: the field already held the expected value before the
        // call, so a deleted or changed write still passes.
        (
            "seeds the expected value in the literal",
            "    let mut c = probe::Counter { n: 1, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "seeds the expected value by assignment",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.n = 1;\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "seeds an unread shorthand value",
            "    let n = 1;\n    let mut c = probe::Counter { n, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "declares a second binding of the name in a block",
            "    let mut c = other::Gauge { n: 5 };\n    {\n        let mut c = probe::Counter { n: 5, m: 0 };\n        c.m = 2;\n    }\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        // #14407 second review: a seed equal by value but spelled otherwise,
        // or an expected operand that is not a plain literal.
        (
            "seeds the expected value through a constant",
            "    const ONE: u32 = 1;\n    let mut c = probe::Counter { n: ONE, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "seeds the expected value through an expression",
            "    let mut c = probe::Counter { n: 0 + 1, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "expects a variable equal to the seed",
            "    let expected = 1;\n    let mut c = probe::Counter { n: 1, m: 0 };\n    c.reset();\n    assert_eq!(c.n, expected);",
        ),
        (
            "expects another binding's field equal to the seed",
            "    let mut c = probe::Counter { n: 1, m: 0 };\n    let d = probe::Counter { n: 1, m: 0 };\n    c.reset();\n    assert_eq!(c.n, d.n);",
        ),
        // #14407 second review: a pre-call write other than a plain
        // `binding.field = value;` seeds the field too.
        (
            "seeds the field through a setter",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.set_n(1);\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "reassigns the receiver before the call",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c = probe::Counter { n: 1, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "seeds the field through an alias",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    let r = &mut c;\n    r.n = 1;\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
        (
            "seeds the field through mem::replace",
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    std::mem::replace(&mut c.n, 1);\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    ] {
        let found = finding(&test_body(lines))?;
        assert_ne!(
            found.class,
            ExposureClass::Exposed,
            "{case} must not expose the write: {found:#?}"
        );
    }
    Ok(())
}

#[test]
fn local_module_shadowing_the_crate_name_gets_no_credit() -> Result<(), String> {
    let test = "mod probe {\n    pub struct Counter {\n        pub n: u32,\n    }\n    impl Counter {\n        pub fn reset(&mut self) {\n            self.n = 1;\n        }\n    }\n}\n#[test]\nfn reference() {\n    let mut c = probe::Counter { n: 5 };\n    c.reset();\n    assert_eq!(c.n, 1);\n}\n";
    let found = finding(test)?;
    assert_ne!(
        relation(&found),
        Some(RelationReason::DirectOwnerCall),
        "a local `mod probe` is not the owner's crate: {found:#?}"
    );
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// #14407 review: a test file that declares or imports its own `Counter`
/// builds its receiver from that namesake, whose `reset` is not the owner.
#[test]
fn a_namesake_counter_in_the_test_file_gets_no_credit() -> Result<(), String> {
    for (case, test) in [
        (
            "local namesake type with its own reset",
            "struct Counter {\n    n: u32,\n}\nimpl Counter {\n    fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n#[test]\nfn reference() {\n    let mut c = Counter { n: 5 };\n    c.reset();\n    assert_eq!(c.n, 1);\n}\n",
        ),
        (
            "namesake imported from another crate",
            "use foreign::Counter;\n#[test]\nfn reference() {\n    let mut c = Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);\n}\n",
        ),
    ] {
        let found = finding(test)?;
        assert_ne!(
            found.class,
            ExposureClass::Exposed,
            "{case} must not expose the write: {found:#?}"
        );
    }
    Ok(())
}

/// #14407 review: another workspace member's `Counter`, imported with a plain
/// `use`, has its own `reset`; its read is not the root owner's.
#[test]
fn a_workspace_member_namesake_gets_no_credit() -> Result<(), String> {
    let sibling = "pub struct Counter {\n    pub n: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n";
    let found = finding_with_files(
        OWNER,
        "tests/t.rs",
        &format!(
            "use sibling::Counter;\n{}",
            test_body("    let mut c = Counter { n: 5 };\n    c.reset();\n    assert_eq!(c.n, 1);")
        ),
        &[
            (
                "Cargo.toml",
                "[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[dev-dependencies]\nsibling = { path = \"sibling\" }\n\n[workspace]\nmembers = [\"sibling\"]\n",
            ),
            (
                "sibling/Cargo.toml",
                "[package]\nname = \"sibling\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
            ),
            ("sibling/src/lib.rs", sibling),
        ],
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// #14407 review: an earlier write in the owner already leaves the asserted
/// value, so deleting the changed write still passes the assertion.
#[test]
fn an_earlier_owner_write_of_the_field_gets_no_credit() -> Result<(), String> {
    let owner = "pub struct Counter {\n    pub n: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.n = 1 + 0;\n        self.n = 1;\n    }\n}\n";
    let found = finding_at(
        owner,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter { n: 5 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// #14407 CodeRabbit review: Rust reads `r#n` and `n` as one identifier, so
/// an earlier raw-spelled write of the field is still an earlier write, and a
/// raw-spelled `struct r#Counter` is still a namesake.
#[test]
fn raw_identifier_spellings_get_no_credit() -> Result<(), String> {
    let owner = "pub struct Counter {\n    pub n: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.r#n = 1;\n        self.n = 1;\n    }\n}\n";
    let found = finding_at(
        owner,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter { n: 5 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    let namesake = format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n    struct r#Counter {{\n        n: u32,\n    }}\n    impl r#Counter {{\n        fn reset(&mut self) {{\n            self.n = 1;\n        }}\n    }}\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&namesake, "src/lib.rs", "")?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// #14407 CodeRabbit review: a `#[path]` attribute can map the owner's file to
/// another module, so the file path no longer names the module a root
/// re-export reaches.
#[test]
fn a_path_remapped_module_gets_no_reexport_credit() -> Result<(), String> {
    let test = test_body(
        "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
    );
    let other = "pub struct Counter {\n    pub n: u32,\n    pub m: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n";
    let remapped = finding_in(
        "src/owner.rs",
        OWNER,
        "tests/t.rs",
        &test,
        &[
            (
                "src/lib.rs",
                "#[path = \"owner.rs\"]\nmod internal;\n#[path = \"counter.rs\"]\npub mod owner;\npub use owner::Counter;\n",
            ),
            ("src/counter.rs", other),
        ],
    )?;
    assert_ne!(remapped.class, ExposureClass::Exposed, "{remapped:#?}");
    let control = finding_in(
        "src/owner.rs",
        OWNER,
        "tests/t.rs",
        &test,
        &[("src/lib.rs", "pub mod owner;\npub use owner::Counter;\n")],
    )?;
    assert_eq!(control.class, ExposureClass::Exposed, "{control:#?}");
    Ok(())
}

/// #14407 CodeRabbit review: an expected literal other than the one the owner
/// writes fails with or without the write, and a receiver whose `Drop` may
/// end the process during unwinding can turn the failing assertion into a
/// pass.
#[test]
fn an_unwritten_expected_value_or_a_custom_drop_gets_no_credit() -> Result<(), String> {
    let found = finding_at(
        OWNER,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 3);",
        ),
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    let dropping = format!(
        "{OWNER}impl Drop for Counter {{\n    fn drop(&mut self) {{\n        if self.n != 1 {{\n            std::process::exit(0);\n        }}\n    }}\n}}\n"
    );
    let found = finding_at(
        &dropping,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    let control = finding_at(
        OWNER,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    )?;
    assert_eq!(control.class, ExposureClass::Exposed, "{control:#?}");
    Ok(())
}

/// #14407 second review: an inline `mod tests` in the owner's own file that
/// declares its own `Counter` builds its receiver from that namesake.
#[test]
fn a_namesake_counter_in_the_owner_file_gets_no_credit() -> Result<(), String> {
    let owner = format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n    struct Counter {{\n        n: u32,\n    }}\n    impl Counter {{\n        fn reset(&mut self) {{\n            self.n = 1;\n        }}\n    }}\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&owner, "src/lib.rs", "")?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    let renamed = format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n    use other::Gauge as Counter;\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&renamed, "src/lib.rs", "")?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// #14407 review: a receiver built by a constructor has a pre-call field value
/// the read rule does not see (`new()` already sets `n: 1`), so a deleted write
/// may pass. Only a visible struct literal earns the short-field read credit.
#[test]
fn a_constructor_built_receiver_gets_no_short_field_credit() -> Result<(), String> {
    let owner = "pub struct Counter {\n    pub n: u32,\n    pub m: u32,\n}\nimpl Counter {\n    pub fn new() -> Counter {\n        Counter { n: 1, m: 0 }\n    }\n    pub fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n";
    let found = finding_at(
        owner,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter::new();\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    )?;
    assert_eq!(
        relation(&found),
        Some(RelationReason::DirectOwnerCall),
        "the receiver still reaches the owner: {found:#?}"
    );
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    // Control: the same owner with a struct-literal receiver is exposed.
    let literal = finding_at(
        owner,
        "tests/t.rs",
        &test_body(
            "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
        ),
    )?;
    assert_eq!(literal.class, ExposureClass::Exposed, "{literal:#?}");
    Ok(())
}

const O_FIELD: &str =
    "    let mut c = Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);";

/// #14407 exact-head review F1: a test whose `Counter` is an external type
/// (through a foreign glob, or a library module re-exporting a dependency's
/// type) cannot run the owner, so its read gets no credit. The control with
/// the owner's own import is exposed.
#[test]
fn an_external_namesake_receiver_gets_no_credit() -> Result<(), String> {
    let glob = finding(&format!("use foreign::*;\n{}", test_body(O_FIELD)))?;
    assert_ne!(glob.class, ExposureClass::Exposed, "{glob:#?}");
    let reexported = finding_with_files(
        &format!("{OWNER}pub mod other;\n"),
        "tests/t.rs",
        &format!("use probe::other::Counter;\n{}", test_body(O_FIELD)),
        &[("src/other.rs", "pub use dep::Counter;\n")],
    )?;
    assert_ne!(reexported.class, ExposureClass::Exposed, "{reexported:#?}");
    let control = finding(&format!("use probe::Counter;\n{}", test_body(O_FIELD)))?;
    assert_eq!(control.class, ExposureClass::Exposed, "{control:#?}");
    Ok(())
}

/// #14407 exact-head review F2: `probe::Counter { .. }` names the crate root's
/// type. An owner declared in a module is that type only when `src/lib.rs`
/// re-exports it from the owner's module.
#[test]
fn a_crate_root_literal_names_a_module_owner_only_through_its_reexport() -> Result<(), String> {
    let test = test_body(
        "    let mut c = probe::Counter { n: 5, m: 0 };\n    c.reset();\n    assert_eq!(c.n, 1);",
    );
    let foreign_root = finding_in(
        "src/other.rs",
        OWNER,
        "tests/t.rs",
        &test,
        &[("src/lib.rs", "pub mod other;\npub use dep::Counter;\n")],
    )?;
    assert_ne!(
        foreign_root.class,
        ExposureClass::Exposed,
        "{foreign_root:#?}"
    );
    let own_root = finding_in(
        "src/other.rs",
        OWNER,
        "tests/t.rs",
        &test,
        &[("src/lib.rs", &format!("pub mod other;\n{OWNER}"))],
    )?;
    assert_ne!(own_root.class, ExposureClass::Exposed, "{own_root:#?}");
    let reexport = finding_in(
        "src/other.rs",
        OWNER,
        "tests/t.rs",
        &test,
        &[("src/lib.rs", "pub mod other;\npub use other::Counter;\n")],
    )?;
    assert_eq!(reexport.class, ExposureClass::Exposed, "{reexport:#?}");
    Ok(())
}

/// #14407 exact-head review F3: an unrelated same-named type elsewhere in the
/// workspace does not withdraw the credit of a test that binds the owner's
/// own type.
#[test]
fn an_unrelated_namesake_elsewhere_keeps_the_credit() -> Result<(), String> {
    let found = finding_with_files(
        &format!("{OWNER}pub mod other;\n"),
        "tests/t.rs",
        &format!("use probe::Counter;\n{}", test_body(O_FIELD)),
        &[("src/other.rs", "pub struct Counter {\n    pub w: u32,\n}\n")],
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}

/// #14407 review of the binding repair: a same-file test importing a foreign
/// `Counter`, an integration test whose `self::` glob reaches a test-crate
/// namesake, and an inline-module owner behind a root re-export of another
/// type all keep the gap.
#[test]
fn unbound_namesake_routes_get_no_credit() -> Result<(), String> {
    let same_file_import = format!(
        "{OWNER}#[cfg(test)]\nmod tests {{\n    use dep::Counter;\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5, m: 0 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&same_file_import, "src/lib.rs", "")?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    let test_crate_glob = finding_with_files(
        OWNER,
        "tests/t.rs",
        &format!("mod common;\nuse self::common::*;\n{}", test_body(O_FIELD)),
        &[(
            "tests/common/mod.rs",
            "pub struct Counter {\n    pub n: u32,\n    pub m: u32,\n}\nimpl Counter {\n    pub fn reset(&mut self) {\n        self.n = 1;\n    }\n}\n",
        )],
    )?;
    assert_ne!(
        test_crate_glob.class,
        ExposureClass::Exposed,
        "{test_crate_glob:#?}"
    );
    let test_crate_reexport = finding_with_files(
        OWNER,
        "tests/t.rs",
        &format!("mod common;\nuse self::common::*;\n{}", test_body(O_FIELD)),
        &[("tests/common/mod.rs", "pub use extdep::Counter;\n")],
    )?;
    assert_ne!(
        test_crate_reexport.class,
        ExposureClass::Exposed,
        "{test_crate_reexport:#?}"
    );
    let foreign_glob_module = finding_with_files(
        OWNER,
        "tests/t.rs",
        &format!("mod common;\nuse self::common::*;\n{}", test_body(O_FIELD)),
        &[("tests/common/mod.rs", "pub use extdep::*;\n")],
    )?;
    assert_ne!(
        foreign_glob_module.class,
        ExposureClass::Exposed,
        "{foreign_glob_module:#?}"
    );
    let owner_file_reexport = format!(
        "{OWNER}mod ext {{\n    pub use extdep::Counter;\n}}\n#[cfg(test)]\nmod tests {{\n    use super::ext::*;\n    #[test]\n    fn reference() {{\n        let mut c = Counter {{ n: 5, m: 0 }};\n        c.reset();\n        assert_eq!(c.n, 1);\n    }}\n}}\n"
    );
    let found = finding_at(&owner_file_reexport, "src/lib.rs", "")?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    let macro_namesake = finding_with_files(
        OWNER,
        "tests/t.rs",
        &format!("mk!(Counter);\n{}", test_body(O_FIELD)),
        &[(
            "tests/support.rs",
            "#[macro_export]\nmacro_rules! mk {\n    ($t:ident) => {\n        pub struct $t { pub n: u32, pub m: u32 }\n    };\n}\n",
        )],
    )?;
    assert_ne!(
        macro_namesake.class,
        ExposureClass::Exposed,
        "{macro_namesake:#?}"
    );
    let inline_owner = "pub mod inner {\n    pub struct Counter {\n        pub n: u32,\n        pub m: u32,\n    }\n    impl Counter {\n        pub fn reset(&mut self) {\n            self.n = 1;\n        }\n    }\n}\npub use dep::Counter;\n";
    let found = finding_at(
        inline_owner,
        "tests/t.rs",
        &format!("use probe::Counter;\n{}", test_body(O_FIELD)),
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:#?}");
    Ok(())
}
