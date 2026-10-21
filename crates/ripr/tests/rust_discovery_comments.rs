//! Public-consumer controls for #6979: text in comments/strings cannot
//! substitute for a module edge to an unchanged test file.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str =
    "pub fn discount(total: u32) -> u32 {\n    if total > 100 { total - 10 } else { total }\n}\n";
const TEST: &str = "use crate::used::discount;\n#[test]\nfn discount_applies() { assert_eq!(discount(100), 100); }\n";
const DIFF: &str = "diff --git a/src/used.rs b/src/used.rs\n--- a/src/used.rs\n+++ b/src/used.rs\n@@ -1,3 +1,3 @@\n pub fn discount(total: u32) -> u32 {\n-    if total >= 100 { total - 10 } else { total }\n+    if total > 100 { total - 10 } else { total }\n }\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new(lib: &str, test_path: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let this = Self(std::env::temp_dir().join(format!(
            "ripr-discovery-comments-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        this.write(
            "Cargo.toml",
            "[package]\nname='discovery-consumer'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )?;
        this.write("src/lib.rs", &format!("pub mod used;\n{lib}\n"))?;
        this.write("src/used.rs", OWNER)?;
        this.write(test_path, TEST)?;
        this.write(
            "tests/live.rs",
            "#[test]\nfn integration_is_collected() { assert_eq!(2 + 2, 4); }\n",
        )?;
        this.write("diff.patch", DIFF)?;
        Ok(this)
    }

    fn write(&self, relative: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, text).map_err(|error| error.to_string())
    }

    fn finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut findings = output.findings.into_iter();
        match (findings.next(), findings.next()) {
            (Some(finding), None) => {
                assert_eq!(finding.probe.location.line, 2);
                assert!(finding.probe.location.file.ends_with("src/used.rs"));
                Ok(finding)
            }
            other => Err(format!(
                "expected exactly one production finding: {other:?}"
            )),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn comment_and_string_module_mentions_do_not_register_tests() -> Result<(), String> {
    for text in [
        "/* #[cfg(test)] mod used_tests; */",
        "/* outer /* nested */ mod used_tests; */",
        "/// mod used_tests;",
        "const NOTE: &str = \"mod used_tests;\";",
        "const NOTE: &str = r###\"mod used_tests;\"###;",
        "const NOTE: &[u8] = br###\"mod used_tests;\"###;",
    ] {
        let fixture = Fixture::new(text, "src/used_tests.rs")?;
        let finding = fixture.finding()?;
        assert_eq!(
            finding.class,
            ExposureClass::NoStaticPath,
            "{text}: {finding:?}"
        );
        assert!(finding.related_tests.is_empty(), "{text}: {finding:?}");
    }
    let fixture = Fixture::new("#[cfg(test)] mod tests;", "src/tests/data/used_tests.rs")?;
    fixture.write("src/tests/mod.rs", "/* mod used_tests; */\n")?;
    let finding = fixture.finding()?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    Ok(())
}

/// rustc drops a leading UTF-8 BOM, so a BOM-prefixed module file must walk
/// exactly like the same file without one (#9009).
#[test]
fn a_leading_bom_does_not_change_the_module_walk() -> Result<(), String> {
    let bom = "\u{feff}";
    let fixture = Fixture::new("#[cfg(test)] mod tests;", "src/tests/data/used_tests.rs")?;
    fixture.write("src/tests/mod.rs", &format!("{bom}/* mod used_tests; */\n"))?;
    fixture.write(
        "src/lib.rs",
        &format!("{bom}pub mod used;\n#[cfg(test)] mod tests;\n"),
    )?;
    fixture.write("src/used.rs", &format!("{bom}{OWNER}"))?;
    let finding = fixture.finding()?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");

    let fixture = Fixture::new("#[cfg(test)] mod tests;", "src/tests/data/used_tests.rs")?;
    fixture.write(
        "src/tests/mod.rs",
        &format!("{bom}#[path = \"data/used_tests.rs\"] mod renamed;\n"),
    )?;
    let finding = fixture.finding()?;
    assert_eq!(
        finding
            .related_tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        vec!["discount_applies"]
    );
    Ok(())
}

#[test]
fn real_cfg_and_renamed_modules_keep_collected_tests() -> Result<(), String> {
    for declaration in [
        "#[cfg(test)] mod used_tests;",
        "#[cfg(test)] mod /* comment */ used_tests;",
        "const NOTE: &str = \"//\"; #[cfg(test)] mod used_tests;",
        "#[cfg(test)] #[path = \"used_tests.rs\"] mod renamed;",
    ] {
        let finding = Fixture::new(declaration, "src/used_tests.rs")?.finding()?;
        assert_eq!(
            finding
                .related_tests
                .iter()
                .map(|test| test.name.as_str())
                .collect::<Vec<_>>(),
            vec!["discount_applies"]
        );
    }
    let fixture = Fixture::new("#[cfg(test)] mod tests;", "src/tests/data/used_tests.rs")?;
    fixture.write(
        "src/tests/mod.rs",
        "#[path = \"data/used_tests.rs\"] mod renamed;\n",
    )?;
    let finding = fixture.finding()?;
    assert_eq!(
        finding
            .related_tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        vec!["discount_applies"]
    );
    Ok(())
}

#[test]
fn an_incomplete_module_walk_cannot_drop_test_credit() -> Result<(), String> {
    let fixture = Fixture::new(
        "/* mod used_tests; */\nmacro_rules! opaque { () => {} }\nopaque!();",
        "src/used_tests.rs",
    )?;
    let finding = fixture.finding()?;
    assert_eq!(
        finding
            .related_tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        vec!["discount_applies"]
    );
    Ok(())
}
