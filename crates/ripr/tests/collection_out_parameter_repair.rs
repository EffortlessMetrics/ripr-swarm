//! #11402: a changed push into a `&mut Vec` parameter, tested by passing a
//! fresh local vector and checking only how many items it holds. The changed
//! value is followed to that vector.
//!
//! When a count read pins the single push (`assert_eq!(b.len(), 1)`), that
//! read notices a deleted push, so the
//! deleted-call finding is noticed in `ripr check` and the matching gap is
//! already checked in `ripr repair`. The changed pushed value is the one gap
//! left, and both commands start a repair on that same gap, whose stub
//! writes an exact check of the vector's contents (never a mock).
//!
//! A followed count read that does not pin the push still refuses in both
//! commands with the contents-check reason, and anything less certain keeps
//! the earlier answer, also in both commands.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

const OLD: &str =
    "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_string());\n}\n";
const NEW: &str =
    "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_uppercase());\n}\n";

/// The plain reason both commands give once the value is followed.
const CONTENTS_CHECK: &str = "an exact count the test checks after the call, such as `assert_eq!(v.len(), 1)`, that would fail if nothing were pushed; until then, check the full contents by hand";
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

/// In the default check, check offers a repair start, bare repair starts on
/// that same gap, the deleted-call finding reads noticed and check offers the
/// side-effect stub. The two narrow modes leave the unchanged test out of
/// check's evidence, so check may offer no start there (bare repair reads the
/// whole workspace either way); any start check does offer is the same gap
/// bare repair starts.
fn assert_both_start(case: &str, old: &str, new: &str, test: &str) -> Result<(), String> {
    for check_args in [
        &[][..],
        &["--no-unchanged-tests"][..],
        &["--mode", "instant"][..],
    ] {
        let repo = Repo::create(old, new, test)?;
        let (check, started, repair) = repo.answers(check_args)?;
        let Some(id) = check_start_id(&check) else {
            if check_args.is_empty() {
                return Err(format!(
                    "{case} {check_args:?}: check must offer a repair start:\n{check}"
                ));
            }
            continue;
        };
        if !started || !repair.contains(&format!("seam `{id}`")) {
            return Err(format!(
                "{case} {check_args:?}: bare repair must start on the gap check offers ({id}):\n{repair}"
            ));
        }
        if check.contains(CONTENTS_CHECK) || repair.contains(CONTENTS_CHECK) {
            return Err(format!(
                "{case} {check_args:?}: a started repair names no missing check:\n{check}\n{repair}"
            ));
        }
        if check_args.is_empty()
            && !(check.contains("(exposed); kind: call_deletion")
                && check.contains("--kind side_effect"))
        {
            return Err(format!(
                "{case}: the deleted push must read noticed and the stub must be offered:\n{check}"
            ));
        }
    }
    Ok(())
}

