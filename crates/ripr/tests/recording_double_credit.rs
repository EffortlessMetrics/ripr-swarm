//! #9766 / RIPR-SPEC-0240: exact field observation must read the trait double.
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Subject(PathBuf);
impl Subject {
    fn new(source: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let root = parent.join(format!(
            "ripr-recording-double-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self::create_with(root, |root| {
            std::fs::create_dir(root.join("src")).map_err(|e| e.to_string())?;
            std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"recording-double\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n").map_err(|e| e.to_string())?;
            std::fs::write(root.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
            std::fs::write(root.join("edit.diff"), "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -13 +13 @@\n-    sink.send(format!(\"n={}\", n));\n+    sink.send(format!(\"n={n}\"));\n").map_err(|e| e.to_string())?;
            Ok(())
        })
    }
    fn create_with(
        root: PathBuf,
        setup: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<Self, String> {
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let subject = Self(root);
        setup(&subject.0)?;
        Ok(subject)
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn source(method: &str, param: &str, body: &str) -> String {
    format!(
        r#"pub trait Sink {{
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
"#
    )
}
const SETUP: &str = "let mut recorder = Recorder { lines: vec![], other: vec![] };\n        report(&mut recorder, 3);";
fn assert_reading(label: &str, source: &str, exposed: bool) -> Result<(), String> {
    assert_reading_with_probe(label, source, exposed, None)
}
fn assert_reading_with_probe(
    label: &str,
    source: &str,
    exposed: bool,
    after: Option<&str>,
) -> Result<(), String> {
    let subject = Subject::new(source)?;
    if let Some(after) = after {
        let path = subject.0.join("edit.diff");
        let diff = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let changed = diff.replace(
            "+    sink.send(format!(\"n={n}\"));",
            &format!("+    {after}"),
        );
        assert_ne!(
            diff, changed,
            "the alternate changed call must reach the diff producer"
        );
        std::fs::write(path, changed).map_err(|e| e.to_string())?;
    }
    let output = check_workspace(CheckInput {
        root: subject.0.clone(),
        diff_file: Some(subject.0.join("edit.diff")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let findings: Vec<_> = output
        .findings
        .iter()
        .filter(|f| f.probe.location.line == 13)
        .collect();
    assert!(
        !findings.is_empty(),
        "{label}: target call must produce findings"
    );
    assert!(
        findings.iter().any(|f| f
            .related_tests
            .iter()
            .any(|t| t.name.ends_with("exact_line"))),
        "{label}: the exact test must be related"
    );
    assert_eq!(
        findings.iter().any(|f| f.class == ExposureClass::Exposed),
        exposed,
        "{label}: {:?}",
        findings
            .iter()
            .map(|f| (
                &f.probe.family,
                &f.probe.expression,
                &f.class,
                &f.static_limit_kind
            ))
            .collect::<Vec<_>>()
    );
    if exposed {
        assert!(
            findings.iter().all(|f| f.class == ExposureClass::Exposed),
            "{label}: both call and side-effect readings must be exposed"
        );
        assert!(
            findings
                .iter()
                .filter(|f| f.class == ExposureClass::Exposed)
                .all(|f| f.static_limit_kind.is_none()),
            "{label}: credit cannot retain an unresolved limit"
        );
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
        assert_reading(
            assertion,
            &source(
                "self.lines.push(line);",
                "&mut dyn Sink",
                &format!("{SETUP}\n        {assertion}"),
            ),
            true,
        )?;
    }
    Ok(())
}
#[test]
fn missing_argument_transfer_is_never_credited() -> Result<(), String> {
    for (label, method, assertion) in [
        (
            "no-op",
            "let _ = line;",
            "assert_eq!(recorder.lines, Vec::<String>::new());",
        ),
        (
            "constant payload",
            "self.lines.push(\"fixed\".to_string());",
            "assert_eq!(recorder.lines, vec![\"fixed\".to_string()]);",
        ),
        (
            "unrelated field",
            "self.other.push(line);",
            "assert_eq!(recorder.lines, Vec::<String>::new());",
        ),
        (
            "conditional transfer",
            "if false { self.lines.push(line); }",
            "assert_eq!(recorder.lines, Vec::<String>::new());",
        ),
        (
            "local argument shadow",
            "let line = \"fixed\".to_string(); self.lines.push(line);",
            "assert_eq!(recorder.lines, vec![\"fixed\".to_string()]);",
        ),
    ] {
        assert_reading(
            label,
            &source(
                method,
                "&mut dyn Sink",
                &format!("{SETUP}\n        {assertion}"),
            ),
            false,
        )?;
    }
    let raw_receiver = source(
        "self.lines.push(line);",
        "&mut dyn Sink",
        &format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);"),
    ).replace("use super::*;", "use super::*; struct r#Recorder { lines: Vec<String>, other: Vec<String> } impl super::Sink for r#Recorder { fn send(&mut self, _: String) { self.lines.push(\"n=3\".to_string()); } }");
    assert_reading(
        "raw local double cannot borrow the root writer",
        &raw_receiver,
        false,
    )?;
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
    let original = source(
        "self.lines.push(line);",
        "&mut dyn Sink",
        &format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);"),
    );
    let from = original
        .find("pub struct Recorder")
        .ok_or("double declaration missing")?;
    let to = original
        .find("pub fn report")
        .ok_or("owner declaration missing")?;
    let declarations = &original[from..to];
    let moved = original
        .replacen(declarations, &"\n".repeat(declarations.lines().count()), 1)
        .replacen("    #[test]", &format!("{declarations}    #[test]"), 1);
    assert_reading("test-module double", &moved, true)
}

#[test]
fn user_read_methods_do_not_supply_recording_credit() -> Result<(), String> {
    let body =
        format!("{SETUP}\n        assert_eq!(recorder.lines.clone(), vec![\"n=3\".to_string()]);");
    let custom_clone = source("self.lines.push(line);", "&mut dyn Sink", &body)
        + "\ntrait Clone { fn clone(&self) -> Vec<String>; }\nimpl Clone for Vec<String> { fn clone(&self) -> Vec<String> { vec![\"n=3\".to_string()] } }\n";
    assert_reading("borrowed user clone (source-only)", &custom_clone, false)?;
    for name in ["clone", "r#clone"] {
        let owned_clone = source("self.lines.push(line);", "&mut dyn Sink", &body)
            + &format!(
                "\ntrait ForgedClone {{ fn {name}(self) -> Vec<String>; }}\nimpl ForgedClone for Vec<String> {{ fn {name}(self) -> Vec<String> {{ vec![\"n=3\".to_string()] }} }}\n"
            );
        assert_reading(&format!("owned user {name}"), &owned_clone, false)?;
    }
    Ok(())
}

#[test]
fn hidden_execution_and_module_value_twins_cannot_supply_credit() -> Result<(), String> {
    let base = source(
        "self.lines.push(line);",
        "&mut dyn Sink",
        &format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);"),
    );
    for (label, candidate) in [
        ("overloaded setup equality", base.replace("n: u32) {", "n: u32) { let skip = Stop == Stop;")
            + "\nstruct Stop; impl PartialEq for Stop { fn eq(&self, _: &Self) -> bool { std::process::exit(0); } }\n"),
        ("let-else exits before observation", base.replace("n: u32) {", "n: u32) { let N = 1 else { std::process::exit(0); };") + "\nconst N: u32 = 2;\n"),
        ("hidden format exit", base.replace("format!(\"n={}\", n)", "format!(\"n={}\", { std::process::exit(0); n })")),
        ("module const redirects owner", base.replace("use super::*;", "use super::*; const report: fn(&mut dyn Sink, u32) = rival; fn rival(s: &mut dyn Sink, _: u32) { s.send(\"n=3\".to_string()); }")),
        ("module static redirects owner", base.replace("use super::*;", "use super::*; static report: fn(&mut dyn Sink, u32) = rival; fn rival(s: &mut dyn Sink, _: u32) { s.send(\"n=3\".to_string()); }")),
        ("tuple constructor redirects owner", base.replace("use super::*;", "use super::*; struct report<'a>(&'a mut dyn Sink, u32); impl Drop for report<'_> { fn drop(&mut self) { self.0.send(\"n=3\".to_string()); } }")),
    ] {
        assert_reading(label, &candidate, false)?;
    }
    Ok(())
}

#[test]
fn primitive_slice_setup_and_later_append_keep_the_exact_pin() -> Result<(), String> {
    let body = "let mut recorder = Recorder { lines: vec![], other: vec![] };\n        report(&mut recorder, &[1, 2]);\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);";
    let candidate = source("self.lines.push(line);", "&mut dyn Sink", body)
        .replace(
            "n: u32) {",
            "items: &[u32]) { let n: u32 = items.iter().sum();",
        )
        .replace(
            "\n}\n#[cfg(test)]",
            "\n    if n > 100 { sink.send(\"over budget\".to_string()); }\n}\n#[cfg(test)]",
        );
    assert_reading("typed slice sum and later append", &candidate, true)
}

#[test]
fn a_scalar_shadow_drops_slice_provenance() -> Result<(), String> {
    let body = "let mut recorder = Recorder { lines: vec![], other: vec![] };\n        report(&mut recorder, &[1, 2]);\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);";
    for (label, parameter, binding, read) in [
        ("plain slice shadow", "items", "items", "items"),
        ("raw local aliases plain slice", "items", "r#items", "items"),
        (
            "plain local aliases raw slice",
            "r#items",
            "items",
            "r#items",
        ),
        ("Unicode NFC aliases", "café", "café", "café"),
    ] {
        let candidate = source("self.lines.push(line);", "&mut dyn Sink", body).replace(
            "n: u32) {",
            &format!(
                "{parameter}: &[u32]) {{ let {binding}: u32 = 3; let n: u32 = {read}.iter().sum();"
            ),
        ) + "\nstruct Stop; trait Iter { fn iter(&self) -> Stop; } impl Iter for u32 { fn iter(&self) -> Stop { std::process::exit(0); } } impl Stop { fn sum(self) -> u32 { 3 } }\n";
        assert_reading(label, &candidate, false)?;
    }
    Ok(())
}
#[test]
fn the_changed_call_cannot_hide_execution_in_format_arguments() -> Result<(), String> {
    let candidate = source(
        "self.lines.push(line);",
        "&mut dyn Sink",
        &format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);"),
    );
    assert_reading_with_probe(
        "hostile changed call with clean indexed source",
        &candidate,
        false,
        Some("sink.send(format!(\"n={}\", { std::process::exit(0); n }));"),
    )
}

#[test]
fn fixture_setup_owns_only_exclusively_acquired_roots() -> Result<(), String> {
    let parent = Subject::new("")?;
    let sentinel = parent.0.join("sentinel");
    std::fs::write(&sentinel, "preserve").map_err(|e| e.to_string())?;
    let collision = Subject::create_with(parent.0.clone(), |_| Ok(()));
    assert!(collision.is_err(), "an existing root must not be acquired");
    assert_eq!(
        std::fs::read_to_string(&sentinel).map_err(|e| e.to_string())?,
        "preserve"
    );
    let child = parent.0.join("partial");
    let failed = Subject::create_with(child.clone(), |root| {
        std::fs::write(root.join("partial-file"), "partial").map_err(|e| e.to_string())?;
        Err("injected setup failure".to_string())
    });
    assert!(failed.is_err());
    assert!(
        !child.exists(),
        "failed setup must remove the acquired child"
    );
    assert_eq!(
        std::fs::read_to_string(&sentinel).map_err(|e| e.to_string())?,
        "preserve"
    );
    Ok(())
}
#[test]
fn plain_test_cfg_uses_the_shared_predicate_authority() -> Result<(), String> {
    let candidate = source(
        "self.lines.push(line);",
        "&mut dyn Sink",
        &format!("{SETUP}\n        assert_eq!(recorder.lines, vec![\"n=3\".to_string()]);"),
    )
    .replace("#[cfg(test)]", "#[cfg( /* plain test gate */ test )]");
    assert_reading("plain test gate with trivia", &candidate, true)
}
