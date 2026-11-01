//! #11398: a changed `self.field` write inside a method has a repository seam
//! whose required check is the written field. Only an exact read of that field
//! on the binding that ran the owner grades it; a sibling read, a read on
//! another binding, a later write or no read never does. No case here starts a
//! repair: a field write has no start policy, so `check` and `repair` give the
//! same specific refusal that names the field check.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fw\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const FIELD_CHECK: &str = "a test assertion that checks the changed field value";

struct Repo(PathBuf);

impl Repo {
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-field-write-seam-{}-{}",
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

    fn check_text(&self) -> Result<String, String> {
        Ok(text(&self.ripr(&["check", "--base", "HEAD~1"])?))
    }

    fn repair_text(&self) -> Result<(bool, String), String> {
        let output = self.ripr(&["repair", "--root", "."])?;
        Ok((output.status.success(), text(&output)))
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

fn printed_repair_start(check: &str) -> bool {
    check
        .lines()
        .any(|line| line.trim().starts_with("ripr repair "))
}

const COUNTER_OLD: &str = "#[derive(Default)]\npub struct Counter {\n    pub count: u32,\n    pub label: u32,\n}\n\nimpl Counter {\n    pub fn bump(&mut self) {\n        self.count += 2;\n    }\n}\n";
const COUNTER_NEW: &str = "#[derive(Default)]\npub struct Counter {\n    pub count: u32,\n    pub label: u32,\n}\n\nimpl Counter {\n    pub fn bump(&mut self) {\n        self.count += 1;\n    }\n}\n";

/// A field write whose weak or missing read leaves the field unchecked: both
/// commands refuse with the plain field-check reason, never start, and never
/// call the write checked.
fn assert_field_check_refusal(case: &str, old: &str, new: &str, test: &str) -> Result<(), String> {
    let repo = Repo::create(old, new, test)?;
    let check = repo.check_text()?;
    assert!(
        !printed_repair_start(&check),
        "{case}: check must not print a repair start:\n{check}"
    );
    assert!(
        !check.contains("Static exposure: exposed"),
        "{case}: the write must not read as checked:\n{check}"
    );
    let (ok, repair) = repo.repair_text()?;
    assert!(!ok, "{case}: repair must refuse:\n{repair}");
    assert!(
        repair.contains(FIELD_CHECK),
        "{case}: repair must name the field check:\n{repair}"
    );
    assert!(
        !repair.contains("touch no code") && !repair.contains("no seams visible"),
        "{case}: the write must have a seam:\n{repair}"
    );
    Ok(())
}

#[test]
fn setter_with_no_read_of_the_field_names_the_field_check() -> Result<(), String> {
    assert_field_check_refusal(
        "no-read",
        COUNTER_OLD,
        COUNTER_NEW,
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n}\n",
    )
}

#[test]
fn sibling_field_read_does_not_check_the_written_field() -> Result<(), String> {
    assert_field_check_refusal(
        "sibling",
        COUNTER_OLD,
        COUNTER_NEW,
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n    assert_eq!(c.label, 7);\n}\n",
    )
}

#[test]
fn read_on_another_binding_does_not_check_the_written_field() -> Result<(), String> {
    assert_field_check_refusal(
        "other-binding",
        COUNTER_OLD,
        COUNTER_NEW,
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n    let d = Counter::default();\n    assert_eq!(d.count, 5);\n}\n",
    )
}

#[test]
fn exact_read_of_the_written_field_is_checked_not_a_start() -> Result<(), String> {
    let repo = Repo::create(
        COUNTER_OLD,
        COUNTER_NEW,
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n    assert_eq!(c.count, 1);\n}\n",
    )?;
    let check = repo.check_text()?;
    assert!(
        !printed_repair_start(&check),
        "an exact read is not a start:\n{check}"
    );
    let (ok, repair) = repo.repair_text()?;
    assert!(!ok, "{repair}");
    assert!(
        repair.contains("`self.count += 1`: existing tests already check it")
            && !repair.contains("Ready to repair.")
            && !repair.contains(FIELD_CHECK),
        "an exact read of the written field is already checked:\n{repair}"
    );
    Ok(())
}

#[test]
fn should_panic_test_does_not_check_the_written_field() -> Result<(), String> {
    assert_field_check_refusal(
        "should-panic",
        COUNTER_OLD,
        COUNTER_NEW,
        "use fw::Counter;\n\n#[test]\n#[should_panic]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n    assert_eq!(c.count, 1);\n}\n",
    )
}

#[test]
fn ignored_test_does_not_check_the_written_field() -> Result<(), String> {
    let repo = Repo::create(
        COUNTER_OLD,
        COUNTER_NEW,
        "use fw::Counter;\n\n#[test]\n#[ignore]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n    assert_eq!(c.count, 1);\n}\n",
    )?;
    let check = repo.check_text()?;
    assert!(
        !check.contains("Static exposure: exposed") && !printed_repair_start(&check),
        "an ignored test never runs, so it checks nothing:\n{check}"
    );
    let (ok, repair) = repo.repair_text()?;
    assert!(
        !ok && repair.contains("`self.count += 1`") && !repair.contains("existing tests already"),
        "an ignored test must leave the write unchecked, with a seam:\n{repair}"
    );
    Ok(())
}

