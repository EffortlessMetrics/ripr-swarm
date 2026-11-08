//! #13592 B3: a class imported through a barrel (`export * from "./u"`) is
//! related to its tests exactly like a direct import of the declaring module.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const DIFF: &str = "diff --git a/src/u.ts b/src/u.ts
--- a/src/u.ts
+++ b/src/u.ts
@@ -3,3 +3,3 @@
   get origin(): string {
-    return \"https://\" + this.host;
+    return \"http://\" + this.host;
   }
";

const SOURCE: &str = "export class Url {\n  constructor(public host: string) {}\n  get origin(): string {\n    return \"http://\" + this.host;\n  }\n}\n";

struct Subject(PathBuf);

impl Subject {
    fn create(test_import: &str, mock: &str) -> Result<Self, String> {
        Self::create_with(test_import, mock, SOURCE, "export * from \"./u\";\n")
    }

    fn create_with(
        test_import: &str,
        mock: &str,
        source: &str,
        barrel: &str,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ts-class-barrel-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("test")).map_err(|e| e.to_string())?;
        let test = format!(
            "import {{ it, expect, vi }} from \"vitest\";\nimport {{ Url }} from \"{test_import}\";\n{mock}\nit(\"origin\", () => {{\n  const url = new Url(\"host\");\n  expect(url.origin).toBe(\"https://host\");\n}});\n"
        );
        let files = [
            (
                "ripr.toml",
                "[languages]\nenabled = [\"rust\", \"typescript\"]\n".to_string(),
            ),
            (
                "package.json",
                "{\"name\":\"class-barrel\",\"devDependencies\":{\"vitest\":\"^2.0.0\"}}"
                    .to_string(),
            ),
            ("src/u.ts", source.to_string()),
            ("src/index.ts", barrel.to_string()),
            ("test/u.test.ts", test),
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

fn class_of(json: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    value["findings"][0]["classification"]
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| format!("no finding class in {json}"))
}

fn relation_of(json: &str) -> Result<(String, String), String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let related = &value["findings"][0]["related_tests"][0];
    match (
        related["relation_reason"].as_str(),
        related["relation_confidence"].as_str(),
    ) {
        (Some(reason), Some(confidence)) => Ok((reason.to_string(), confidence.to_string())),
        _ => Err(format!("no related-test relation in {json}")),
    }
}

#[test]
fn barrel_import_of_a_class_matches_the_direct_import() -> Result<(), String> {
    let direct_json = Subject::create("../src/u", "")?.json()?;
    let barrel_json = Subject::create("../src", "")?.json()?;
    let direct = class_of(&direct_json)?;
    let barrel = class_of(&barrel_json)?;
    // RIPR-SPEC-0095 disclosure: the barrel keeps its re-export provenance.
    let direct_relation = relation_of(&direct_json)?;
    if direct_relation != ("direct_owner_call".to_string(), "high".to_string()) {
        return Err(format!("direct import relation: {direct_relation:?}"));
    }
    let barrel_relation = relation_of(&barrel_json)?;
    if barrel_relation != ("re_export_chain_followed".to_string(), "medium".to_string()) {
        return Err(format!("barrel import relation: {barrel_relation:?}"));
    }
    if direct == "no_static_path" {
        return Err("the direct import must reach the getter".to_string());
    }
    if barrel != direct {
        return Err(format!("barrel {barrel} must match direct {direct}"));
    }
    Ok(())
}

#[test]
fn mocking_the_declaring_module_keeps_the_barrel_import_unrelated() -> Result<(), String> {
    let mocked = class_of(&Subject::create("../src", "vi.mock(\"../src/u\");")?.json()?)?;
    if mocked != "no_static_path" {
        return Err(format!("a mocked chain must stay unreached, got {mocked}"));
    }
    Ok(())
}

#[test]
fn mocking_the_barrel_keeps_the_class_unrelated() -> Result<(), String> {
    // Only the rewrite's own mock guard can withhold credit here: after a
    // rewrite the import names `../src/u`, which this test does not mock.
    let mocked = class_of(&Subject::create("../src", "vi.mock(\"../src\");")?.json()?)?;
    if mocked != "no_static_path" {
        return Err(format!("a mocked barrel must stay unreached, got {mocked}"));
    }
    Ok(())
}

#[test]
fn default_class_re_exported_by_name_matches_the_direct_import() -> Result<(), String> {
    let source = SOURCE.replace("export class Url", "export default class Url");
    let barrel = "export { default as Url } from \"./u\";\n";
    let barrel_class = class_of(&Subject::create_with("../src", "", &source, barrel)?.json()?)?;
    if barrel_class == "no_static_path" {
        return Err("a default class re-exported by name must reach the getter".to_string());
    }
    Ok(())
}
