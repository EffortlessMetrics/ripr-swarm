//! A Mocha project's TypeScript repair packet must name a verify command that
//! actually runs the test file: the package's local `mocha` binary, never
//! `npm test -- <file>` through npm's `npm init` placeholder `scripts.test`
//! (`echo "Error: no test specified" && exit 1`).

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PLACEHOLDER_SCRIPT: &str = r#""test": "echo \"Error: no test specified\" && exit 1""#;
const AGE_JS: &str = "export function isAdult(age) {\n  return age > 18;\n}\n";
const AGE_SPEC: &str = "import { expect } from 'chai';\nimport { isAdult } from '../src/age.js';\ndescribe('age', () => {\n  it('adult', () => { expect(isAdult(30)).to.equal(true); });\n  it('child', () => { expect(isAdult(5)).to.equal(false); });\n});\n";
const DIFF: &str = "diff --git a/src/age.js b/src/age.js
--- a/src/age.js
+++ b/src/age.js
@@ -1,3 +1,3 @@
 export function isAdult(age) {
-  return age >= 18;
+  return age > 18;
 }
";

struct Subject(PathBuf);

impl Subject {
    fn create(package_json: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ts-mocha-verify-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("test")).map_err(|e| e.to_string())?;
        for (name, body) in [
            ("package.json", package_json),
            ("package-lock.json", "{}"),
            ("src/age.js", AGE_JS),
            ("test/age.spec.js", AGE_SPEC),
            ("change.diff", DIFF),
        ] {
            std::fs::write(root.join(name), body).map_err(|e| e.to_string())?;
        }
        Ok(Self(root))
    }

    fn check_json(&self) -> Result<String, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--quiet", "--format", "json", "--root"])
            .arg(&self.0)
            .arg("--diff")
            .arg(self.0.join("change.diff"))
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "ripr check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn manifest(dev_dependencies: &str) -> String {
    format!(
        "{{\"name\":\"s\",\"type\":\"module\",\"scripts\":{{{PLACEHOLDER_SCRIPT}}},\"devDependencies\":{{{dev_dependencies}}}}}"
    )
}

#[test]
fn mocha_project_verify_command_launches_the_local_mocha_binary() -> Result<(), String> {
    let subject = Subject::create(&manifest("\"mocha\":\"^11.0.0\",\"chai\":\"^5.0.0\""))?;
    let json = subject.check_json()?;
    if !json.contains("npx --no-install mocha test/age.spec.js") {
        return Err(format!("expected a local mocha launch in: {json}"));
    }
    if json.contains("npm test") {
        return Err(format!("must not route through `npm test`: {json}"));
    }
    Ok(())
}

#[test]
fn placeholder_test_script_without_a_framework_emits_no_npm_test_command() -> Result<(), String> {
    let subject = Subject::create(&manifest("\"chai\":\"^5.0.0\""))?;
    let json = subject.check_json()?;
    if json.contains("npm test") {
        return Err(format!(
            "placeholder script must not be run via npm: {json}"
        ));
    }
    Ok(())
}
