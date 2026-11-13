//! Production-path witness for #14115: the "assertion not credited" line
//! names the import that blocks credit, not an unrelated standard `assert!`
//! whose arguments nest `assert_eq!`. Static evidence only: the native runs
//! recorded on the issue are the execution proof.

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

const LIB: &str = "mod a_nested;\nmod suite;\n\npub fn weight(x: u32) -> u32 {\n    x * 4\n}\n\npub fn check(f: impl FnOnce() -> bool) -> bool {\n    f()\n}\n";
/// A standard `assert!` over a closure that asserts with `assert_eq!`.
const NESTED: &str = "#[test]\nfn nested() {\n    assert!(crate::check(|| {\n        assert_eq!(1, 1);\n        true\n    }));\n}\n";
/// The same test with the inner assertion outside the `assert!` arguments.
const UNNESTED: &str = "#[test]\nfn nested() {\n    let ok = crate::check(|| {\n        assert_eq!(1, 1);\n        true\n    });\n    assert!(ok);\n}\n";
const GLOB: &str = "use rayon::prelude::*;\n";
const WEIGHS: &str = "#[test]\nfn weighs() {\n    assert_eq!(crate::weight(4), 12);\n}\n";

/// The finding for `weight`'s changed `x * 4` (was `x * 3`) with the given
/// `src/a_nested.rs` and `src/suite.rs`.
fn finding(nested: &str, suite: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-standard-wrapper-disclosure-{}-{sequence}",
            std::process::id()
        )),
    };
    let root = &repo.root;
    let package = root.join("pkg");
    std::fs::create_dir_all(package.join("src")).map_err(|error| format!("src dir: {error}"))?;
    std::fs::create_dir_all(root.join("rayon/src")).map_err(|error| format!("dep dir: {error}"))?;
    let write = |path: &str, text: &str| {
        std::fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))
    };
    // A path dependency outside the package: ripr does not index it, and it
    // is not the registry `rayon`.
    write(
        "rayon/Cargo.toml",
        "[package]\nname = \"rayon\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )?;
    write(
        "rayon/src/lib.rs",
        "pub mod prelude {\n    pub struct Unrelated;\n}\n",
    )?;
    write(
        "pkg/Cargo.toml",
        "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\nrayon = { path = \"../rayon\" }\n\n[workspace]\n",
    )?;
    write("pkg/src/lib.rs", LIB)?;
    write("pkg/src/a_nested.rs", nested)?;
    write("pkg/src/suite.rs", suite)?;
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3,4 +3,4 @@\n \n pub fn weight(x: u32) -> u32 {\n-    x * 3\n+    x * 4\n }\n";
    write("pkg/diff.patch", diff)?;
    let output = check_workspace(CheckInput {
        root: package.clone(),
        base: None,
        diff_file: Some(package.join("diff.patch")),
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
            finding.probe.location.line == 5 && finding.probe.expression.contains("x * 4")
        })
        .ok_or_else(|| "no finding for the changed `x * 4`".to_string())
}

fn refusals(finding: &Finding) -> Vec<&str> {
    finding
        .evidence
        .iter()
        .map(String::as_str)
        .filter(|line| line.starts_with("assertion not credited: "))
        .collect()
}

#[test]
fn a_nested_standard_assertion_alone_blocks_nothing() -> Result<(), String> {
    let found = finding(NESTED, WEIGHS)?;
    assert_eq!(found.class, ExposureClass::Exposed, "{:?}", found.evidence);
    assert_eq!(refusals(&found), Vec::<&str>::new());
    Ok(())
}

#[test]
fn the_refusal_names_the_glob_in_the_tests_own_file() -> Result<(), String> {
    let suite = format!("{GLOB}\n{WEIGHS}");
    // Control: with no nested call anywhere, the glob is the named site.
    let control = finding(UNNESTED, &suite)?;
    let expected = refusals(&control)
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(expected.len(), 1, "{:?}", control.evidence);
    assert!(
        expected[0].contains("`use rayon::prelude::*;` at src/suite.rs:1 glob-imports"),
        "{expected:?}"
    );
    assert_ne!(control.class, ExposureClass::Exposed);

    // The nested standard call sits in a file that sorts before the test's.
    // It is not a wrapper, so the named site and the class do not change.
    let found = finding(NESTED, &suite)?;
    assert_eq!(refusals(&found), expected, "{:?}", found.evidence);
    assert_eq!(found.class, control.class);
    assert!(
        found
            .evidence
            .iter()
            .all(|line| !line.contains("in its arguments")),
        "{:?}",
        found.evidence
    );
    Ok(())
}
