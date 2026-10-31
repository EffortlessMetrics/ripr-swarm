//! #11398: a changed field write in a method (`self.count += 1`, a builder's
//! `self.timeout = t * 2; self`) has a repository seam, so `ripr check` and
//! `ripr repair` give the same specific answer for it instead of "no code ripr
//! can start a repair on". The seam is graded only by the diff probe's exact
//! field read (RIPR-SPEC-0228). An unread field names the missing field check,
//! an exactly read one is already checked, a read the rule cannot judge stays
//! unknown, and no case starts a repair.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fw\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const MISSING_FIELD_CHECK: &str = "a test assertion that checks the changed field value";
const UNJUDGED_READ: &str = "a test assertion that would fail if the changed behavior were wrong";
const NO_CODE: &str = "touch no code ripr can start a repair on";

const COUNTER_OLD: &str = "pub struct Counter {\n    pub count: u32,\n    pub label: String,\n}\n\nimpl Counter {\n    pub fn new() -> Self {\n        Counter { count: 0, label: String::new() }\n    }\n\n    pub fn bump(&mut self) {\n        self.count += 2;\n    }\n\n    pub fn reset(&mut self) {\n        self.count = 0;\n    }\n}\n";

const BUILDER_OLD: &str = "pub struct Req {\n    pub url: String,\n    pub timeout: u32,\n}\n\nimpl Req {\n    pub fn new(url: &str) -> Self {\n        Req { url: url.to_string(), timeout: 30 }\n    }\n\n    pub fn timeout(mut self, t: u32) -> Self {\n        self.timeout = t;\n        self\n    }\n}\n";

struct Repo(PathBuf);

impl Repo {
    /// A two-commit repository: `old_src` + `test` committed on `main`, then
    /// `new_src` on `feature`, so bare `ripr repair` reads the same change.
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-field-write-repair-{}-{}",
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

