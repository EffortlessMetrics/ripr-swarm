//! RIPR-SPEC-0099 amendment: tsconfig `paths` aliases resolve by default, and
//! an explicit `resolve_tsconfig_paths = false` restores the fail-closed read.

#![cfg(feature = "lang-typescript")]

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
        Self::create_with(
            ripr_toml,
            "tsconfig.json",
            "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"src/*\"]}}}",
        )
    }

    /// The same workspace with `config_body` as its root `config_name`.
    fn create_with(ripr_toml: &str, config_name: &str, config_body: &str) -> Result<Self, String> {
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
            (config_name, config_body),
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

    fn write(&self, name: &str, body: &str) -> Result<(), String> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, body).map_err(|e| e.to_string())
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
    if !json.contains("\"classification\": \"exposed\"") {
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
    if !json.contains("typescript_path_alias_unresolved") {
        return Err(format!("unresolved alias must stay disclosed: {json}"));
    }
    Ok(())
}

const TYPESCRIPT_ON: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

fn expect_related(json: &str, label: &str) -> Result<(), String> {
    if !json.contains("\"classification\": \"exposed\"") || !json.contains("test/pricing.test.ts") {
        return Err(format!("{label}: aliased test must be related: {json}"));
    }
    Ok(())
}

fn expect_unresolved(json: &str, label: &str) -> Result<(), String> {
    if !json.contains("\"classification\": \"no_static_path\"")
        || !json.contains("typescript_path_alias_unresolved")
    {
        return Err(format!(
            "{label}: alias must stay unresolved and disclosed: {json}"
        ));
    }
    Ok(())
}

/// #12760: `paths` without `baseUrl` (the create-next-app default) resolves
/// against the config that declares it, for `tsconfig.json`, the
/// `jsconfig.json` fallback, and a same-directory `extends` base.
#[test]
fn relative_paths_without_base_url_relate_the_aliased_test() -> Result<(), String> {
    const PATHS_ONLY: &str = "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}";
    for config in ["tsconfig.json", "jsconfig.json"] {
        let subject = Subject::create_with(TYPESCRIPT_ON, config, PATHS_ONLY)?;
        expect_related(&subject.json()?, config)?;
    }
    let subject = Subject::create_with(
        TYPESCRIPT_ON,
        "tsconfig.json",
        "{\"extends\":\"./tsconfig.base.json\"}",
    )?;
    subject.write("tsconfig.base.json", PATHS_ONLY)?;
    expect_related(&subject.json()?, "inherited paths")
}

/// The alias names a same-named module elsewhere: the changed owner in
/// `src/` is not what the test imports, so it must not be credited.
#[test]
fn paths_without_base_url_do_not_credit_a_same_named_decoy() -> Result<(), String> {
    let subject = Subject::create_with(
        TYPESCRIPT_ON,
        "tsconfig.json",
        "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./other/*\"]}}}",
    )?;
    subject.write(
        "other/pricing.ts",
        "export function discountRate(tier: string): number {\n  return tier === \"pro\" ? 0.1 : 0;\n}\n",
    )?;
    let json = subject.json()?;
    if !json.contains("\"classification\": \"no_static_path\"") {
        return Err(format!("the decoy import must not credit src/: {json}"));
    }
    Ok(())
}

/// `tsc` rejects a non-relative target when no `baseUrl` is set (TS5090).
/// The key still owns the import, so it stays unresolved and disclosed
/// instead of being guessed against the config directory.
#[test]
fn non_relative_target_without_base_url_stays_unresolved() -> Result<(), String> {
    let subject = Subject::create_with(
        TYPESCRIPT_ON,
        "tsconfig.json",
        "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"src/*\"]}}}",
    )?;
    let json = subject.json()?;
    expect_unresolved(&json, "non-relative target")?;
    if json.contains("add compilerOptions.baseUrl") {
        return Err(format!("advice must not ask for a baseUrl: {json}"));
    }
    if !json.contains("start it with `./`") {
        return Err(format!("advice must name the relative-target fix: {json}"));
    }
    Ok(())
}

/// `paths` fallback arrays follow `tsc`'s order: `@/pricing` names the first
/// target that exists, and a later target is used only when nothing sits at
/// the earlier one.
#[test]
fn fallback_array_relates_the_test_only_through_the_first_existing_target() -> Result<(), String> {
    const FALLBACK: &str =
        "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"generated/*\",\"src/*\"]}}}";
    let subject = Subject::create_with(TYPESCRIPT_ON, "tsconfig.json", FALLBACK)?;
    expect_related(&subject.json()?, "only the second target exists")?;

    // The first target now exists, so the test imports that same-named
    // module and the changed `src/pricing.ts` is not what it exercises.
    subject.write(
        "generated/pricing.ts",
        "export function discountRate(tier: string): number {\n  return tier === \"pro\" ? 0.1 : 0;\n}\n",
    )?;
    let json = subject.json()?;
    if !json.contains("\"classification\": \"no_static_path\"") {
        return Err(format!("an existing first target must win: {json}"));
    }
    Ok(())
}

/// A declaration file at the first target is something `tsc` resolves and
/// this reader does not model: the walk stops, unresolved and disclosed,
/// instead of crediting the later target.
#[test]
fn fallback_array_stops_at_a_first_target_it_cannot_rule_out() -> Result<(), String> {
    let subject = Subject::create_with(
        TYPESCRIPT_ON,
        "tsconfig.json",
        "{\"compilerOptions\":{\"baseUrl\":\".\",\"paths\":{\"@/*\":[\"generated/*\",\"src/*\"]}}}",
    )?;
    subject.write(
        "generated/pricing.d.ts",
        "export declare function discountRate(tier: string): number;\n",
    )?;
    expect_unresolved(&subject.json()?, "declaration file at the first target")
}

const SOLUTION_ROOT: &str = "{\"files\":[],\"references\":[{\"path\":\"./tsconfig.app.json\"},{\"path\":\"./tsconfig.node.json\"}]}";

/// The Vite template layout: a solution-style root whose `paths` live in the
/// `tsconfig.app.json` it references.
#[test]
fn solution_style_root_relates_the_test_through_its_referenced_project() -> Result<(), String> {
    let subject = Subject::create_with(TYPESCRIPT_ON, "tsconfig.json", SOLUTION_ROOT)?;
    subject.write(
        "tsconfig.app.json",
        "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}},\"include\":[\"src\"]}",
    )?;
    subject.write(
        "tsconfig.node.json",
        "{\"compilerOptions\":{\"strict\":true},\"include\":[\"vite.config.ts\"]}",
    )?;
    expect_related(&subject.json()?, "referenced sibling project")
}

/// Two referenced projects map the same alias to different files. Which one
/// compiles the test is not modeled, so neither is credited.
#[test]
fn referenced_projects_that_disagree_leave_the_alias_unresolved() -> Result<(), String> {
    let subject = Subject::create_with(TYPESCRIPT_ON, "tsconfig.json", SOLUTION_ROOT)?;
    subject.write(
        "tsconfig.app.json",
        "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}",
    )?;
    subject.write(
        "tsconfig.node.json",
        "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./other/*\"]}}}",
    )?;
    subject.write(
        "other/pricing.ts",
        "export function discountRate(tier: string): number {\n  return tier === \"pro\" ? 0.1 : 0;\n}\n",
    )?;
    expect_unresolved(&subject.json()?, "disagreeing referenced projects")
}

/// A reference this reader cannot open is a project it cannot rule out.
#[test]
fn unreadable_reference_leaves_the_alias_unresolved() -> Result<(), String> {
    let subject = Subject::create_with(TYPESCRIPT_ON, "tsconfig.json", SOLUTION_ROOT)?;
    subject.write(
        "tsconfig.app.json",
        "{\"compilerOptions\":{\"paths\":{\"@/*\":[\"./src/*\"]}}}",
    )?;
    expect_unresolved(&subject.json()?, "missing tsconfig.node.json")
}
