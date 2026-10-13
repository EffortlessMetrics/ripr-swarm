//! End-to-end witness for uncased enum variants in RIPR-SPEC-0229 arm
//! selection (#7543).
//!
//! `Kind::A` may name a variant or an associated constant, which a pattern
//! compares by value. Arm selection reads it as a variant only when the
//! owner's file declares `enum Kind` with that variant, so examples 4 and 6
//! name their missing discriminators with single-letter variants, and the
//! same crate with `Kind` imported from another file names none.

use ripr::{CheckInput, CheckOutput, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const ENUM: &str = "#[derive(Debug, Clone, Copy, PartialEq)]\npub enum Kind {\n    A,\n    B,\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// `lib` is the new `src/lib.rs`; `diff` changes its line 10 (with the
    /// enum in `src/lib.rs`) or line 7 (with it in `src/kinds.rs`).
    fn create(lib: &str, kinds: Option<&str>, test: &str, diff: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-match-arm-uncased-variant-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        let write = |path: &str, text: &str| {
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"uncased-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", lib)?;
        if let Some(kinds) = kinds {
            write("src/kinds.rs", kinds)?;
        }
        write("tests/it.rs", test)?;
        write("diff.patch", diff)?;
        Ok(Self { root })
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn body(arms: &str, returns: &str) -> String {
    format!("pub fn f(k: Kind) -> {returns} {{\n    match k {{\n{arms}    }}\n}}\n")
}

fn diff(path: &str, line: usize, context: &[&str], old: &str, new: &str) -> String {
    // `context` holds the three lines before the changed one; `}` closers
    // follow it.
    let start = line - 3;
    let before = context
        .iter()
        .map(|line| format!(" {line}\n"))
        .collect::<String>();
    format!(
        "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -{start},6 +{start},6 @@\n{before}-{old}\n+{new}\n     }}\n }}\n"
    )
}

/// The missing-discriminator values of the one changed match-arm finding.
fn named_discriminators(output: &CheckOutput, line: usize) -> Result<Vec<String>, String> {
    let mut arms = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::MatchArm && finding.probe.location.line == line
    });
    let finding = arms.next().ok_or_else(|| {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        format!("missing the changed match arm at line {line}; observed {observed}")
    })?;
    if format!("{output:?}").contains("MalformedDiff") {
        return Err("the fixture diff must parse cleanly".to_string());
    }
    Ok(finding
        .activation
        .missing_discriminators
        .iter()
        .map(|missing| missing.value.clone())
        .collect())
}

const WILDCARD_ARMS: &str = "        Kind::A => 1,\n        _ => 2,\n";
const WILDCARD_TEST: &str = "use uncased_variant::{Kind, f};\n\n#[test]\nfn kind_a_is_one() {\n    assert_eq!(f(Kind::A), 1);\n}\n";

#[test]
fn a_declared_single_letter_enum_names_the_unselected_wildcard() -> Result<(), String> {
    // RIPR-SPEC-0229 example 4: `f(Kind::A)` takes the earlier arm.
    let lib = format!("{ENUM}\n{}", body(WILDCARD_ARMS, "i32"));
    let repo = TempRepo::create(
        &lib,
        None,
        WILDCARD_TEST,
        &diff(
            "src/lib.rs",
            10,
            &[
                "pub fn f(k: Kind) -> i32 {",
                "    match k {",
                "        Kind::A => 1,",
            ],
            "        _ => 0,",
            "        _ => 2,",
        ),
    )?;
    assert_eq!(named_discriminators(&repo.check()?, 10)?, vec!["_"]);
    Ok(())
}

#[test]
fn a_declared_single_letter_enum_names_the_unselected_variant_arm() -> Result<(), String> {
    // RIPR-SPEC-0229 example 6: only the expected side names `Kind::B`.
    let arms = "        Kind::A => Kind::B,\n        Kind::B => Kind::A,\n";
    let lib = format!("{ENUM}\n{}", body(arms, "Kind"));
    let test = "use uncased_variant::{Kind, f};\n\n#[test]\nfn a_maps_to_b() {\n    assert_eq!(f(Kind::A), Kind::B);\n}\n";
    let repo = TempRepo::create(
        &lib,
        None,
        test,
        &diff(
            "src/lib.rs",
            10,
            &[
                "pub fn f(k: Kind) -> Kind {",
                "    match k {",
                "        Kind::A => Kind::B,",
            ],
            "        Kind::B => Kind::B,",
            "        Kind::B => Kind::A,",
        ),
    )?;
    assert_eq!(named_discriminators(&repo.check()?, 10)?, vec!["Kind::B"]);
    Ok(())
}

#[test]
fn an_imported_single_letter_type_names_no_arm() -> Result<(), String> {
    // The same crate with `Kind` declared in another file: the owner's file
    // does not show `A` is a variant, so `Kind::A` may be a constant and the
    // wildcard is not shown unselected.
    let lib = format!(
        "mod kinds;\npub use kinds::Kind;\n\n{}",
        body(WILDCARD_ARMS, "i32")
    );
    let repo = TempRepo::create(
        &lib,
        Some(ENUM),
        WILDCARD_TEST,
        &diff(
            "src/lib.rs",
            7,
            &[
                "pub fn f(k: Kind) -> i32 {",
                "    match k {",
                "        Kind::A => 1,",
            ],
            "        _ => 0,",
            "        _ => 2,",
        ),
    )?;
    assert_eq!(
        named_discriminators(&repo.check()?, 7)?,
        Vec::<String>::new()
    );
    Ok(())
}