    /// The `ripr check` screen and bare `ripr repair` (exit, text).
    fn answers(&self) -> Result<(String, bool, String), String> {
        let check = text(&self.ripr(&["check", "--base", "HEAD~1"])?);
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

fn counter_case(test_body: &str) -> Result<Repo, String> {
    Repo::create(
        COUNTER_OLD,
        &COUNTER_OLD.replace("self.count += 2;", "self.count += 1;"),
        &format!(
            "use fw::Counter;\n\n#[test]\nfn bumps() {{\n    let mut c = Counter::new();\n{test_body}}}\n"
        ),
    )
}

fn builder_case(test_body: &str) -> Result<Repo, String> {
    Repo::create(
        BUILDER_OLD,
        &BUILDER_OLD.replace("self.timeout = t;", "self.timeout = t * 2;"),
        &format!("use fw::Req;\n\n#[test]\nfn sets() {{\n{test_body}}}\n"),
    )
}

/// The changed field write names the missing field check in both commands,
/// on its own line, and nothing starts.
fn assert_names_missing_field_check(case: &str, repo: &Repo, line: &str) -> Result<(), String> {
    let (check, started, repair) = repo.answers()?;
    if started {
        return Err(format!("{case}: repair must not start:\n{repair}"));
    }
    if !check.contains(&format!("still missing: {MISSING_FIELD_CHECK}")) {
        return Err(format!(
            "{case}: check must name the missing field check:\n{check}"
        ));
    }
    if !repair.contains(&format!("the code at src/lib.rs:{line}"))
        || !repair.contains(&format!("missing evidence: {MISSING_FIELD_CHECK}"))
    {
        return Err(format!(
            "{case}: repair must name the field write and its missing check:\n{repair}"
        ));
    }
    if repair.contains(NO_CODE) || repair.contains("no seams visible") {
        return Err(format!(
            "{case}: repair must not say no code is on the changed line:\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn sibling_field_read_names_the_missing_field_check_in_both_commands() -> Result<(), String> {
    let repo = counter_case("    c.bump();\n    assert_eq!(c.label, \"\");\n")?;
    assert_names_missing_field_check("sibling read", &repo, "12")
}

#[test]
fn unread_field_write_names_the_missing_field_check_in_both_commands() -> Result<(), String> {
    let repo = counter_case("    c.bump();\n")?;
    assert_names_missing_field_check("no read", &repo, "12")
}

#[test]
fn builder_returning_self_names_the_missing_field_check() -> Result<(), String> {
    // The audit case: the tail `self` hands the value on, so the write is the
    // owner's last word on `timeout`; the test reads only `url`.
    let repo = builder_case(
        "    let r = Req::new(\"http://x\").timeout(10);\n    assert_eq!(r.url, \"http://x\");\n",
    )?;
    assert_names_missing_field_check("builder sibling read", &repo, "12")?;
    // The hand-written step names the receiver, not a wrapper or `Err`
    // variant that a struct-literal field would need.
    let (check, _, _) = repo.answers()?;
    if !check.contains("sets a field of `self` inside a method")
        || !check.contains("the value you called it on")
        || check.contains("match the wrapper")
    {
        return Err(format!(
            "builder: check must name the field write, not a wrapper:\n{check}"
        ));
    }
    Ok(())
}

#[test]
fn exact_field_read_is_already_checked_in_both_commands() -> Result<(), String> {
    let repo = counter_case("    c.bump();\n    assert_eq!(c.count, 1);\n")?;
    let (check, started, repair) = repo.answers()?;
    if started {
        return Err(format!("exact read: repair must not start:\n{repair}"));
    }
    if !check.contains("Static exposure: exposed") {
        return Err(format!("exact read: check must read exposed:\n{check}"));
    }
    if !already_checked(&repair) || repair.contains(MISSING_FIELD_CHECK) {
        return Err(format!(
            "exact read: repair must call the write already checked:\n{repair}"
        ));
    }
    Ok(())
}

/// Whether bare repair says existing tests already check the change. #11392
/// rewords "already grip it" to "already check it"; either spelling counts.
fn already_checked(repair: &str) -> bool {
    ["already grip it", "already check it"]
        .iter()
        .any(|phrase| repair.contains(phrase))
}

/// A read the exact rule cannot judge keeps the write unknown: it never reads
/// as already checked and never as "no test reads the field" alone.
fn assert_read_unjudged(case: &str, repo: &Repo) -> Result<(), String> {
    let (check, started, repair) = repo.answers()?;
    if started {
        return Err(format!("{case}: repair must not start:\n{repair}"));
    }
    if check.contains("Static exposure: exposed") || already_checked(&repair) {
        return Err(format!(
            "{case}: an unconfirmed read is not a check:\n{check}\n{repair}"
        ));
    }
    if !repair.contains(UNJUDGED_READ) {
        return Err(format!(
            "{case}: repair must say the read could not be judged:\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn builder_chain_exact_read_stays_unjudged_not_missing() -> Result<(), String> {
    let repo = builder_case(
        "    let r = Req::new(\"http://x\").timeout(10);\n    assert_eq!(r.timeout, 20);\n",
    )?;
    assert_read_unjudged("builder chain read", &repo)
}

#[test]
fn weak_field_read_stays_unjudged() -> Result<(), String> {
    let repo = counter_case("    c.bump();\n    assert!(c.count > 0);\n")?;
    assert_read_unjudged("weak read", &repo)
}

#[test]
fn read_after_a_later_reset_is_not_a_check_of_the_write() -> Result<(), String> {
    // `reset` rewrites `count` between the owner call and the read, so the
    // exact rule refuses it; the name still appears, so the read is unjudged.
    let repo = counter_case("    c.bump();\n    c.reset();\n    assert_eq!(c.count, 1);\n")?;
    assert_read_unjudged("reset between", &repo)
}

#[test]
fn read_of_the_same_field_on_another_binding_is_not_a_check() -> Result<(), String> {
    let repo =
        counter_case("    let d = Counter::new();\n    c.bump();\n    assert_eq!(d.count, 0);\n")?;
    assert_read_unjudged("other binding", &repo)
}

#[test]
fn write_followed_by_an_unresolved_self_call_has_no_field_seam() -> Result<(), String> {
    // A later `self.touch()` may write the field again, so no seam names the
    // write as the owner's last word; the answer stays "no code".
    let old = "pub struct Counter {\n    pub count: u32,\n}\n\nimpl Counter {\n    pub fn bump(&mut self) {\n        self.count += 2;\n        self.touch();\n    }\n\n    fn touch(&mut self) {}\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("self.count += 2;", "self.count += 1;"),
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter { count: 0 };\n    c.bump();\n}\n",
    )?;
    let (_check, started, repair) = repo.answers()?;
    if started || repair.contains(MISSING_FIELD_CHECK) || repair.contains("self.count += 1") {
        return Err(format!(
            "a write followed by a self call must not get a field seam:\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn nested_field_path_write_has_no_field_seam() -> Result<(), String> {
    let old = "pub struct Inner {\n    pub count: u32,\n}\n\npub struct Outer {\n    pub inner: Inner,\n}\n\nimpl Outer {\n    pub fn bump(&mut self) {\n        self.inner.count += 2;\n    }\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("+= 2;", "+= 1;"),
        "use fw::{Inner, Outer};\n\n#[test]\nfn bumps() {\n    let mut o = Outer { inner: Inner { count: 0 } };\n    o.bump();\n}\n",
    )?;
    let (_check, started, repair) = repo.answers()?;
    if started || repair.contains("self.inner.count += 1") {
        return Err(format!(
            "a nested field path must not get a field seam:\n{repair}"
        ));
    }
    Ok(())
}
