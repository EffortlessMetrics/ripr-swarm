//! A `let n = <literal>;` that a later pattern rebinds does not give the
//! owner call its value (#10798). Runtime labels: with `gate(x)` changed from
//! `x >= 10` to `x > 10`, only the input `10` separates old from new, so a
//! test whose call really receives `9` must not read as `exposed`.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str = "pub fn gate(x: i32) -> i32 {\n    if x > 10 { 1 } else { 0 }\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,1 +2,1 @@
-    if x >= 10 { 1 } else { 0 }
+    if x > 10 { 1 } else { 0 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-let-literal-rebound-{}-{sequence}",
            std::process::id()
        ));
        // Built before any write so `Drop` removes a partly written root.
        let repo = Self { root };
        let root = &repo.root;
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
        let write = |path: &str, text: &str| {
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"rebound\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", LIB)?;
        write("tests/it.rs", &format!("use rebound::gate;\n\n{test}"))?;
        write("diff.patch", DIFF)?;
        Ok(repo)
    }

    fn classes(&self) -> Result<Vec<ExposureClass>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let found: Vec<ExposureClass> = output
            .findings
            .iter()
            .filter(|finding| finding.probe.location.line == 2)
            .map(|finding| finding.class.clone())
            .collect();
        if found.is_empty() {
            return Err("premise: a finding on the rewritten line".to_string());
        }
        Ok(found)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_single_literal_binding_at_the_boundary_is_credited() -> Result<(), String> {
    let found =
        TempRepo::create("#[test]\nfn t() {\n    let n = 10;\n    assert_eq!(gate(n), 0);\n}\n")?
            .classes()?;
    if !found.contains(&ExposureClass::Exposed) {
        return Err(format!("the discriminating binding must credit: {found:?}"));
    }
    Ok(())
}

#[test]
fn a_pattern_that_rebinds_the_name_is_not_credited() -> Result<(), String> {
    for (label, body) in [
        (
            "tuple let pattern",
            "    let n = 10;\n    let (n, _m) = (9, 1);\n    assert_eq!(gate(n), 0);\n",
        ),
        (
            "struct let pattern",
            "    struct P {\n        n: i32,\n    }\n    let n = 10;\n    let P { n } = P { n: 9 };\n    assert_eq!(gate(n), 0);\n",
        ),
        (
            "for pattern",
            "    let n = 10;\n    for n in [9] {\n        assert_eq!(gate(n), 0);\n    }\n",
        ),
    ] {
        let found = TempRepo::create(&format!("#[test]\nfn t() {{\n{body}}}\n"))?.classes()?;
        if found.contains(&ExposureClass::Exposed) {
            return Err(format!("{label} must not be credited: {found:?}"));
        }
    }
    Ok(())
}
