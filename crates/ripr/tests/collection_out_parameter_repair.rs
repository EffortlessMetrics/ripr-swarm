//! #11402: a changed push into a `&mut Vec` parameter, tested by passing a
//! fresh local vector and checking only how many items it holds. The changed
//! value is followed to that vector, so neither `ripr check` nor bare
//! `ripr repair` says the value cannot be followed any more. Both still refuse
//! to start, and both name the same reason: the exact check of the vector's
//! contents, which ripr cannot write yet (never a mock). Anything less
//! certain keeps the earlier answer, also in both commands.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const OLD: &str =
    "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_string());\n}\n";
const NEW: &str =
    "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_uppercase());\n}\n";

/// The plain reason both commands give once the value is followed.
const CONTENTS_CHECK: &str =
    "an exact check of what the test's vector holds after the call, which ripr cannot write yet";
/// The plain reason both commands give while the value is not followed.
const NOT_FOLLOWED: &str = "a way to follow the changed value to something a test can check";

struct Repo(PathBuf);

impl Repo {
    /// `old_src` + `test` on `main`, then `new_src` on `feature`.
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-collection-out-param-{}-{}",
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

    /// The `ripr check` screen (with `check_args` added) and bare
    /// `ripr repair` (started, text).
    fn answers(&self, check_args: &[&str]) -> Result<(String, bool, String), String> {
        let mut args = vec!["check", "--base", "main"];
        args.extend_from_slice(check_args);
        let check = text(&self.ripr(&args)?);
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

/// Neither command starts, in the default check and the two narrow modes,
/// and both name `reason` (and not `absent`) in the default run.
fn assert_both_refuse_with(
    case: &str,
    repo: &Repo,
    reason: &str,
    absent: &str,
) -> Result<(), String> {
    for check_args in [
        &[][..],
        &["--no-unchanged-tests"][..],
        &["--mode", "instant"][..],
    ] {
        let (check, started, repair) = repo.answers(check_args)?;
        if started {
            return Err(format!("{case}: repair must not start:\n{repair}"));
        }
        if let Some(id) = check_start_id(&check) {
            return Err(format!(
                "{case} {check_args:?}: check must not offer `ripr repair {id}`:\n{check}"
            ));
        }
        if !check_args.is_empty() {
            continue;
        }
        for (command, screen) in [("check", &check), ("repair", &repair)] {
            if !screen.contains(reason) || screen.contains(absent) {
                return Err(format!(
                    "{case}: {command} must name `{reason}`, not `{absent}`:\n{screen}"
                ));
            }
        }
    }
    Ok(())
}

fn test_file(body: &str) -> String {
    format!("use fx::log_line;\n\n#[test]\nfn log_line_appends() {{\n{body}}}\n")
}

/// The changed value is followed: both commands name the contents check.
fn assert_followed(case: &str, old: &str, new: &str, body: &str) -> Result<(), String> {
    let repo = Repo::create(old, new, &test_file(body))?;
    assert_both_refuse_with(case, &repo, CONTENTS_CHECK, NOT_FOLLOWED)
}

/// The changed value is not followed: both commands keep the earlier reason.
fn assert_not_followed(case: &str, old: &str, new: &str, test: &str) -> Result<(), String> {
    let repo = Repo::create(old, new, test)?;
    assert_both_refuse_with(case, &repo, NOT_FOLLOWED, CONTENTS_CHECK)
}

const COUNT_BODY: &str =
    "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert_eq!(b.len(), 1);\n";

#[test]
fn count_read_of_the_passed_vector_is_followed_in_both_commands() -> Result<(), String> {
    assert_followed("len", OLD, NEW, COUNT_BODY)
}

#[test]
fn emptiness_read_of_the_passed_vector_is_followed_in_both_commands() -> Result<(), String> {
    assert_followed(
        "is_empty",
        OLD,
        NEW,
        "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert!(!b.is_empty());\n",
    )
}

#[test]
fn reading_a_different_binding_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "different binding",
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    let c: Vec<String> = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert_eq!(c.len(), 0);\n",
        ),
    )
}

#[test]
fn reading_the_pushed_item_is_not_this_rules_to_grade() -> Result<(), String> {
    // A read of the contents may already check the pushed value.
    let repo = Repo::create(
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert_eq!(b.len(), 1);\n    assert_eq!(b[0], \"hi\");\n",
        ),
    )?;
    let (check, started, repair) = repo.answers(&[])?;
    if started || check_start_id(&check).is_some() {
        return Err(format!(
            "contents read: neither may start:\n{check}\n{repair}"
        ));
    }
    if check.contains(CONTENTS_CHECK) || repair.contains(CONTENTS_CHECK) {
        return Err(format!(
            "contents read: the count-only rule must not apply:\n{check}\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn reborrowed_vector_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "reborrow",
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    let r = &mut b;\n    log_line(r, \"hi\");\n    assert_eq!(b.len(), 1);\n",
        ),
    )
}

#[test]
fn vector_used_before_the_call_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "used before",
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    b.push(String::new());\n    log_line(&mut b, \"hi\");\n    assert_eq!(b.len(), 2);\n",
        ),
    )
}

#[test]
fn vector_aliased_after_the_call_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "aliased after",
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    let c = &b;\n    assert_eq!(c.len(), 1);\n",
        ),
    )
}

#[test]
fn namesake_in_another_module_is_not_followed() -> Result<(), String> {
    // The test calls `other::log_line`, which pushes nothing.
    let other = "\npub mod other {\n    pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n        let _ = (buf, msg);\n    }\n}\n";
    let repo = Repo::create(
        &format!("{OLD}{other}"),
        &format!("{NEW}{other}"),
        &format!("use fx::other::log_line;\n\n#[test]\nfn log_line_appends() {{\n{COUNT_BODY}}}\n"),
    )?;
    let (check, started, repair) = repo.answers(&[])?;
    if started || check_start_id(&check).is_some() {
        return Err(format!("namesake: neither may start:\n{check}\n{repair}"));
    }
    if check.contains(CONTENTS_CHECK) || repair.contains(CONTENTS_CHECK) {
        return Err(format!(
            "namesake: the other module's call must not be followed:\n{check}\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn conditional_push_is_not_followed() -> Result<(), String> {
    let old = "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    if !msg.is_empty() {\n        buf.push(msg.to_string());\n    }\n}\n";
    assert_not_followed(
        "conditional push",
        old,
        &old.replace("msg.to_string()", "msg.to_uppercase()"),
        &test_file(COUNT_BODY),
    )
}

#[test]
fn second_push_is_not_followed() -> Result<(), String> {
    let old = "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_string());\n    buf.push(String::new());\n}\n";
    assert_not_followed(
        "second push",
        old,
        &old.replace("msg.to_string()", "msg.to_uppercase()"),
        &test_file(COUNT_BODY),
    )
}

#[test]
fn ignored_test_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "ignore",
        OLD,
        NEW,
        &format!(
            "use fx::log_line;\n\n#[test]\n#[ignore]\nfn log_line_appends() {{\n{COUNT_BODY}}}\n"
        ),
    )
}

#[test]
fn should_panic_test_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "should_panic",
        OLD,
        NEW,
        &format!(
            "use fx::log_line;\n\n#[test]\n#[should_panic]\nfn log_line_appends() {{\n{COUNT_BODY}}}\n"
        ),
    )
}
