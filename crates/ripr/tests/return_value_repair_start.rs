//! #10701: a weak test on a changed return value (arithmetic, string, async,
//! `Err` variant) reaches a repair start with a test edit target, and the
//! cases that cannot be made sound state one plain-language reason in both
//! `ripr check` and `ripr repair` with the working manual route.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const CARGO_TOML: &str = "[package]\nname = \"ar\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

struct Repo(PathBuf);

impl Repo {
    /// A two-commit repository: `old_src` + `test` committed, then `new_src`.
    fn create(old_src: &str, new_src: &str, test: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-return-value-repair-{}-{}",
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
        repo.git(&["init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
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
        let output = self.ripr(&["check", "--base", "HEAD~1"])?;
        Ok(text(&output))
    }

    fn repair_text(&self, extra: &[&str]) -> Result<(bool, String), String> {
        let mut args = vec!["repair", "--root", "."];
        args.extend_from_slice(extra);
        let output = self.ripr(&args)?;
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

/// The `ripr repair <id>` seam id the check screen printed, if any.
fn printed_repair_seam(check: &str) -> Option<String> {
    check.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_string())
    })
}

/// Weak changed-return cases: `check` prints a repair start for the seam and
/// `repair` prepares it with a test-file edit target.
fn assert_repair_ready(case: &str, old_src: &str, new_src: &str, test: &str) -> Result<(), String> {
    let repo = Repo::create(old_src, new_src, test)?;
    let check = repo.check_text()?;
    assert!(
        check.contains("Static exposure: weak"),
        "{case}: precondition, check must report a weak finding:\n{check}"
    );
    let seam = printed_repair_seam(&check)
        .ok_or_else(|| format!("{case}: check printed no repair start:\n{check}"))?;
    let (ok, repair) = repo.repair_text(&[&seam])?;
    assert!(ok, "{case}: repair must start:\n{repair}");
    assert!(
        repair.contains("repair_ready=true (missing evidence: none)"),
        "{case}: {repair}"
    );
    assert!(
        repair.contains("edit one test file: tests/t.rs"),
        "{case}: repair must name the test edit target:\n{repair}"
    );
    assert!(!repair.contains("producer-owned"), "{case}: {repair}");
    Ok(())
}

#[test]
fn arithmetic_return_with_relational_assert_reaches_repair() -> Result<(), String> {
    assert_repair_ready(
        "arithmetic",
        "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use ar::dbl;\n\n#[test]\nfn dbl_positive() {\n    assert!(dbl(2) > 0);\n}\n",
    )
}

#[test]
fn string_return_with_weak_assert_reaches_repair() -> Result<(), String> {
    assert_repair_ready(
        "string",
        "pub fn greet(n: &str) -> String {\n    format!(\"hello {}\", n)\n}\n",
        "pub fn greet(n: &str) -> String {\n    format!(\"hi {}\", n)\n}\n",
        "use ar::greet;\n\n#[test]\nfn greets() {\n    assert!(!greet(\"bob\").is_empty());\n}\n",
    )
}

#[test]
fn awaited_async_return_with_weak_assert_reaches_repair() -> Result<(), String> {
    assert_repair_ready(
        "async",
        "pub async fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub async fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use ar::dbl;\n\n#[tokio::test]\nasync fn dbl_positive() {\n    assert!(dbl(2).await > 0);\n}\n",
    )
}

#[test]
fn err_variant_with_is_err_assert_reaches_repair() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn parse(x: i32) -> Result<i32, String> {\n    if x < 0 {\n        return Err(\"negative\".to_string());\n    }\n    Ok(x)\n}\n",
        "pub fn parse(x: i32) -> Result<i32, String> {\n    if x < 0 {\n        return Err(\"bad\".to_string());\n    }\n    Ok(x)\n}\n",
        "use ar::parse;\n\n#[test]\nfn rejects() {\n    assert!(parse(-1).is_err());\n}\n",
    )?;
    let check = repo.check_text()?;
    let seam = printed_repair_seam(&check)
        .ok_or_else(|| format!("check printed no repair start:\n{check}"))?;
    let (ok, repair) = repo.repair_text(&[&seam])?;
    assert!(ok, "{repair}");
    assert!(
        repair.contains("repair_ready=true (missing evidence: none)")
            && repair.contains("edit one test file: tests/t.rs"),
        "{repair}"
    );
    Ok(())
}

/// Controls that must stay fail-closed: both commands say why in plain
/// words, never with the internal evidence name, and name the manual route.
fn assert_plain_reason(case: &str, old_src: &str, new_src: &str, test: &str) -> Result<(), String> {
    let repo = Repo::create(old_src, new_src, test)?;
    let check = repo.check_text()?;
    let (ok, repair) = repo.repair_text(&[])?;
    assert!(!ok, "{case}: repair must not start:\n{repair}");
    for (name, output) in [("check", &check), ("repair", &repair)] {
        assert!(
            !output.contains("producer-owned") && !output.contains("discriminator fact"),
            "{case}: {name} leaks internal wording:\n{output}"
        );
    }
    let plain = "a test assertion that checks the changed return value";
    assert!(
        repair.contains(plain),
        "{case}: repair must state the plain reason:\n{repair}"
    );
    assert!(
        repair.contains("--kind return_value") && repair.contains("write the test by hand"),
        "{case}: repair must name the working route:\n{repair}"
    );
    assert!(
        !check.contains("Static exposure: weak"),
        "{case}: a weak finding with a plain reason belongs to the repair-ready matrix:\n{check}"
    );
    Ok(())
}

#[test]
fn return_value_with_no_test_gives_one_plain_reason_in_both_commands() -> Result<(), String> {
    let repo = Repo::create(
        "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "// no tests\n",
    )?;
    let check = repo.check_text()?;
    let plain = "a test assertion that checks the changed return value";
    assert!(
        check.contains(plain),
        "check must state the reason:\n{check}"
    );
    assert!(
        check.contains("agent stub") && check.contains("--kind return_value"),
        "check must name the stub route:\n{check}"
    );
    let (ok, repair) = repo.repair_text(&[])?;
    assert!(!ok, "{repair}");
    for output in [&check, &repair] {
        assert!(!output.contains("producer-owned"), "{output}");
    }
    assert!(
        repair.contains(plain)
            && repair.contains("--kind return_value")
            && repair.contains("write the test by hand"),
        "{repair}"
    );
    Ok(())
}

#[test]
fn executor_helper_async_call_stays_fail_closed_with_a_plain_reason() -> Result<(), String> {
    assert_plain_reason(
        "async-executor-helper",
        "pub async fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub async fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use ar::dbl;\n\n#[test]\nfn dbl_positive() {\n    let v = block_on(dbl(2));\n    assert!(v > 0);\n}\nfn block_on<T>(_f: impl std::future::Future<Output = T>) -> T {\n    unimplemented!()\n}\n",
    )
}

const DBL_OLD: &str = "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n";
const DBL_NEW: &str = "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n";

/// A case that must not start a repair: `check` prints no `ripr repair <id>`
/// start and an unfiltered `repair` refuses, never claiming `repair_ready`.
fn assert_no_start(case: &str, old_src: &str, new_src: &str, test: &str) -> Result<Repo, String> {
    let repo = Repo::create(old_src, new_src, test)?;
    let check = repo.check_text()?;
    assert!(
        printed_repair_seam(&check).is_none(),
        "{case}: check must not print a repair start:\n{check}"
    );
    let (ok, repair) = repo.repair_text(&[])?;
    assert!(!ok, "{case}: repair must refuse:\n{repair}");
    assert!(
        !repair.contains("repair_ready=true"),
        "{case}: repair must not be ready:\n{repair}"
    );
    Ok(repo)
}

/// The seam id `check` prints for the weak `dbl` change, used to address the
/// same seam when other tests make it strong.
fn weak_dbl_seam() -> Result<String, String> {
    let repo = Repo::create(
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn dbl_positive() {\n    assert!(dbl(2) > 0);\n}\n",
    )?;
    let check = repo.check_text()?;
    printed_repair_seam(&check).ok_or_else(|| format!("no weak repair start:\n{check}"))
}

#[test]
fn strong_exact_assert_is_not_a_repair_start() -> Result<(), String> {
    let seam = weak_dbl_seam()?;
    let repo = assert_no_start(
        "exact",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn exact() {\n    assert_eq!(dbl(2), 6);\n}\n",
    )?;
    // Address the very seam the weak case started: the refusal must come from
    // the seam being pinned, not from nothing being selected.
    let (ok, repair) = repo.repair_text(&[&seam])?;
    assert!(!ok, "{repair}");
    assert!(
        repair.contains(&seam),
        "refusal must name the seam: {repair}"
    );
    assert!(!repair.contains("repair_ready=true"), "{repair}");
    Ok(())
}

#[test]
fn equality_assert_pinning_the_owner_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start(
        "assert-eq-operator",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn pinned() {\n    assert!(dbl(2) == 6);\n}\n",
    )?;
    Ok(())
}

#[test]
fn terminal_err_return_guard_pinning_the_owner_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start(
        "err-return-guard",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn guarded() -> Result<(), String> {\n    if dbl(2) != 6 {\n        return Err(format!(\"dbl(2) was {}\", dbl(2)));\n    }\n    Ok(())\n}\n",
    )?;
    Ok(())
}

#[test]
fn function_value_reach_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start(
        "function-value",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn mapped() {\n    let v: Vec<i32> = vec![1, 2].into_iter().map(dbl).collect();\n    assert!(!v.is_empty());\n}\n",
    )?;
    Ok(())
}

#[test]
fn statically_contradicted_assertion_is_not_a_repair_start() -> Result<(), String> {
    // `dbl(2)` is 6 after the change, so `== 5` fails today: the failing test
    // is not evidence for repair to strengthen.
    assert_no_start(
        "contradicted",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn wrong() {\n    assert_eq!(dbl(2), 5);\n}\n",
    )?;
    Ok(())
}

#[test]
fn indirect_call_without_a_direct_owner_call_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start(
        "no-direct-call",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\nfn helper() -> i32 {\n    dbl(2)\n}\n\n#[test]\nfn via_helper() {\n    assert!(helper() > 0);\n}\n",
    )?;
    Ok(())
}

const ASYNC_OLD: &str = "pub async fn dbl(x: i32) -> i32 {\n    x * 2\n}\n";
const ASYNC_NEW: &str = "pub async fn dbl(x: i32) -> i32 {\n    x * 3\n}\n";

/// The executor-helper control differs from the awaited positive in exactly
/// one respect (`block_on(dbl(2))` versus `dbl(2).await`), so only the
/// indirect-reach check can explain the different outcomes.
#[test]
fn executor_helper_differs_from_the_awaited_call_only_by_indirect_reach() -> Result<(), String> {
    assert_repair_ready(
        "awaited-twin",
        ASYNC_OLD,
        ASYNC_NEW,
        "use ar::dbl;\n\n#[tokio::test]\nasync fn dbl_positive() {\n    let v = dbl(2).await;\n    assert!(v > 0);\n}\n",
    )?;
    assert_plain_reason(
        "executor-twin",
        ASYNC_OLD,
        ASYNC_NEW,
        "use ar::dbl;\n\n#[tokio::test]\nasync fn dbl_positive() {\n    let v = block_on(dbl(2));\n    assert!(v > 0);\n}\nfn block_on<T>(_f: impl std::future::Future<Output = T>) -> T {\n    unimplemented!()\n}\n",
    )
}
