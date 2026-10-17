//! Public controls for caught assertion failures (#8508).

use super::{run_command, unique_temp_workspace, workspace_root};
use serde_json::Value;
use std::{fs, path::PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Subject(PathBuf);

impl Subject {
    fn new(body: &str) -> TestResult<Self> {
        let fixture = workspace_root().join("fixtures/typescript_finally_completion");
        let root = unique_temp_workspace("typescript-finally-completion");
        fs::create_dir_all(std::env::temp_dir())?;
        fs::create_dir(&root)?;
        let subject = Self(root);
        fs::create_dir_all(subject.0.join("src"))?;
        fs::create_dir_all(subject.0.join("tests"))?;
        for path in ["package.json", "ripr.toml", "src/load.ts"] {
            fs::copy(fixture.join("input").join(path), subject.0.join(path))?;
        }
        fs::copy(fixture.join("diff.patch"), subject.0.join("change.diff"))?;
        fs::write(
            subject.0.join("tests/load.test.ts"),
            format!(
                "import {{ expect, test }} from 'vitest';\nimport {{ load }} from '../src/load';\ntest('missing token', async () => {{\n{body}\n}});\n"
            ),
        )?;
        Ok(subject)
    }

    fn check(&self, class: &str, kind: &str, strength: &str) -> TestResult {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&self.0),
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "change.diff",
                "--mode",
                "fast",
                "--format",
                "json",
            ],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "nonempty intended subject: {report}");
        let finding = &findings[0];
        assert_eq!(finding["probe"]["file"], "src/load.ts");
        assert_eq!(finding["probe"]["line"], 3);
        assert_eq!(finding["probe"]["owner"], "typescript:src/load.ts::load");
        assert_eq!(finding["probe"]["family"], "error_path");
        assert_eq!(finding["classification"], class, "{finding}");
        assert_eq!(finding["related_tests_total"], 1);
        let related = finding["related_tests"]
            .as_array()
            .ok_or("missing related tests")?;
        assert_eq!(related.len(), 1);
        assert_eq!(related[0]["name"], "missing token");
        assert_eq!(related[0]["oracle_kind"], kind, "{finding}");
        assert_eq!(related[0]["oracle_strength"], strength, "{finding}");
        assert_eq!(finding["ripr"]["reach"]["state"], "yes");
        if kind == "unknown" {
            let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
            assert!(
                evidence
                    .iter()
                    .any(|line| line.as_str().is_some_and(|line| {
                        line.starts_with(
                            "test_assertion_admission: assertion_like_present_but_unresolved",
                        )
                    })),
                "withheld assertions must not become established absence: {finding}"
            );
        }
        Ok(())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const CHECK: &str = "await expect(load()).rejects.toBe('TOKEN_REQUIRED');";

#[test]
fn completion_replacing_finalizers_cannot_confirm_the_changed_throw() -> TestResult {
    for body in [
        format!("try {{ {CHECK} }} finally {{ return; }}"),
        format!("try {{ {CHECK} }} finally {{ if (true) {{ return; }} }}"),
        format!("try {{ {CHECK} }} finally {{ try {{}} finally {{ return; }} }}"),
        format!("for (;;) {{ try {{ {CHECK} }} finally {{ break; }} }}"),
        format!("for (let i = 0; i < 1; i++) {{ try {{ {CHECK} }} finally {{ continue; }} }}"),
        format!("try {{ {CHECK} }} finally {{ throw 'masked'; }}"),
        // The handler's assertion is replaced the same way.
        format!("try {{ throw 'setup'; }} catch {{ {CHECK} }} finally {{ return; }}"),
        // An enclosing replacing finalizer also withholds inner assertions.
        format!("try {{ try {{ {CHECK} }} finally {{ void 0; }} }} finally {{ return; }}"),
    ] {
        Subject::new(&body)?.check("weakly_exposed", "unknown", "unknown")?;
    }
    Ok(())
}

#[test]
fn benign_and_independent_finalizers_keep_credit() -> TestResult {
    for body in [
        format!("try {{ {CHECK} }} finally {{ void 0; }}"),
        // A return inside a nested function cannot replace the completion.
        format!("try {{ {CHECK} }} finally {{ const f = () => {{ return 1; }}; f(); }}"),
        // A break that stays inside the finalizer's own loop is local.
        format!("try {{ {CHECK} }} finally {{ for (;;) {{ break; }} }}"),
        // The finalizer's own awaited assertion is independent.
        format!("try {{ throw 'setup'; }} finally {{ {CHECK} }}"),
        format!("try {{ return; }} finally {{ {CHECK} }}"),
        format!("try {{ {CHECK} }} finally {{ return; }}\n{CHECK}"),
    ] {
        Subject::new(&body)?.check("exposed", "exact_error_variant", "strong")?;
    }
    Ok(())
}
