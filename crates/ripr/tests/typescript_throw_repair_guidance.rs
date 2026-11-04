#![cfg(feature = "lang-typescript")]

use std::process::Command;

#[test]
fn weak_throw_has_a_reason_and_no_start() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!("ripr-throw-guidance-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("test"))?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    std::fs::write(root.join(".gitignore"), "/target/\n")?;
    let git = |args: &[&str]| -> Result<(), Box<dyn std::error::Error>> {
        let result = Command::new("git")
            .current_dir(&root)
            .args([
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
            ])
            .args(args)
            .output()?;
        assert!(
            result.status.success(),
            "git: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(())
    };
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"fixture","devDependencies":{"vitest":"*"}}"#,
    )?;
    let source = "export class EmptyError extends Error {}\nexport class TooLongError extends Error {}\nexport function parse(s: string): number {\n  if (s.length > 10) {\n    throw new EmptyError();\n  }\n  return s.length;\n}\n";
    std::fs::write(root.join("src/index.ts"), source)?;
    let test = "import { test, expect } from 'vitest';\nimport { parse, TooLongError } from '../src/index';\ntest('long input is error', () => {\n  expect(() => parse('abcdefghijklmnop')).toThrow();\n});\n";
    std::fs::write(root.join("test/t.test.ts"), test)?;
    git(&["init", "-q"])?;
    git(&["add", "."])?;
    git(&["commit", "-qm", "base"])?;
    std::fs::write(
        root.join("src/index.ts"),
        source.replace("throw new EmptyError()", "throw new TooLongError()"),
    )?;
    git(&["add", "."])?;
    git(&["commit", "-qm", "change"])?;
    let check = || -> Result<String, Box<dyn std::error::Error>> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&root)
            .args(["check", "--base", "HEAD~1"])
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?)
    };
    let weak = check()?;
    assert!(
        weak.contains("Static exposure: weak")
            && weak.contains("selected error assertion has no resolved expected value"),
        "{weak}"
    );
    assert!(
        weak.contains("no repair start offered; follow the Safe next action above")
            && !weak.contains("this finding has no gap ID"),
        "{weak}"
    );
    assert!(
        !weak
            .lines()
            .any(|line| line.trim_start().starts_with("ripr agent repair ")),
        "{weak}"
    );
    std::fs::write(
        root.join("test/t.test.ts"),
        test.replace("toThrow()", "toThrow(TooLongError)"),
    )?;
    let exact = check()?;
    assert!(
        exact.contains("Static exposure: exposed")
            && !exact
                .lines()
                .any(|line| line.trim_start().starts_with("ripr agent repair ")),
        "{exact}"
    );
    // The issue's inline conditional is an ambiguous predicate, not an
    // established error-path probe. Its refusal must still name the real
    // missing target instead of claiming there is no gap ID.
    git(&["reset", "--hard", "HEAD~1"])?;
    let inline = "export class EmptyError extends Error {}\nexport class TooLongError extends Error {}\nexport function parse(s: string): number {\n  if (s.length === 0) { throw new EmptyError(); }\n  if (s.length > 10) { throw new EmptyError(); }\n  return s.length;\n}\n";
    std::fs::write(root.join("src/index.ts"), inline)?;
    std::fs::write(root.join("test/t.test.ts"), test)?;
    git(&["add", "."])?;
    git(&["commit", "--amend", "-qm", "inline base"])?;
    std::fs::write(
        root.join("src/index.ts"),
        inline.replace(
            "if (s.length > 10) { throw new EmptyError(); }",
            "if (s.length > 10) { throw new TooLongError(); }",
        ),
    )?;
    git(&["add", "."])?;
    git(&["commit", "-qm", "inline change"])?;
    let inline_weak = check()?;
    assert!(
        inline_weak.contains("cannot name a safe target")
            && inline_weak.contains("no repair start offered; follow the Safe next action above")
            && !inline_weak.contains("this finding has no gap ID"),
        "{inline_weak}"
    );
    std::fs::write(
        root.join("test/t.test.ts"),
        test.replace("toThrow()", "toThrow(TooLongError)"),
    )?;
    let inline_exact = check()?;
    assert!(
        inline_exact.contains("Static exposure: weak"),
        "{inline_exact}"
    );
    Ok(())
}