/// The changed value is followed, but no count read pins the push: both
/// commands name the contents check and neither starts.
fn assert_followed_not_started(case: &str, old: &str, new: &str, body: &str) -> Result<(), String> {
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
fn count_read_of_the_passed_vector_starts_in_both_commands() -> Result<(), String> {
    assert_both_start("len", OLD, NEW, &test_file(COUNT_BODY))
}

#[test]
fn emptiness_read_keeps_the_contents_check_reason() -> Result<(), String> {
    // A boolean read grades weaker than an exact count on both sides, so it
    // pins nothing: the deleted push is not graded noticed, and the changed
    // value is followed but does not start.
    assert_followed_not_started(
        "is_empty",
        OLD,
        NEW,
        "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert!(!b.is_empty());\n",
    )
}

#[test]
fn count_read_that_does_not_pin_the_push_keeps_the_contents_check_reason() -> Result<(), String> {
    // `len() != 2` holds with the push and without it, so it notices
    // neither a deleted push nor a changed pushed value.
    assert_followed_not_started(
        "assert_ne",
        OLD,
        NEW,
        "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert_ne!(b.len(), 2);\n",
    )
}

#[test]
fn inner_attribute_in_the_test_body_is_not_followed() -> Result<(), String> {
    assert_not_followed(
        "inner attribute",
        OLD,
        NEW,
        &test_file(&format!("    #![allow(unused_mut)]\n{COUNT_BODY}")),
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

#[test]
fn string_count_read_starts_in_both_commands() -> Result<(), String> {
    let old = "pub fn add_mark(buf: &mut String, c: char) {\n    buf.push(c);\n}\n";
    assert_both_start(
        "String",
        old,
        &old.replace("buf.push(c)", "buf.push(c.to_ascii_uppercase())"),
        "use fx::add_mark;\n\n#[test]\nfn add_mark_appends() {\n    let mut s = String::new();\n    add_mark(&mut s, 'a');\n    assert_eq!(s.len(), 1);\n}\n",
    )
}

#[test]
fn push_argument_that_may_return_first_is_not_followed() -> Result<(), String> {
    // With `skip` true the owner returns before pushing, so the count read
    // of zero says nothing about the pushed value.
    let old = "pub fn log_line(buf: &mut Vec<String>, msg: &str, skip: bool) {\n    buf.push(if skip { return } else { msg.to_string() });\n}\n";
    assert_not_followed(
        "return in the argument",
        old,
        &old.replace("msg.to_string()", "msg.to_uppercase()"),
        "use fx::log_line;\n\n#[test]\nfn log_line_skips() {\n    let mut b = Vec::new();\n    log_line(&mut b, \"hi\", true);\n    assert_eq!(b.len(), 0);\n}\n",
    )
}

#[test]
fn exactly_checked_vector_is_already_noticed_not_a_contents_gap() -> Result<(), String> {
    // An exact read of the receiver by name already checks the pushed value,
    // so the contents-check refusal must not replace that answer.
    let repo = Repo::create(
        OLD,
        NEW,
        "use fx::log_line;\n\n#[test]\nfn log_line_appends() {\n    let mut buf = Vec::new();\n    log_line(&mut buf, \"hi\");\n    assert_eq!(buf, vec![\"HI\".to_string()]);\n}\n",
    )?;
    let (check, started, repair) = repo.answers(&[])?;
    if started || check_start_id(&check).is_some() {
        return Err(format!(
            "exact check: neither may start:\n{check}\n{repair}"
        ));
    }
    if check.contains(CONTENTS_CHECK) || repair.contains(CONTENTS_CHECK) {
        return Err(format!(
            "exact check: the contents-check refusal must not apply:\n{check}\n{repair}"
        ));
    }
    Ok(())
}

/// Neither command starts and neither names the contents check: the
/// count-only rule does not apply.
fn assert_rule_does_not_apply(case: &str, repo: &Repo) -> Result<(), String> {
    let (check, started, repair) = repo.answers(&[])?;
    if started || check_start_id(&check).is_some() {
        return Err(format!("{case}: neither may start:\n{check}\n{repair}"));
    }
    if check.contains(CONTENTS_CHECK) || repair.contains(CONTENTS_CHECK) {
        return Err(format!(
            "{case}: the count-only rule must not apply:\n{check}\n{repair}"
        ));
    }
    Ok(())
}

#[test]
fn async_owner_is_not_followed() -> Result<(), String> {
    // The test never polls the returned future, so nothing is pushed.
    let repo = Repo::create(
        &OLD.replace("pub fn", "pub async fn"),
        &NEW.replace("pub fn", "pub async fn"),
        &test_file(COUNT_BODY),
    )?;
    assert_rule_does_not_apply("async owner", &repo)
}

#[test]
fn final_contents_check_without_semicolon_is_not_this_rules_to_grade() -> Result<(), String> {
    // The last check is the body's tail expression and reads the contents.
    let repo = Repo::create(
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert_eq!(b.len(), 1);\n    assert_eq!(b[0], \"HI\")\n",
        ),
    )?;
    assert_rule_does_not_apply("tail contents check", &repo)
}

#[test]
fn stub_writes_an_exact_contents_check_of_the_pushed_buffer() -> Result<(), String> {
    let repo = Repo::create(OLD, NEW, &test_file(COUNT_BODY))?;
    let stub = text(&repo.ripr(&[
        "agent",
        "stub",
        "--root",
        ".",
        "--at",
        "src/lib.rs:2",
        "--kind",
        "side_effect",
    ])?);
    for wanted in [
        "let mut buf: Vec<String> = Vec::new();",
        "log_line(&mut buf, msg);",
        "assert_eq!(buf, expected_buf);",
        "write the full contents `buf` should hold after `log_line` runs",
    ] {
        if !stub.contains(wanted) {
            return Err(format!("stub must contain `{wanted}`:\n{stub}"));
        }
    }
    if stub.contains("mock") || stub.contains("changed comparison") {
        return Err(format!(
            "stub must not suggest a mock or a comparison input:\n{stub}"
        ));
    }
    Ok(())
}

#[test]
fn stub_for_a_push_into_a_local_vector_still_needs_an_observer() -> Result<(), String> {
    // The vector is the function's own local, not a buffer the caller holds.
    let old = "pub fn count(msg: &str) -> usize {\n    let mut v = Vec::new();\n    v.push(msg.to_string());\n    v.len()\n}\n";
    let repo = Repo::create(
        old,
        &old.replace("msg.to_string()", "msg.to_uppercase()"),
        "use fx::count;\n\n#[test]\nfn counts_one() {\n    assert_eq!(count(\"hi\"), 1);\n}\n",
    )?;
    let stub = text(&repo.ripr(&[
        "agent",
        "stub",
        "--root",
        ".",
        "--at",
        "src/lib.rs:3",
        "--kind",
        "side_effect",
    ])?);
    if stub.contains("assert_eq!(v,") || !stub.contains("observer or mock") {
        return Err(format!(
            "a local vector's push keeps the observer refusal:\n{stub}"
        ));
    }
    Ok(())
}

#[test]
fn inline_count_read_with_no_editable_test_starts_in_neither_command() -> Result<(), String> {
    // The same pinning count read in the owner file's own test module. ripr
    // cannot yet place a new test for a changed effect there, so bare repair
    // refuses with no test route, and check must not offer that start.
    let inline = format!(
        "\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn log_line_appends() {{\n    {}    }}\n}}\n",
        COUNT_BODY.replace("\n    ", "\n        ")
    );
    for check_args in [
        &[][..],
        &["--no-unchanged-tests"][..],
        &["--mode", "instant"][..],
    ] {
        let repo = Repo::create(&format!("{OLD}{inline}"), &format!("{NEW}{inline}"), "")?;
        let (check, started, repair) = repo.answers(check_args)?;
        if started || !repair.contains("refusal code=no_test_route") {
            return Err(format!(
                "inline {check_args:?}: bare repair must refuse with no test route:\n{repair}"
            ));
        }
        if let Some(id) = check_start_id(&check) {
            return Err(format!(
                "inline {check_args:?}: check must not offer `ripr repair {id}`:\n{check}"
            ));
        }
    }
    Ok(())
}

/// The stub output for the push at `src/lib.rs:line` with `src` on both sides
/// of a one-word change.
fn side_effect_stub(src: &str, line: usize) -> Result<String, String> {
    let repo = Repo::create(
        src,
        &src.replace("msg.to_string()", "msg.to_uppercase()"),
        &test_file(COUNT_BODY),
    )?;
    Ok(text(&repo.ripr(&[
        "agent",
        "stub",
        "--root",
        ".",
        "--at",
        &format!("src/lib.rs:{line}"),
        "--kind",
        "side_effect",
    ])?))
}

#[test]
fn stub_for_a_conditional_push_into_the_buffer_still_needs_an_observer() -> Result<(), String> {
    // The push may not run, so the owner is not the one-push shape.
    let stub = side_effect_stub(
        "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    if !msg.is_empty() {\n        buf.push(msg.to_string());\n    }\n}\n",
        3,
    )?;
    if stub.contains("assert_eq!(buf,") || !stub.contains("observer or mock") {
        return Err(format!(
            "a conditional push keeps the observer refusal:\n{stub}"
        ));
    }
    Ok(())
}

#[test]
fn stub_for_a_local_that_shadows_the_buffer_still_needs_an_observer() -> Result<(), String> {
    // The push lands in a fresh local named like the parameter, not in the
    // caller's buffer.
    let stub = side_effect_stub(
        "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    let buf = &mut Vec::new();\n    buf.push(msg.to_string());\n}\n",
        3,
    )?;
    if stub.contains("assert_eq!(buf,") || !stub.contains("observer or mock") {
        return Err(format!(
            "a shadowing local keeps the observer refusal:\n{stub}"
        ));
    }
    Ok(())
}

/// The brief bare `ripr repair` prints for the started gap: from its
/// "Repair prepared" summary to the end of its output.
fn started_brief(repo: &Repo) -> Result<String, String> {
    let (_, started, repair) = repo.answers(&[])?;
    if !started {
        return Err(format!("repair must start:\n{repair}"));
    }
    repair
        .find("Repair prepared")
        .map(|start| repair[start..].to_string())
        .ok_or_else(|| format!("no repair brief:\n{repair}"))
}

#[test]
fn repair_brief_names_the_contents_check_for_a_pinned_push() -> Result<(), String> {
    // #12711: the brief suggests the check the stub writes, and the new
    // test's suggested name and the stub's name agree and do not say
    // "observer".
    let repo = Repo::create(OLD, NEW, &test_file(COUNT_BODY))?;
    let brief = started_brief(&repo)?;
    for wanted in [
        "Suggested assertion: an exact check of the full contents of `buf` after the call, such as `assert_eq!(buf, expected_buf)`",
        "(suggested test `log_line_full_contents`)",
    ] {
        if !brief.contains(wanted) {
            return Err(format!("brief must contain {wanted:?}:\n{brief}"));
        }
    }
    if brief.contains("Suggested assertion: none") || brief.contains("observer") {
        return Err(format!("brief must not say none or observer:\n{brief}"));
    }
    let stub = text(&repo.ripr(&[
        "agent",
        "stub",
        "--root",
        ".",
        "--at",
        "src/lib.rs:2",
        "--kind",
        "side_effect",
    ])?);
    if !stub.contains("fn log_line_full_contents()")
        || !stub.contains("assert_eq!(buf, expected_buf)")
    {
        return Err(format!(
            "stub must write the same named contents check:\n{stub}"
        ));
    }
    Ok(())
}

#[test]
fn repair_brief_for_other_gaps_is_unchanged() -> Result<(), String> {
    // #12711: a started boundary gap keeps its own suggested assertion and
    // test name.
    let repo = Repo::create(
        "pub fn discounted(total: u32) -> u32 {\n    if total >= 100 { total - 10 } else { total }\n}\n",
        "pub fn discounted(total: u32) -> u32 {\n    if total > 100 { total - 10 } else { total }\n}\n",
        "use fx::discounted;\n\n#[test]\nfn discount_applies() {\n    assert_eq!(discounted(200), 190);\n}\n",
    )?;
    let brief = started_brief(&repo)?;
    for wanted in [
        "Suggested assertion: assert_eq!(discounted(",
        "(suggested test `discounted_boundary_discriminator`)",
    ] {
        if !brief.contains(wanted) {
            return Err(format!("boundary brief must contain {wanted:?}:\n{brief}"));
        }
    }
    if brief.contains("an exact check of the full contents") || brief.contains("full_contents") {
        return Err(format!(
            "boundary brief must not name a contents check:\n{brief}"
        ));
    }
    // A push whose count read does not pin it gets no contents check from
    // the brief either: the route does not propose one, so nothing starts.
    let unpinned = Repo::create(
        OLD,
        NEW,
        &test_file(
            "    let mut b = Vec::new();\n    log_line(&mut b, \"hi\");\n    assert_ne!(b.len(), 2);\n",
        ),
    )?;
    let (_, started, repair) = unpinned.answers(&[])?;
    if started
        || repair.contains("an exact check of the full contents")
        || repair.contains("full_contents")
    {
        return Err(format!(
            "an unpinned push must not start or name a contents check:\n{repair}"
        ));
    }
    Ok(())
}
