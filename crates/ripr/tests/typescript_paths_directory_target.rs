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

const OWNER: &str = "packages/core/src/index.ts";

struct Repo {
    root: PathBuf,
    /// The file `discount` lives in.
    owner: &'static str,
}

impl Repo {
    /// A committed pnpm-style workspace whose only test of `discount` lives
    /// in `apps/web` and reaches it through the `paths` entry `target`.
    fn create(target: &str) -> Result<Self, String> {
        Self::create_shape(target, "", OWNER)
    }

    /// As [`Repo::create`], with extra `compilerOptions` members (each ending
    /// in a comma) and the owner at `owner`.
    fn create_shape(target: &str, options: &str, owner: &'static str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-paths-directory-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self { root, owner };
        repo.write(
            "tsconfig.json",
            &format!(
                "{{\"compilerOptions\":{{{options}\"baseUrl\":\".\",\"paths\":{{\"@acme/*\":[\"{target}\"]}}}}}}\n"
            ),
        )?;
        repo.write(
            "pnpm-workspace.yaml",
            "packages:\n  - \"packages/*\"\n  - \"apps/*\"\n",
        )?;
        repo.write("package.json", "{\"name\":\"mono\",\"private\":true}\n")?;
        repo.write(owner, OLD_OWNER)?;
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
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .current_dir(&self.root)
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
        self.write(self.owner, NEW_OWNER)?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--no-gpg-sign", "-m", "change"])?;
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.root)
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
        if finding["probe"]["file"] != self.owner {
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
        let _cleanup = std::fs::remove_dir_all(&self.root);
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

/// `bundler` performs directory lookup, as the default resolution does.
#[test]
fn bundler_resolution_loads_the_directory_index() -> Result<(), String> {
    let repo = Repo::create_shape("packages/*/src", "\"moduleResolution\":\"Bundler\",", OWNER)?;
    let (classification, tests) = repo.check_change()?;
    assert_eq!(tests, ["discount boundary"], "{classification}");
    Ok(())
}

/// Resolutions that skip directory lookup credit nothing: `nodenext` for an
/// ES-module importer, `classic` always, and `module: esnext` with no
/// `moduleResolution`, which selects `classic`.
#[test]
fn a_resolution_without_directory_lookup_stays_unresolved() -> Result<(), String> {
    for options in [
        "\"moduleResolution\":\"NodeNext\",",
        "\"module\":\"node16\",",
        "\"moduleResolution\":\"classic\",",
        "\"module\":\"esnext\",",
    ] {
        let (classification, tests) =
            Repo::create_shape("packages/*/src", options, OWNER)?.check_change()?;
        assert!(tests.is_empty(), "{options} {classification} {tests:?}");
    }
    Ok(())
}

/// A target written with a source extension names that file. When it is
/// absent, a directory of the same stem is not a substitute.
#[test]
fn a_target_with_an_extension_never_loads_a_directory_index() -> Result<(), String> {
    let (classification, tests) = Repo::create("packages/*/src.ts")?.check_change()?;
    assert!(tests.is_empty(), "{classification} {tests:?}");
    Ok(())
}

/// Directory lookup adds `.ts`, `.tsx`, `.js` and `.jsx`. An `index.mts` is
/// reachable by naming the file, not by naming its directory.
#[test]
fn a_directory_index_with_a_module_extension_stays_unresolved() -> Result<(), String> {
    let owner = "packages/core/src/index.mts";
    let (classification, tests) =
        Repo::create_shape("packages/*/src", "", owner)?.check_change()?;
    assert!(tests.is_empty(), "{classification} {tests:?}");
    let (classification, tests) =
        Repo::create_shape("packages/*/src/index.mts", "", owner)?.check_change()?;
    assert_eq!(tests, ["discount boundary"], "{classification}");
    Ok(())
}
