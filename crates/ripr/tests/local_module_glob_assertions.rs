//! #5500: a resolved source-visible local glob supplies no foreign macro.
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub(crate) fn charge() -> u64 {\n    600\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn observed() { assert_eq!(charge(), 600); }\n}\n";
const DIFF: &str = "--- a/src/proof.rs\n+++ b/src/proof.rs\n@@ -1,3 +1,3 @@\n pub(crate) fn charge() -> u64 {\n-    2\n+    600\n }\n";

fn classification(
    root_source: &str,
    owner: &str,
) -> Result<ExposureClass, Box<dyn std::error::Error>> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-local-glob-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='local_glob'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
    )?;
    std::fs::write(root.join("src/lib.rs"), root_source)?;
    std::fs::write(root.join("src/proof.rs"), owner)?;
    std::fs::write(root.join("input.diff"), DIFF)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("input.diff")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings = output
        .findings
        .iter()
        .filter(|finding| finding.id == "probe:src_proof.rs:return_value:d87248f4")
        .collect::<Vec<_>>();
    assert_eq!(findings.len(), 1, "exact changed return subject survives");
    let class = findings[0].class.clone();
    std::fs::remove_dir_all(root)?;
    Ok(class)
}

#[test]
fn resolved_local_module_glob_keeps_standard_assertion_identity()
-> Result<(), Box<dyn std::error::Error>> {
    for import in ["proof", "crate::proof", "r#proof"] {
        let source = format!("mod proof;\npub(crate) use {import}::*;\n");
        assert_eq!(
            classification(&source, OWNER)?,
            ExposureClass::Exposed,
            "{import}"
        );
    }
    Ok(())
}

#[test]
fn custom_macro_and_opaque_module_exports_never_gain_assertion_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let custom =
        "macro_rules! assert_eq { ($($value:tt)*) => { () }; }\npub(crate) use assert_eq;\n";
    // Keep charge at its exact source coordinate; the custom macro's scope
    // covers the test and its re-export is visible to the existing scanner.
    let shadowed = OWNER.replace("#[cfg(test)]", &format!("{custom}#[cfg(test)]"));
    assert_eq!(
        classification("mod proof;\npub(crate) use proof::*;\n", &shadowed)?,
        ExposureClass::StaticUnknown
    );
    let opaque = OWNER.replace(
        "#[cfg(test)]",
        "macro_rules! opaque { () => { struct Extra; }; }\nopaque!();\n#[cfg(test)]",
    );
    assert_eq!(
        classification("mod proof;\npub(crate) use proof::*;\n", &opaque)?,
        ExposureClass::StaticUnknown
    );
    assert_eq!(
        classification(
            "mod proof;\npub(crate) use proof::*;\nuse core::primitive::*;\n",
            OWNER
        )?,
        ExposureClass::StaticUnknown
    );
    Ok(())
}