#[test]
fn later_write_to_the_field_leaves_no_seam() -> Result<(), String> {
    let old = COUNTER_OLD.replace(
        "        self.count += 2;\n",
        "        self.count += 2;\n        self.count += 1;\n",
    );
    let new = old.replacen("self.count += 2;", "self.count += 5;", 1);
    let repo = Repo::create(
        &old,
        &new,
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n    assert_eq!(c.count, 6);\n}\n",
    )?;
    let (_, repair) = repo.repair_text()?;
    assert!(
        !repair.contains(FIELD_CHECK),
        "a write the owner overwrites later has no field seam:\n{repair}"
    );
    Ok(())
}

const BUILDER_OLD: &str = "pub struct Req {\n    pub url: u32,\n    pub timeout: u32,\n}\n\nimpl Req {\n    pub fn new() -> Self {\n        Req { url: 0, timeout: 0 }\n    }\n\n    pub fn timeout(mut self, t: u32) -> Self {\n        self.timeout = t;\n        self\n    }\n}\n";
const BUILDER_NEW: &str = "pub struct Req {\n    pub url: u32,\n    pub timeout: u32,\n}\n\nimpl Req {\n    pub fn new() -> Self {\n        Req { url: 0, timeout: 0 }\n    }\n\n    pub fn timeout(mut self, t: u32) -> Self {\n        self.timeout = t * 2;\n        self\n    }\n}\n";

#[test]
fn builder_tail_self_is_not_a_later_write() -> Result<(), String> {
    assert_field_check_refusal(
        "builder",
        BUILDER_OLD,
        BUILDER_NEW,
        "use fw::Req;\n\n#[test]\nfn sets_url() {\n    let r = Req::new().timeout(5);\n    assert_eq!(r.url, 0);\n}\n",
    )
}

/// #11398: the hand-written step for a field write names the receiver, not a
/// wrapper or `Err` variant that a struct-literal field would need.
#[test]
fn builder_field_write_names_the_receiver_not_a_wrapper() -> Result<(), String> {
    let repo = Repo::create(
        BUILDER_OLD,
        BUILDER_NEW,
        "use fw::Req;\n\n#[test]\nfn sets_url() {\n    let r = Req::new().timeout(5);\n    assert_eq!(r.url, 0);\n}\n",
    )?;
    let check = repo.check_text()?;
    assert!(
        check.contains("sets a field of `self` inside a method")
            && check.contains("the value you called it on")
            && !check.contains("match the wrapper"),
        "check must name the field write, not a wrapper:\n{check}"
    );
    Ok(())
}

/// #11398 review: `retries(mut self, ..)` writes a moved copy. For a `Copy`
/// type the test's `o` keeps its old field, so `assert_eq!(o.retries, 3)`
/// never sees the write and must not read as a check of it.
#[test]
fn copy_builder_read_of_the_unmoved_binding_is_not_a_check() -> Result<(), String> {
    let old = "#[derive(Clone, Copy)]\npub struct Opts {\n    pub retries: u32,\n}\n\nimpl Opts {\n    pub fn retries(mut self, n: u32) -> Self {\n        self.retries = n;\n        self\n    }\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("self.retries = n;", "self.retries = n * 2;"),
        "use fw::Opts;\n\n#[test]\nfn keeps() {\n    let o = Opts { retries: 3 };\n    o.retries(5);\n    assert_eq!(o.retries, 3);\n}\n",
    )?;
    let check = repo.check_text()?;
    assert!(
        !check.contains("Static exposure: exposed") && !printed_repair_start(&check),
        "a read of the unmoved binding must not credit the write:\n{check}"
    );
    Ok(())
}

/// The borrowed-receiver twin of the case above: the same exact read of a
/// `&mut self` write IS the check, so the rule is the receiver, not the read.
#[test]
fn borrowed_receiver_read_of_the_written_field_is_still_a_check() -> Result<(), String> {
    let old = "#[derive(Clone, Copy)]\npub struct Opts {\n    pub retries: u32,\n}\n\nimpl Opts {\n    pub fn set_retries(&mut self, n: u32) {\n        self.retries = n;\n    }\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("self.retries = n;", "self.retries = n * 2;"),
        "use fw::Opts;\n\n#[test]\nfn sets() {\n    let mut o = Opts { retries: 3 };\n    o.set_retries(5);\n    assert_eq!(o.retries, 10);\n}\n",
    )?;
    let check = repo.check_text()?;
    assert!(
        check.contains("Static exposure: exposed"),
        "a borrowed receiver's exact read must stay a check:\n{check}"
    );
    Ok(())
}

/// A private field cannot be read by a test, so its write has no seam: the
/// pilot ranking put bytesize's private `format` setters first as a weak gap
/// whose tests observe the field through `Display` (#11398).
#[test]
fn private_field_write_has_no_seam() -> Result<(), String> {
    let old = COUNTER_OLD.replace("pub count", "count");
    let new = COUNTER_NEW.replace("pub count", "count");
    let repo = Repo::create(
        &old,
        &new,
        "use fw::Counter;\n\n#[test]\nfn bumps() {\n    let mut c = Counter::default();\n    c.bump();\n}\n",
    )?;
    let (_, repair) = repo.repair_text()?;
    assert!(
        !repair.contains(FIELD_CHECK),
        "a private field has no readable check:\n{repair}"
    );
    Ok(())
}
