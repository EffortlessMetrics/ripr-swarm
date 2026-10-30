//! #10711: the layout of a function body must not change repair readiness.
//!
//! A predicate in a single-line body (`pub fn f(x: u32) -> bool { x >= 18 }`,
//! what an agent writes before running rustfmt, and what `fn_single_line`
//! projects keep) must reach the same repair start, with the same missing
//! boundary fact, as the identical logic in the rustfmt layout.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"lp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

struct Repo(PathBuf);

impl Repo {
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-layout-parity-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        std::fs::create_dir_all(repo.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(repo.0.join("tests")).map_err(|e| e.to_string())?;
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", old_src)?;
        repo.write("tests/t.rs", test)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])?;
        repo.write("src/lib.rs", new_src)?;
        repo.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(name), text).map_err(|e| e.to_string())
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

    /// Run `ripr` and require `expected_code`, so a crashing or refusing CLI
    /// cannot pass the parity comparison on its error text alone.
    fn ripr(&self, args: &[&str], expected_code: i32) -> Result<String, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.code() != Some(expected_code) {
            return Err(format!(
                "ripr {args:?} exited {:?}, expected {expected_code}:\n{}\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The layout-independent facts of one `ripr repair` run: the readiness line,
/// the missing discriminator and the changed behavior. Seam ids and line
/// numbers legitimately differ between layouts and are left out.
fn repair_facts(repo: &Repo) -> Result<Vec<String>, String> {
    let repair = repo.ripr(&["repair", "--root", "."], 0)?;
    let facts: Vec<String> = repair
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("readiness:")
                || line.starts_with("missing discriminator:")
                || line.starts_with("changed behavior:")
        })
        .map(str::to_string)
        .collect();
    assert!(
        facts.iter().any(|l| l.contains("repair_ready=true")),
        "precondition, repair starts for this layout:\n{repair}"
    );
    Ok(facts)
}

/// The missing discriminator values `check` reports, layout-independent.
fn check_verdict(repo: &Repo) -> Result<Vec<String>, String> {
    let check = repo.ripr(&["check", "--base", "HEAD~1"], 0)?;
    let missing: Vec<String> = check
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("Missing check") || line.starts_with("Static exposure:"))
        .map(str::to_string)
        .collect();
    assert!(
        !missing.is_empty(),
        "precondition, check prints a verdict and missing check:\n{check}"
    );
    Ok(missing)
}

struct Shape {
    name: &'static str,
    signature: &'static str,
    before: &'static str,
    after: &'static str,
    test: &'static str,
}

const SHAPES: &[Shape] = &[
    Shape {
        name: "parameter against a literal",
        signature: "pub fn is_adult(age: u32) -> bool",
        before: "age >= 18",
        after: "age > 18",
        test: "use lp::is_adult;\n\n#[test]\nfn around_the_boundary() {\n    assert_eq!(is_adult(17), false);\n    assert_eq!(is_adult(19), true);\n}\n",
    },
    Shape {
        name: "two parameters",
        signature: "pub fn at_least(a: u32, b: u32) -> bool",
        before: "a >= b",
        after: "a > b",
        test: "use lp::at_least;\n\n#[test]\nfn apart() {\n    assert!(at_least(1, 2) == false);\n    assert!(at_least(3, 2));\n}\n",
    },
    Shape {
        name: "if expression",
        signature: "pub fn grade(score: u32) -> u32",
        before: "if score >= 50 { 1 } else { 0 }",
        after: "if score > 50 { 1 } else { 0 }",
        test: "use lp::grade;\n\n#[test]\nfn apart() {\n    assert_eq!(grade(10), 0);\n    assert_eq!(grade(90), 1);\n}\n",
    },
];

fn one_line(signature: &str, body: &str) -> String {
    format!("{signature} {{ {body} }}\n")
}

fn rustfmt_layout(signature: &str, body: &str) -> String {
    format!("{signature} {{\n    {body}\n}}\n")
}

#[test]
fn one_line_and_rustfmt_layouts_have_identical_repair_readiness_and_facts() -> Result<(), String> {
    for shape in SHAPES {
        let compact = Repo::create(
            &one_line(shape.signature, shape.before),
            &one_line(shape.signature, shape.after),
            shape.test,
        )?;
        let formatted = Repo::create(
            &rustfmt_layout(shape.signature, shape.before),
            &rustfmt_layout(shape.signature, shape.after),
            shape.test,
        )?;
        let compact_repair = repair_facts(&compact)?;
        let formatted_repair = repair_facts(&formatted)?;
        assert_eq!(
            compact_repair, formatted_repair,
            "repair facts differ by layout for {}",
            shape.name
        );
        assert_eq!(
            check_verdict(&compact)?,
            check_verdict(&formatted)?,
            "check verdict differs by layout for {}",
            shape.name
        );
    }
    Ok(())
}
