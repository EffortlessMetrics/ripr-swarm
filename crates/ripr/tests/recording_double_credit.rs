//! #9766 / RIPR-SPEC-0240: exact field observation must read the trait double.
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Subject(PathBuf);
impl Subject {
    fn new(source: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-recording-double-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"recording-double\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
        std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(root.join("edit.diff"), "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -13 +13 @@\n-    sink.send(format!(\"n={}\", n));\n+    sink.send(format!(\"n={n}\"));\n").map_err(|e| e.to_string())?;
        Ok(Self(root))
    }
}
impl Drop for Subject {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}
fn source(method: &str, param: &str, body: &str) -> String {
    format!(r#"pub trait Sink {{
    fn send(&mut self, line: String);
}}
pub struct Recorder {{
    pub lines: Vec<String>, pub other: Vec<String>,
}}
impl Sink for Recorder {{
    fn send(&mut self, line: String) {{
        {method}
    }}
}}
pub fn report(sink: {param}, n: u32) {{
    sink.send(format!("n={{}}", n));
}}
#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn exact_line() {{
        {body}
    }}
}}
"#)
}
const SETUP: &str = "let mut recorder = Recorder { lines: vec![], other: vec![] };\n        report(&mut recorder, 3);";
fn assert_reading(label: &str, source: &str, exposed: bool) -> Result<(), String> {
    let subject = Subject::new(source)?;
    let output = check_workspace(CheckInput {
        root: subject.0.clone(), diff_file: Some(subject.0.join("edit.diff")),
        mode: Mode::Ready, format: OutputFormat::Json, include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<_> = output.findings.iter().filter(|f| f.probe.location.line == 13).collect();
    assert!(!findings.is_empty(), "{label}: target call must produce findings");
    assert!(findings.iter().any(|f| f.related_tests.iter().any(|t| t.name.ends_with("exact_line"))), "{label}: the exact test must be related");
    assert_eq!(findings.iter().any(|f| f.class == ExposureClass::Exposed), exposed,
        "{label}: {:?}", findings.iter().map(|f| (&f.probe.family, &f.probe.expression, &f.class, &f.static_limit_kind)).collect::<Vec<_>>());
    if exposed {
        assert!(findings.iter().filter(|f| f.class == ExposureClass::Exposed).all(|f| f.static_limit_kind.is_none()), "{label}: credit cannot retain an unresolved limit");
    }
    Ok(())
}
#[test]
fn exact_recorded_argument_is_credited() -> Result<(), String> {
    for assertion in [
        "assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);",
        "assert_eq!(vec![\"n=3\".to_string()], recorder.lines);",
        "assert_eq!(recorder.lines.clone(), vec![\"n=3\".to_string()]);",
    ] {
        assert_reading(assertion, &source("self.lines.push(line);", "&mut dyn Sink", &format!("{SETUP}\n        {assertion}")), true)?;
    }
    Ok(())
}
#[test]
fn missing_argument_transfer_is_never_credited() -> Result<(), String> {
    for (label, method, assertion) in [
        ("no-op", "let _ = line;", "assert_eq!(recorder.lines, Vec::<String>::new());"),
        ("constant payload", "self.lines.push(\"fixed\".to_string());", "assert_eq!(recorder.lines, vec![\"fixed\".to_string()]);"),
        ("unrelated field", "self.other.push(line);", "assert_eq!(recorder.lines, Vec::<String>::new());"),
        ("conditional transfer", "if false { self.lines.push(line); }", "assert_eq!(recorder.lines, Vec::<String>::new());"),
        ("local argument shadow", "let line = \"fixed\".to_string(); self.lines.push(line);", "assert_eq!(recorder.lines, vec![\"fixed\".to_string()]);"),
    ] {
        assert_reading(label, &source(method, "&mut dyn Sink", &format!("{SETUP}\n        {assertion}")), false)?;
    }
    Ok(())
}
#[test]
fn recording_credit_preserves_observer_and_receiver_refusals() -> Result<(), String> {
    for (label, param, body) in [
        ("length only", "&mut dyn Sink", format!("{SETUP}\n        assert_eq!(recorder.lines.len(), 1);")),
        ("concrete receiver", "&mut Recorder", format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);")),
        ("before call", "&mut dyn Sink", "let mut recorder = Recorder { lines: vec![], other: vec![] };\n        assert_eq!(recorder.lines, Vec::<String>::new());\n        report(&mut recorder, 3);".to_string()),
        ("overwritten field", "&mut dyn Sink", format!("{SETUP}\n        recorder.lines.clear();\n        assert_eq!(recorder.lines, Vec::<String>::new());")),
        ("uninvoked assertion", "&mut dyn Sink", format!("{SETUP}\n        let _check = || assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);")),
    ] {
        assert_reading(label, &source("self.lines.push(line);", param, &body), false)?;
    }
    Ok(())
}

#[test]
fn a_double_declared_in_the_test_module_is_credited() -> Result<(), String> {
    let original = source("self.lines.push(line);", "&mut dyn Sink", &format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);"));
    let from = original.find("pub struct Recorder").ok_or("double declaration missing")?;
    let to = original.find("pub fn report").ok_or("owner declaration missing")?;
    let declarations = &original[from..to];
    let moved = original.replacen(declarations, &"\n".repeat(declarations.lines().count()), 1)
        .replacen("    #[test]", &format!("{declarations}    #[test]"), 1);
    assert_reading("test-module double", &moved, true)
}
