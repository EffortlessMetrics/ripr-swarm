//! RIPR-SPEC-0082: with no `ripr.toml`, `ripr check` enables the TypeScript
//! preview adapter from a root `tsconfig.json` or detectable TypeScript source,
//! and does not enable it for `package.json` plus plain JavaScript alone.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn diff_for(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}
--- a/{path}
+++ b/{path}
@@ -1,3 +1,3 @@
 export function discountRate(tier) {{
-  return tier === \"pro\" ? 0.1 : 0;
+  return tier === \"pro\" ? 0.2 : 0;
 }}
"
    )
}

struct Subject(PathBuf);

impl Subject {
    fn create(files: &[(&str, &str)], changed: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ts-no-config-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        for (name, body) in files {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, body).map_err(|e| e.to_string())?;
        }
        std::fs::write(root.join("change.diff"), diff_for(changed)).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn ran_typescript_adapter(&self) -> Result<bool, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--quiet", "--format", "json", "--root"])
            .arg(&self.0)
            .arg("--diff")
            .arg(self.0.join("change.diff"))
            .output()
            .map_err(|e| e.to_string())?;
        let json = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
        // The unavailable-adapter advisory also names typescript, so key on `enabled`.
        Ok(json.contains("\"language\": \"typescript\"") && json.contains("\"enabled\": true"))
    }
}

impl Drop for Subject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const PRICING_JS: &str =
    "export function discountRate(tier) {\n  return tier === \"pro\" ? 0.2 : 0;\n}\n";
const PRICING_TS: &str = "export function discountRate(tier: string): number {\n  return tier === \"pro\" ? 0.2 : 0;\n}\n";
const PACKAGE: &str = "{\"name\":\"no-config\"}";

#[test]
fn root_tsconfig_enables_typescript_without_ripr_toml() -> Result<(), String> {
    let subject = Subject::create(
        &[
            ("package.json", PACKAGE),
            ("tsconfig.json", "{}"),
            ("src/pricing.js", PRICING_JS),
        ],
        "src/pricing.js",
    )?;
    if !subject.ran_typescript_adapter()? {
        return Err("root tsconfig.json must enable the typescript adapter".to_string());
    }
    Ok(())
}

#[test]
fn typescript_source_enables_typescript_without_ripr_toml() -> Result<(), String> {
    let subject = Subject::create(
        &[("package.json", PACKAGE), ("src/pricing.ts", PRICING_TS)],
        "src/pricing.ts",
    )?;
    if !subject.ran_typescript_adapter()? {
        return Err("detectable TypeScript source must enable the typescript adapter".to_string());
    }
    Ok(())
}

#[test]
fn package_json_with_plain_javascript_does_not_enable_typescript() -> Result<(), String> {
    let subject = Subject::create(
        &[("package.json", PACKAGE), ("src/pricing.js", PRICING_JS)],
        "src/pricing.js",
    )?;
    if subject.ran_typescript_adapter()? {
        return Err("package.json plus plain JavaScript must not enable typescript".to_string());
    }
    Ok(())
}
