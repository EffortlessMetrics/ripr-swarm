//! RIPR-SPEC-0099 amendment: tsconfig `paths` aliases resolve by default, and
//! an explicit `resolve_tsconfig_paths = false` restores the fail-closed read.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DIFF: &str = "diff --git a/src/pricing.ts b/src/pricing.ts
--- a/src/pricing.ts
+++ b/src/pricing.ts
@@ -1,3 +1,3 @@
 export function discountRate(tier: string): number {
-  return tier === \"pro\" ? 0.1 : 0;
+  return tier === \"pro\" ? 0.2 : 0;
 }
";

struct Subject(PathBuf);

impl Subject {
    fn create(ripr_toml: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ts-alias-default-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("test")).map_err(|e| e.to_string())?;
        let files = [
            ("ripr.toml", ripr_toml),
            (
                "package.json",
                "{\"name\":\"alias-default\",\"devDependencies\":{\"vitest\":\"^2.0.0\"}}",
            ),
            (
                "tsconfig.json",
                "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"src/*\"]}}}",
            ),
            (
                "src/pricing.ts",
                "export function discountRate(tier: string): number {\n  return tier === \"pro\" ? 0.2 : 0;\n}\n",
            ),
            (
                "test/pricing.test.ts",
                "import { it, expect } from \"vitest\";\nimport { discountRate } from \"@/pricing\";\n\nit(\"pro discount\", () => {\n  expect(discountRate(\"pro\")).toBe(0.1);\n});\n",
            ),
        ];
        for (name, body) in files {
            std::fs::write(root.join(name), body).map_err(|e| e.to_string())?;
        }
        std::fs::write(root.join("change.diff"), DIFF).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn json(&self) -> Result<String, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--quiet", "--format", "json", "--root"])
            .arg(&self.0)
            .arg("--diff")
            .arg(self.0.join("change.diff"))
            .output()
            .map_err(|e| e.to_string())?;
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn aliased_test_is_related_without_any_typescript_config() -> Result<(), String> {
    let subject = Subject::create("[languages]\nenabled = [\"rust\", \"typescript\"]\n")?;
    let json = subject.json()?;
    if !json.contains("\"language\": \"typescript\"") {
        return Err(format!("typescript adapter must have run: {json}"));
    }
    if json.contains("\"classification\": \"no_static_path\"") {
        return Err(format!("aliased test must be credited by default: {json}"));
    }
    if !json.contains("test/pricing.test.ts") {
        return Err(format!("aliased test must be related by default: {json}"));
    }
    Ok(())
}

#[test]
fn explicit_false_leaves_aliased_test_unresolved() -> Result<(), String> {
    let subject = Subject::create(
        "[languages]\nenabled = [\"rust\", \"typescript\"]\n\n[typescript]\nresolve_tsconfig_paths = false\n",
    )?;
    let json = subject.json()?;
    if !json.contains("\"classification\": \"no_static_path\"") {
        return Err(format!("explicit false must stay unresolved: {json}"));
    }
    Ok(())
}
