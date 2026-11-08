//! RIPR-SPEC-0099 (#12754): a `paths` alias resolves under the importing
//! test's own package config, never the workspace-root map, and stays
//! unresolved when the package config cannot be modeled exactly.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const BODY: &str = "export function discountRate(tier: string): number {\n  return tier === \"pro\" ? 0.2 : 0;\n}\n";
const TEST: &str = "import { it, expect } from \"vitest\";\nimport { discountRate } from \"@/pricing\";\n\nit(\"pro discount\", () => {\n  expect(discountRate(\"pro\")).toBe(0.1);\n});\n";

fn diff_for(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n export function discountRate(tier: string): number {{\n-  return tier === \"pro\" ? 0.1 : 0;\n+  return tier === \"pro\" ? 0.2 : 0;\n }}\n"
    )
}

struct Subject(PathBuf);

impl Subject {
    fn create(root_config: Option<&str>, package_config: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ts-monorepo-alias-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut files = vec![
            (
                "ripr.toml",
                "[languages]\nenabled = [\"rust\", \"typescript\"]\n",
            ),
            (
                "package.json",
                "{\"name\":\"monorepo\",\"devDependencies\":{\"vitest\":\"^2.0.0\"}}",
            ),
            ("packages/a/tsconfig.json", package_config),
            ("packages/a/src/pricing.ts", BODY),
            ("packages/a/test/pricing.test.ts", TEST),
            // Same-named decoy that only the root map would reach.
            ("shared/pricing.ts", BODY),
        ];
        if let Some(config) = root_config {
            files.push(("tsconfig.json", config));
        }
        for (name, body) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, body).map_err(|e| e.to_string())?;
        }
        Ok(Self(root))
    }

    fn json(&self, changed: &str) -> Result<String, String> {
        std::fs::write(self.0.join("change.diff"), diff_for(changed)).map_err(|e| e.to_string())?;
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

const ROOT: &str = "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"shared/*\"]}}}";
const PACKAGE: &str = "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"src/*\"]}}}";

#[test]
fn package_config_credits_the_package_owner_over_the_root_decoy() -> Result<(), String> {
    let subject = Subject::create(Some(ROOT), PACKAGE)?;
    let json = subject.json("packages/a/src/pricing.ts")?;
    if !json.contains("\"classification\": \"exposed\"")
        || !json.contains("packages/a/test/pricing.test.ts")
    {
        return Err(format!("package test must reach its own owner: {json}"));
    }
    // The root decoy is not reached by package a's test.
    let decoy = subject.json("shared/pricing.ts")?;
    if decoy.contains("\"classification\": \"exposed\"") {
        return Err(format!("root decoy must not be credited: {decoy}"));
    }
    Ok(())
}

#[test]
fn package_only_config_resolves_without_a_root_tsconfig() -> Result<(), String> {
    let subject = Subject::create(None, PACKAGE)?;
    let json = subject.json("packages/a/src/pricing.ts")?;
    if !json.contains("\"classification\": \"exposed\"")
        || !json.contains("packages/a/test/pricing.test.ts")
    {
        return Err(format!("package-only config must resolve: {json}"));
    }
    Ok(())
}

#[test]
fn unmodelable_package_config_stays_unresolved() -> Result<(), String> {
    let subject = Subject::create(
        Some(ROOT),
        "{\"extends\":\"../tsconfig.base.json\",\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"src/*\"]}}}",
    )?;
    let json = subject.json("packages/a/src/pricing.ts")?;
    if json.contains("\"classification\": \"exposed\"") {
        return Err(format!("cross-directory extends must not credit: {json}"));
    }
    if !json.contains("typescript_path_alias_unresolved") {
        return Err(format!("the miss must stay disclosed: {json}"));
    }
    Ok(())
}
