//! RIPR-SPEC-0228: same-instance mutation observations carry field identity.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

const COUNTER: &str = "#[derive(Default)]\npub struct Counter { pub count: u32, pub other: u32 }\nimpl Counter {\n    pub fn bump(&mut self) {\n        self.count += 2 - 1;\n    }\n    pub fn count(&self) -> u32 { self.count }\n}\n";

fn exposed(
    name: &str,
    source: &str,
    body: &str,
    before: &str,
    after: &str,
) -> Result<bool, String> {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("field_mutation_identity")
        .join(name);
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"mutation_subject\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n").map_err(|error| error.to_string())?;
    let source = format!(
        "{source}\n#[cfg(test)]\nmod tests {{\nuse super::*;\n#[test]\nfn observes() {{\n{body}\n}}\n}}\n"
    );
    std::fs::write(root.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
    let line = source
        .lines()
        .position(|line| line.trim() == after)
        .ok_or("missing changed line")?
        + 1;
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-        {before}\n+        {after}\n"
    );
    std::fs::write(root.join("diff.patch"), diff).map_err(|error| error.to_string())?;
    let report = check_workspace(CheckInput {
        root,
        diff_file: Some(
            PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
                .join("field_mutation_identity")
                .join(name)
                .join("diff.patch"),
        ),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    if report.findings.len() != 1 {
        return Err(format!(
            "{name}: expected one finding, got {:?}",
            report.findings
        ));
    }
    Ok(report.findings[0].class.as_str() == "exposed")
}

#[test]
fn direct_getter_and_repeated_mutations_have_identity() -> Result<(), String> {
    for (name, body) in [
        (
            "direct",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count, 1);",
        ),
        (
            "getter",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count(), 1);",
        ),
        (
            "repeated",
            "let mut c = Counter::default();\nc.bump();\nc.bump();\nassert_eq!(c.count, 2);",
        ),
    ] {
        assert!(
            exposed(
                name,
                COUNTER,
                body,
                "self.count += 1;",
                "self.count += 2 - 1;"
            )?,
            "{name}: mutation identity missing"
        );
    }
    Ok(())
}

#[test]
fn mutation_identity_preserves_receiver_and_value_refusals() -> Result<(), String> {
    for (name, body) in [
        (
            "wrong_receiver",
            "let mut c = Counter::default();\nlet d = Counter::default();\nc.bump();\nassert_eq!(d.count, 1);",
        ),
        (
            "sibling",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.other, 1);",
        ),
        (
            "reset",
            "let mut c = Counter::default();\nc.bump();\nc.count = 1;\nassert_eq!(c.count, 1);",
        ),
        (
            "alias",
            "let mut c = Counter::default();\nc.bump();\nlet d = c;\nassert_eq!(d.count, 1);",
        ),
        (
            "snapshot_expected",
            "let mut c = Counter::default();\nc.bump();\nlet want = c.count;\nassert_eq!(c.count, want);",
        ),
        (
            "zero",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count, 0);",
        ),
        (
            "wrong_key",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!([c.count, 0][1], 1);",
        ),
    ] {
        assert!(
            !exposed(
                name,
                COUNTER,
                body,
                "self.count += 1;",
                "self.count += 2 - 1;"
            )?,
            "{name}: false identity"
        );
    }
    Ok(())
}

#[test]
fn literal_arguments_and_enum_variants_have_identity() -> Result<(), String> {
    let account = COUNTER
        .replace("fn bump(&mut self)", "fn bump(&mut self, amount: u32)")
        .replace("self.count += 2 - 1;", "self.count = self.count + amount;");
    assert!(exposed(
        "arguments",
        &account,
        "let mut c = Counter::default();\nc.bump(100);\nc.bump(50);\nassert_eq!(c.count(), 150);",
        "self.count += amount;",
        "self.count = self.count + amount;"
    )?);
    let job = "#[derive(Debug, PartialEq, Eq)]\npub enum State { Idle, Done }\npub struct Job { pub state: State }\nimpl Job {\npub fn new() -> Self { Self { state: State::Idle } }\npub fn finish(&mut self) {\n        self.state = (State::Done);\n}\n}\n";
    for (name, body, credited) in [
        (
            "enum_exact",
            "let mut c = Job::new();\nc.finish();\nassert_eq!(c.state, State::Done);",
            true,
        ),
        (
            "enum_wrong_variant",
            "let mut c = Job::new();\nc.finish();\nassert_eq!(c.state, State::Idle);",
            false,
        ),
        (
            "enum_local_alias",
            "type State = crate::State;\nlet mut c = Job::new();\nc.finish();\nassert_eq!(c.state, State::Done);",
            false,
        ),
    ] {
        assert_eq!(
            exposed(
                name,
                job,
                body,
                "self.state = State::Done;",
                "self.state = (State::Done);"
            )?,
            credited,
            "{name}"
        );
    }
    let custom = job.replace("Debug, PartialEq, Eq", "Debug")
        + "impl PartialEq for State { fn eq(&self, _: &Self) -> bool { true } }\n";
    assert!(!exposed(
        "enum_custom_eq",
        &custom,
        "let mut c = Job::new();\nc.finish();\nassert_eq!(c.state, State::Done);",
        "self.state = State::Done;",
        "self.state = (State::Done);"
    )?);
    Ok(())
}

