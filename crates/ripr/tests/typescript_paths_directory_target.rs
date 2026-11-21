//! #15914: a tsconfig `paths` target that names a directory.
//!
//! The monorepo idiom `"@acme/*": ["packages/*/src"]` maps a package name to
//! its source directory, and the compiler loads that directory's `index`
//! file. `ripr check` left the import unresolved, so a boundary test in a
//! sibling package was dropped and the finding named a missing check that
//! the test already made.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const OLD_OWNER: &str = "export function discount(total: number): number {\n  if (total > 100) {\n    return total - 10;\n  }\n  return total;\n}\n";
const NEW_OWNER: &str = "export function discount(total: number): number {\n  if (total >= 100) {\n    return total - 10;\n  }\n  return total;\n}\n";
/// A sibling package's test, importing the owner by its mapped name.
const SIBLING_TEST: &str = "import { it, expect } from \"vitest\";\nimport { discount } from \"@acme/core\";\nit(\"discount boundary\", () => {\n  expect(discount(100)).toBe(100);\n  expect(discount(101)).toBe(91);\n});\n";

struct Repo(PathBuf);

impl Repo {
    /// A committed pnpm-style workspace whose only test of `discount` lives
    /// in `apps/web` and reaches it through the `paths` entry `target`.
    fn create(target: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-paths-directory-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(
            "tsconfig.json",
            &format!(
                "{{\"compilerOptions\":{{\"baseUrl\":\".\",\"paths\":{{\"@acme/*\":[\"{target}\"]}}}}}}\n"
            ),
        )?;
        repo.write(
            "pnpm-workspace.yaml",
            "packages:\n  - \"packages/*\"\n  - \"apps/*\"\n",
        )?;
        repo.write("package.json", "{\"name\":\"mono\",\"private\":true}\n")?;
        repo.write("packages/core/src/index.ts", OLD_OWNER)?;
        repo.write("apps/web/package.json", "{\"name\":\"web\"}\n")?;
        repo.write("apps/web/e2e/core.test.ts", SIBLING_TEST)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Commits the boundary change and returns the `discount` finding's
    /// classification and related test names from `check --json`.
    fn check_change(&self) -> Result<(String, Vec<String>), String> {
        self.write("packages/core/src/index.ts", NEW_OWNER)?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--no-gpg-sign", "-m", "change"])?;
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .env("RIPR_QUIET", "1")
            .args(["check", "--base", "HEAD~1", "--json"])
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.code() != Some(0) {
            return Err(format!(
                "check must complete: {:?}\n{stdout}\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let report: serde_json::Value =
            serde_json::from_str(&stdout).map_err(|e| format!("{e}\n{stdout}"))?;
        let findings = report["findings"]
            .as_array()
            .ok_or_else(|| format!("no findings array:\n{stdout}"))?;
        let [finding] = findings.as_slice() else {
            return Err(format!("one predicate finding expected:\n{stdout}"));
        };
        if finding["probe"]["file"] != "packages/core/src/index.ts" {
            return Err(format!("the finding must name the owner file\n{stdout}"));
        }
        let tests = finding["related_tests"]
            .as_array()
            .map(|tests| {
                tests
                    .iter()
                    .filter_map(|test| test["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let classification = finding["classification"]
            .as_str()
            .ok_or("classification")?
            .to_string();
        Ok((classification, tests))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_directory_target_relates_the_sibling_package_test() -> Result<(), String> {
    let (classification, tests) = Repo::create("packages/*/src")?.check_change()?;
    assert_eq!(tests, ["discount boundary"], "{classification}");
    assert_eq!(classification, "exposed");
    Ok(())
}

/// The spelling that already resolved gives the same answer, so the
/// directory form is the only thing the first test adds.
#[test]
fn the_explicit_index_target_gives_the_same_answer() -> Result<(), String> {
    let (classification, tests) = Repo::create("packages/*/src/index.ts")?.check_change()?;
    assert_eq!(tests, ["discount boundary"], "{classification}");
    assert_eq!(classification, "exposed");
    Ok(())
}

/// A directory with its own `package.json` is a package: the compiler reads
/// its entry fields, which this resolver does not, so the import stays
/// unresolved and no test is credited.
#[test]
fn a_directory_with_a_manifest_stays_unresolved() -> Result<(), String> {
    let repo = Repo::create("packages/*/src")?;
    repo.write(
        "packages/core/src/package.json",
        "{\"main\":\"other.ts\"}\n",
    )?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "manifest"])?;
    let (classification, tests) = repo.check_change()?;
    assert!(tests.is_empty(), "{classification} {tests:?}");
    assert_ne!(classification, "exposed");
    Ok(())
}
