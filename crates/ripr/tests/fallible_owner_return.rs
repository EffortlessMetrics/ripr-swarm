//! Exact-source acceptance and refusal controls for SPEC-0197 rule 7a.

use ripr::{CheckInput, Mode, OutputFormat, ProbeFamily, check_workspace, render_check};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Subject(PathBuf);

impl Subject {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-fallible-owner-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|e| e.to_string())?;
        let fixture = fixture();
        for name in [
            "Cargo.toml",
            "Cargo.lock",
            "src/lib.rs",
            "src/test_support.rs",
        ] {
            std::fs::copy(fixture.join("input").join(name), path.join(name))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self(path))
    }

    fn support(&self, transform: impl FnOnce(String) -> String) -> Result<(), String> {
        let path = self.0.join("src/test_support.rs");
        let source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        std::fs::write(path, transform(source)).map_err(|e| e.to_string())
    }

    fn check(&self) -> Result<serde_json::Value, String> {
        let report = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(fixture().join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert!(!report.findings.is_empty(), "nonempty analyzed subjects");
        let answer = report
            .findings
            .iter()
            .filter(|f| f.probe.family == ProbeFamily::ReturnValue && f.probe.expression == "7")
            .collect::<Vec<_>>();
        assert_eq!(answer.len(), 1, "unique changed owner literal");
        serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
            .map_err(|e| e.to_string())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/owner_return_pin_fallible_literal")
}

fn answer(report: &serde_json::Value) -> Result<&serde_json::Value, String> {
    report["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|f| f["probe"]["family"] == "return_value" && f["probe"]["expression"] == "7")
        .ok_or_else(|| "missing changed answer".to_string())
}

#[test]
fn frozen_fallible_literal_helper_exposes_exact_owner_return() -> Result<(), String> {
    let subject = Subject::create()?;
    let report = subject.check()?;
    assert_eq!(report["findings"].as_array().ok_or("findings")?.len(), 1);
    let finding = answer(&report)?;
    assert_eq!(
        finding["classification"], "exposed",
        "prepare the pinned fixture first with the locked fetch documented in docs/TESTING.md"
    );
    assert_eq!(finding["ripr"]["propagate"]["state"], "yes");
    assert_eq!(finding["ripr"]["observe"]["state"], "yes");
    assert_eq!(finding["ripr"]["discriminate"]["state"], "yes");
    Ok(())
}

#[test]
fn fallible_literal_helper_refuses_unbound_or_ineffective_oracles() -> Result<(), String> {
    for (name, from, to) in [
        ("inverted", "actual == 7", "actual == 8"),
        ("malformed_literal", "actual == 7", "actual == 7 u8"),
        ("self_expected", "actual == 7", "actual == super::answer()"),
        ("missing", "    ensure!(actual == 7);\n", ""),
        (
            "message_escape",
            "actual == 7",
            "actual == 7, { return Ok(()); \"ignored\" }",
        ),
        (
            "rebound",
            "    ensure!(actual == 7);",
            "    let actual = 7;\n    ensure!(actual == 7);",
        ),
        (
            "closure",
            "    ensure!(actual == 7);",
            "    let _later = || { ensure!(actual == 7); Ok(()) };",
        ),
        (
            "swallowed",
            "    check_answer()\n",
            "    let _ = check_answer();\n    Ok(())\n",
        ),
        (
            "alias",
            "    let actual = super::answer();",
            "    use super::answer as read_answer;\n    let actual = read_answer();",
        ),
        (
            "shadowed_owner",
            "    let actual = super::answer();",
            "    let answer = || 7u8;\n    let actual = answer();",
        ),
        (
            "shadowed_macro",
            "use anyhow::{Result, ensure};",
            "use anyhow::Result;\nmacro_rules! ensure { ($($tokens:tt)*) => { () } }",
        ),
        (
            "custom_result",
            "use anyhow::{Result, ensure};",
            "use anyhow::ensure;\ntype Result<T> = core::result::Result<T, ()>;",
        ),
    ] {
        let subject = Subject::create()?;
        subject.support(|s| s.replace(from, to))?;
        let report = subject.check()?;
        let finding = answer(&report)?;
        assert_ne!(finding["classification"], "exposed", "{name}");
        // A refused receipt grants no observation credit. Propagation of the
        // constant tail `7` is established independently of any receipt
        // (#6617), so it is not a refusal signal.
        assert_ne!(finding["ripr"]["discriminate"]["state"], "yes", "{name}");
    }
    Ok(())
}

#[test]
fn warm_analysis_revalidates_config_lock_and_source_role() -> Result<(), String> {
    for mutation in ["replacement", "lock", "role", "inverted"] {
        let subject = Subject::create()?;
        let cold = subject.check()?;
        let warm = subject.check()?;
        assert_eq!(answer(&cold)?["classification"], "exposed");
        assert_eq!(cold, warm, "repeat stability: {mutation}");
        match mutation {
            "replacement" => {
                std::fs::create_dir_all(subject.0.join(".cargo")).map_err(|e| e.to_string())?;
                std::fs::write(
                    subject.0.join(".cargo/config.toml"),
                    "[source.crates-io]\nreplace-with='fake'\n[source.fake]\ndirectory='vendor'\n",
                )
                .map_err(|e| e.to_string())?;
            }
            "lock" => {
                let path = subject.0.join("Cargo.lock");
                let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                std::fs::write(
                    path,
                    content.replace(
                        "a23eb6b1614318a8071c9b2521f36b424b2c83db5eb3a0fead4a6c0809af6e61",
                        "invalid",
                    ),
                )
                .map_err(|e| e.to_string())?;
            }
            "role" => {
                let path = subject.0.join("src/lib.rs");
                let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                std::fs::write(path, content.replace("#[cfg(test)]\n", ""))
                    .map_err(|e| e.to_string())?;
            }
            "inverted" => subject.support(|s| s.replace("actual == 7", "actual == 8"))?,
            _ => return Err("unknown mutation control".into()),
        }
        let changed = subject.check()?;
        assert_ne!(answer(&changed)?["classification"], "exposed", "{mutation}");
        assert_ne!(
            answer(&changed)?["ripr"]["discriminate"]["state"],
            "yes",
            "{mutation}"
        );
    }
    Ok(())
}