#[test]
fn dispatch_drop_and_source_changes_invalidate_identity() -> Result<(), String> {
    let body = "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count(), 1);";
    for (name, source) in [
        (
            "by_value",
            COUNTER.replace("fn bump(&mut self)", "fn bump(mut self)"),
        ),
        (
            "overwrite_owner",
            COUNTER.replace(
                "self.count += 2 - 1;",
                "self.count += 2 - 1;\n        self.count = 1;",
            ),
        ),
        (
            "drop",
            format!(
                "{COUNTER}\nimpl Drop for Counter {{ fn drop(&mut self) {{ std::process::exit(0); }} }}\n"
            ),
        ),
        (
            "trait_getter",
            format!(
                "{COUNTER}\ntrait Observe {{ fn count(self) -> u32; }}\nimpl Observe for Counter {{ fn count(self) -> u32 {{ 1 }} }}\n"
            ),
        ),
        (
            "generic_method",
            COUNTER.replace("fn bump(&mut self)", "fn bump<T>(&mut self)"),
        ),
        (
            "opaque_expansion",
            format!("{COUNTER}\nunknown_items!();\n"),
        ),
    ] {
        assert!(
            !exposed(
                name,
                &source,
                body,
                "self.count += 1;",
                "self.count += 2 - 1;"
            )?,
            "{name}: false identity"
        );
    }
    // Reuse the same root and unchanged test text while an observed dependency
    // changes, then restore it. An earlier affirmative answer cannot survive.
    for (source, credited) in [
        (COUNTER.to_string(), true),
        (COUNTER.replace("{ self.count }", "{ self.other }"), false),
        (COUNTER.to_string(), true),
    ] {
        assert_eq!(
            exposed(
                "getter_dependency",
                &source,
                body,
                "self.count += 1;",
                "self.count += 2 - 1;"
            )?,
            credited
        );
    }
    Ok(())
}

#[test]
fn discarded_async_mutation_and_malformed_integer_calls_are_refused() -> Result<(), String> {
    let asynchronous = COUNTER.replace("pub fn bump(", "pub async fn bump(");
    assert!(
        !exposed(
            "discarded_async",
            &asynchronous,
            "let mut c = Counter { count: 1, other: 0 };\nc.bump();\nassert_eq!(c.count, 1);",
            "self.count += 1;",
            "self.count += 2 - 1;",
        )?,
        "a discarded future never runs the field write"
    );
    for (name, body) in [
        (
            "negative_unsigned_expected",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count, -1);",
        ),
        (
            "wrong_expected_suffix",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count, 1i32);",
        ),
        (
            "out_of_range_expected",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count, 4294967296u32);",
        ),
        (
            "extra_call_argument",
            "let mut c = Counter::default();\nc.bump(1);\nassert_eq!(c.count, 1);",
        ),
    ] {
        assert!(
            !exposed(
                name,
                COUNTER,
                body,
                "self.count += 1;",
                "self.count += 2 - 1;"
            )?,
            "{name}"
        );
    }
    let parameter = COUNTER.replace("fn bump(&mut self)", "fn bump(&mut self, amount: u32)");
    for (name, body) in [
        (
            "missing_call_argument",
            "let mut c = Counter::default();\nc.bump();\nassert_eq!(c.count, 1);",
        ),
        (
            "negative_unsigned_argument",
            "let mut c = Counter::default();\nc.bump(-1);\nassert_eq!(c.count, 1);",
        ),
        (
            "wrong_argument_suffix",
            "let mut c = Counter::default();\nc.bump(1i32);\nassert_eq!(c.count, 1);",
        ),
        (
            "out_of_range_argument",
            "let mut c = Counter::default();\nc.bump(4294967296u32);\nassert_eq!(c.count, 1);",
        ),
    ] {
        assert!(
            !exposed(
                name,
                &parameter,
                body,
                "self.count += 1;",
                "self.count += 2 - 1;"
            )?,
            "{name}"
        );
    }
    Ok(())
}
