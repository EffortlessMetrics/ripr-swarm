//! #11400: a changed `const` value is its readers' new return value. When
//! every same-file reader returns the constant on one line, `ripr check`
//! names that returned expression as the change and `ripr repair` scopes to
//! the same code, so a weakly checked reader gets the same repair start from
//! both commands. Anything less certain keeps the constant an unknown with no
//! start.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const GREET_OLD: &str = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    format!(\"{GREETING}, {name}\")\n}\n";

const WEAK_TEST: &str = "use fx::greet;\n\n#[test]\nfn greet_mentions_name() {\n    assert!(greet(\"bob\").contains(\"bob\"));\n}\n";

struct Repo(PathBuf);

impl Repo {
    /// `old_src` + `test` on `main`, then `new_src` on `feature`.
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-const-reader-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        repo.write("Cargo.toml", CARGO_TOML)?;
        repo.write("src/lib.rs", old_src)?;
        repo.write("tests/t.rs", test)?;
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.git(&["branch", "-M", "main"])?;
        repo.git(&["checkout", "-qb", "feature"])?;
        repo.write("src/lib.rs", new_src)?;
        repo.git(&["commit", "-qam", "change"])?;
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

    fn ripr(&self, args: &[&str]) -> Result<Output, String> {
        Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())
    }

    /// The `ripr check` screen and bare `ripr repair` (started, text).
    fn answers(&self) -> Result<(String, bool, String), String> {
        let check = text(&self.ripr(&["check", "--base", "main"])?);
        let repair = self.ripr(&["repair", "--root", "."])?;
        Ok((check, repair.status.success(), text(&repair)))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The id after `ripr repair ` on the check screen, if check offers a start.
fn check_start_id(check: &str) -> Option<&str> {
    check.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
    })
}

fn greet_case(new_value: &str, test: &str) -> Result<Repo, String> {
    Repo::create(GREET_OLD, &GREET_OLD.replace("\"hello\"", new_value), test)
}

/// Neither command starts a repair, and check offers no start line.
fn assert_no_start(case: &str, repo: &Repo) -> Result<(String, String), String> {
    let (check, started, repair) = repo.answers()?;
    if started {
        return Err(format!("{case}: repair must not start:\n{repair}"));
    }
    if let Some(id) = check_start_id(&check) {
        return Err(format!(
            "{case}: check must not offer `ripr repair {id}`:\n{check}"
        ));
    }
    Ok((check, repair))
}

#[test]
fn weakly_checked_reader_of_a_changed_constant_starts_in_both_commands() -> Result<(), String> {
    // The audit `string-const` case: `greet` returns the constant in its
    // `format!`, and the only test checks `contains("bob")`.
    let repo = greet_case("\"hi\"", WEAK_TEST)?;
    let (check, started, repair) = repo.answers()?;
    let Some(id) = check_start_id(&check) else {
        return Err(format!("check must offer a repair start:\n{check}"));
    };
    if !check.contains("format!(\"{GREETING}, {name}\")") {
        return Err(format!("check must name greet's returned value:\n{check}"));
    }
    if !started || !repair.contains(&format!("seam `{id}`")) {
        return Err(format!(
            "bare repair must start on the id check prints ({id}):\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn exactly_checked_reader_is_already_checked_and_does_not_start() -> Result<(), String> {
    let repo = greet_case(
        "\"hi\"",
        "use fx::greet;\n\n#[test]\nfn greet_is_exact() {\n    assert_eq!(greet(\"bob\"), \"hi, bob\");\n}\n",
    )?;
    let (check, _) = assert_no_start("exact pin", &repo)?;
    if !check.contains("Static exposure: exposed") {
        return Err(format!("exact pin: check must read exposed:\n{check}"));
    }
    Ok(())
}

#[test]
fn reader_no_test_calls_does_not_start() -> Result<(), String> {
    let repo = Repo::create(
        &format!("{GREET_OLD}\npub fn other() -> u32 {{\n    1\n}}\n"),
        &format!("{GREET_OLD}\npub fn other() -> u32 {{\n    1\n}}\n")
            .replace("\"hello\"", "\"hi\""),
        "use fx::other;\n\n#[test]\nfn other_is_positive() {\n    assert!(other() > 0);\n}\n",
    )?;
    assert_no_start("no test calls greet", &repo).map(|_| ())
}

#[test]
fn constant_declared_twice_in_the_file_does_not_start() -> Result<(), String> {
    // An associated const of the same name cannot be told apart by spelling.
    let old = format!(
        "{GREET_OLD}\npub struct Other;\n\nimpl Other {{\n    pub const GREETING: &'static str = \"yo\";\n}}\n"
    );
    let repo = Repo::create(&old, &old.replacen("\"hello\"", "\"hi\"", 1), WEAK_TEST)?;
    assert_no_start("declared twice", &repo).map(|_| ())
}

#[test]
fn reader_that_shadows_the_name_does_not_start() -> Result<(), String> {
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    let GREETING = \"hey\";\n    format!(\"{GREETING}, {name}\")\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("shadowed", &repo).map(|_| ())
}

#[test]
fn reader_that_returns_the_constant_through_a_binding_does_not_start() -> Result<(), String> {
    // The returned line does not name the constant, so it is not tied to it.
    let old = "pub const GREETING: &str = \"hello\";\n\npub fn greet(name: &str) -> String {\n    let g = GREETING;\n    format!(\"{g}, {name}\")\n}\n";
    let repo = Repo::create(old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("through a binding", &repo).map(|_| ())
}

#[test]
fn a_second_reader_without_a_returned_read_keeps_the_constant_unknown() -> Result<(), String> {
    // All readers or none: `loud` reads the constant in a condition only.
    let old = format!(
        "{GREET_OLD}\npub fn loud(name: &str) -> bool {{\n    if name.starts_with(GREETING) {{\n        return true;\n    }}\n    false\n}}\n"
    );
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("second reader", &repo).map(|_| ())
}

#[test]
fn changed_static_does_not_start() -> Result<(), String> {
    let old = GREET_OLD.replace("pub const GREETING", "pub static GREETING");
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("static", &repo).map(|_| ())
}

#[test]
fn constant_the_value_evaluation_reads_keeps_its_evaluated_answer() -> Result<(), String> {
    // RIPR-SPEC-0048 evaluates `over(5)` with 10 and with 11: both return
    // `false`, so the test cannot notice the change. Moving the probe to
    // `x > LIMIT` would read the literal comparison as a check of it.
    let old = "pub const LIMIT: u32 = 10;\n\npub fn over(x: u32) -> bool {\n    x > LIMIT\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("= 10;", "= 11;"),
        "use fx::over;\n\n#[test]\nfn five_is_not_over() {\n    assert_eq!(over(5), false);\n}\n",
    )?;
    let (check, _) = assert_no_start("evaluated constant", &repo)?;
    if check.contains("Static exposure: exposed") || check.contains("Changed behavior: x > LIMIT") {
        return Err(format!(
            "evaluated constant: check must keep the evaluated answer:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn constant_read_outside_any_function_does_not_start() -> Result<(), String> {
    // A compile-time assertion reads the constant too; that read is not a
    // returned value, so the change is not described through `greet`.
    let old = format!("{GREET_OLD}\nconst _: () = assert!(!GREETING.is_empty());\n");
    let repo = Repo::create(&old, &old.replace("\"hello\"", "\"hi\""), WEAK_TEST)?;
    assert_no_start("item-level read", &repo).map(|_| ())
}
