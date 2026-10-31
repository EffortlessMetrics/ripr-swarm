//! Return-value controls for literal concat! evaluation (#11248).
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn classify(
    prefix: &str,
    expression: &str,
    assertion: &str,
    reject_all: bool,
) -> Result<ExposureClass, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-concat-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"concat-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    let source = format!(
        "{prefix}\npub fn title() -> &'static str {{\n    {expression}\n}}\npub fn banner() -> String {{\n    format!(\"[{{}}]\", title())\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn test() {{\n        {assertion}\n    }}\n}}\n"
    );
    let line = prefix.lines().count().max(1) + 2;
    std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-    \"ripr\"\n+    {expression}\n"
    );
    std::fs::write(root.join("change.diff"), diff).map_err(|e| e.to_string())?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        diff_file: Some(root.join("change.diff")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    if reject_all
        && output
            .findings
            .iter()
            .any(|f| f.probe.location.line == line && f.class == ExposureClass::Exposed)
    {
        return Err(format!("unexpected exposed finding: {:?}", output.findings));
    }
    let result = output
        .findings
        .iter()
        .find(|f| f.probe.family == ProbeFamily::ReturnValue && f.probe.location.line == line)
        .or_else(|| {
            output.findings.iter().find(|f| {
                f.probe.location.line == line && f.probe.family == ProbeFamily::CallDeletion
            })
        })
        .map(|f| f.class.clone())
        .ok_or_else(|| format!("no candidate finding: {:?}", output.findings));
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    result
}

#[test]
fn literal_concat_return_and_forwarding() -> Result<(), String> {
    for assertion in [
        "assert_eq!(title(), \"ripr\");",
        "assert_eq!(banner(), \"[ripr]\");",
    ] {
        assert_eq!(
            classify("", "concat!(\"ri\", \"pr\")", assertion, false)?,
            ExposureClass::Exposed,
            "{assertion}"
        );
    }
    assert_eq!(
        classify(
            "",
            r###"concat!(r#"ri"#, "\x70r")"###,
            "assert_eq!(title(), \"ripr\");",
            false
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn opaque_and_shadowed_concat_remain_uncredited() -> Result<(), String> {
    for (prefix, expression) in [
        ("", "concat!(\"ri\", env!(\"RIPR_PART\"))"),
        (
            "macro_rules! concat { ($($t:tt)*) => { \"wrong\" }; }",
            "concat!(\"ri\", \"pr\")",
        ),
        ("", "other!(\"ri\", \"pr\")"),
        ("", "concat!(\"ri\", 1)"),
        ("", "concat![\"ri\", \"pr\"]"),
        ("", "concat!{\"ri\", \"pr\"}"),
        ("", "concat!()"),
        ("", "std::concat!(\"ri\", \"pr\")"),
    ] {
        assert_ne!(
            classify(prefix, expression, "assert_eq!(title(), \"ripr\");", true)?,
            ExposureClass::Exposed,
            "{expression}"
        );
    }
    Ok(())
}
