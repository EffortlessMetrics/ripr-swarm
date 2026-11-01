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
        // On Windows a sibling test that rebuilds the binary can leave it
        // briefly missing, which surfaces as NotFound at launch; retry only that.
        let mut attempt = 0;
        loop {
            let launched = Command::new(env!("CARGO_BIN_EXE_ripr"))
                .current_dir(&self.0)
                .args(args)
                .output();
            match launched {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && attempt < 20 => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                other => return other.map_err(|e| e.to_string()),
            }
        }
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
    assert!(repair.contains("Ready to repair."), "{case}: {repair}");
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
        repair.contains("Ready to repair.") && repair.contains("edit one test file: tests/t.rs"),
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
    // Both callers change an async owner, which `ripr agent stub` refuses:
    // the route is a hand-written test, and the refusal is named instead of
    // a stub command that would refuse (#11111 burn-down).
    assert!(
        repair.contains("write the test by hand")
            && repair
                .contains("ripr cannot write a fill-in skeleton here because the owner is async")
            && !repair.contains("--kind return_value"),
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
        !repair.contains("Ready to repair."),
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
    // Address the seam the weak case started, so the refusal is the pin's.
    let (ok, repair) = repo.repair_text(&[&seam])?;
    assert!(!ok, "{repair}");
    assert!(
        repair.contains(&seam),
        "refusal must name the seam: {repair}"
    );
    assert!(!repair.contains("Ready to repair."), "{repair}");
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

/// Differs from the awaited positive only by `block_on(dbl(2))` versus
/// `dbl(2).await`, so only the indirect-reach check explains the outcome.
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

const GUARD_TEST: &str = "use ar::dbl;\n\n#[test]\nfn guarded() -> Result<(), String> {\n    if dbl(2) != 6 {\n        return Err(\"mismatch\".to_string());\n    }\n    Ok(())\n}\n";
const SHADOWED_GUARD_TEST: &str = "use ar::dbl;\n\nfn Err(_: String) -> Result<(), String> {\n    Ok(())\n}\n\n#[test]\nfn guarded() -> Result<(), String> {\n    if dbl(2) != 6 {\n        return Err(\"mismatch\".to_string());\n    }\n    Ok(())\n}\n";

/// Differs only by a value-namespace `Err` that makes the guard an ordinary
/// statement (RIPR-SPEC-0197): the prelude guard pins the owner, the shadow not.
#[test]
fn shadowed_err_guard_differs_from_the_prelude_guard_only_by_the_shadow() -> Result<(), String> {
    assert_no_start("prelude-guard", DBL_OLD, DBL_NEW, GUARD_TEST)?;
    assert_repair_ready("shadowed-guard", DBL_OLD, DBL_NEW, SHADOWED_GUARD_TEST)
}

/// A statically limited ("propagation unknown") finding must not start a
/// repair even with a weak seam-level oracle; runs the real fixture.
#[test]
fn statically_limited_finding_keeps_its_refusal_and_manual_route() -> Result<(), String> {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observation_unverified_field_construction");
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--mode", "fast", "--root"])
        .arg(fixture.join("input"))
        .arg("--diff")
        .arg(fixture.join("diff.patch"))
        .output()
        .map_err(|e| e.to_string())?;
    let check = text(&output);
    assert!(
        check.contains("propagation unknown"),
        "precondition, the finding is statically limited:\n{check}"
    );
    assert!(
        printed_repair_seam(&check).is_none(),
        "a limited finding must not print a repair start:\n{check}"
    );
    assert!(
        check.contains("no repair start offered")
            && check.contains("a test assertion that checks the changed return value"),
        "the refusal must stay, with its plain reason:\n{check}"
    );
    Ok(())
}

/// Function-value reach beside a direct call: only the indirect-reach rule
/// explains the refusal.
#[test]
fn function_value_reach_beside_a_direct_call_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "direct-twin",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn mapped() {\n    let first = dbl(1);\n    let v: Vec<i32> = vec![first].into_iter().collect();\n    assert!(!v.is_empty());\n}\n",
    )?;
    assert_no_start(
        "function-value-beside-direct",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn mapped() {\n    let first = dbl(1);\n    let v: Vec<i32> = vec![first].into_iter().map(dbl).collect();\n    assert!(!v.is_empty());\n}\n",
    )?;
    Ok(())
}

/// A weak test beside a contradicted exact assertion: only the contradiction
/// rule explains the refusal.
#[test]
fn contradicted_assertion_beside_a_weak_test_is_not_a_repair_start() -> Result<(), String> {
    let weak = "#[test]\nfn weak() {\n    assert!(dbl(2) > 0);\n}\n";
    assert_repair_ready(
        "weak-alone",
        DBL_OLD,
        DBL_NEW,
        &format!("use ar::dbl;\n\n{weak}"),
    )?;
    assert_no_start(
        "weak-beside-contradicted",
        DBL_OLD,
        DBL_NEW,
        &format!(
            "use ar::dbl;\n\n{weak}\n#[test]\nfn wrong() {{\n    assert_eq!(dbl(2), 5);\n}}\n"
        ),
    )?;
    Ok(())
}

const CONFIG_OLD: &str = "pub struct Config {\n    pub timeout_secs: u32,\n    pub retries: u32,\n}\n\npub fn default_config() -> Config {\n    let retries = 3;\n    Config {\n        timeout_secs: 30,\n        retries: 1,\n    }\n}\n";
const CONFIG_NEW: &str = "pub struct Config {\n    pub timeout_secs: u32,\n    pub retries: u32,\n}\n\npub fn default_config() -> Config {\n    let retries = 3;\n    Config {\n        timeout_secs: 30,\n        retries,\n    }\n}\n";

/// `ripr repair` agrees with `ripr check` on the struct-field change of
/// `observation_unverified_field_construction`: a weak assertion on a sibling
/// field is not a repair start, bare or by location, and the manual stub route
/// stays named.
#[test]
fn repair_agrees_with_check_on_a_statically_limited_struct_field_finding() -> Result<(), String> {
    let repo = assert_no_start(
        "field-construction",
        CONFIG_OLD,
        CONFIG_NEW,
        "use ar::default_config;\n\n#[test]\nfn config_exists() {\n    let cfg = default_config();\n    assert!(cfg.timeout_secs > 0);\n}\n",
    )?;
    {
        let item = "src/lib.rs:8";
        let (ok, repair) = repo.repair_text(&[item])?;
        assert!(!ok, "{item}: repair must refuse:\n{repair}");
        assert!(
            !repair.contains("Ready to repair."),
            "{item}: repair must not be ready:\n{repair}"
        );
        assert!(
            repair.contains("--kind return_value"),
            "{item}: the manual stub route must stay named:\n{repair}"
        );
    }
    Ok(())
}

/// A test that only discards the owner's result has no assertion, so
/// propagation stays unknown and no repair start is offered.
#[test]
fn discarded_only_test_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start(
        "discarded-only",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn discarded() {\n    let _ = dbl(2);\n}\n",
    )?;
    Ok(())
}

fn dbl_test(body: &str) -> String {
    format!("use ar::dbl;\n\n#[test]\nfn t() {{\n{body}}}\n")
}

/// A test that never runs (`#[ignore]`) or passes by panicking
/// (`#[should_panic]`) is not repairable; each differs only by its attribute.
#[test]
fn ignored_and_should_panic_tests_are_not_a_repair_start() -> Result<(), String> {
    let body = "    assert!(dbl(2) > 0);\n";
    assert_repair_ready("plain-twin", DBL_OLD, DBL_NEW, &dbl_test(body))?;
    for attribute in [
        "#[ignore]",
        "#[should_panic]",
        "#[should_panic(expected = \"x\")]",
    ] {
        let test = format!("use ar::dbl;\n\n#[test]\n{attribute}\nfn t() {{\n{body}}}\n");
        assert_no_start(attribute, DBL_OLD, DBL_NEW, &test)?;
    }
    Ok(())
}

/// `let v = dbl(2); assert!(v == 6)` pins the value as `assert!(dbl(2) == 6)`
/// does, so repair withholds; a weak assertion on the same local still starts.
#[test]
fn equality_through_a_local_binding_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "local-weak-twin",
        DBL_OLD,
        DBL_NEW,
        &dbl_test("    let v = dbl(2);\n    assert!(v > 0);\n"),
    )?;
    assert_no_start(
        "local-equality",
        DBL_OLD,
        DBL_NEW,
        &dbl_test("    let v = dbl(2);\n    assert!(v == 6);\n"),
    )?;
    Ok(())
}

/// A strong non-equality oracle (`matches!`) already grips the exact value, so
/// the weak twin starts a repair and the `matches!` one does not; only the
/// weak-or-absent discriminator gate separates them.
#[test]
fn strong_non_equality_oracle_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "weak-twin",
        DBL_OLD,
        DBL_NEW,
        &dbl_test("    assert!(dbl(2) > 0);\n"),
    )?;
    assert_no_start(
        "matches-exact",
        DBL_OLD,
        DBL_NEW,
        &dbl_test("    assert!(matches!(dbl(2), 6));\n"),
    )?;
    Ok(())
}

/// An executor-wrapped call beside a direct call: the direct call alone starts
/// a repair, so only the executor-wrap rule explains the refusal.
#[test]
fn executor_wrapped_call_beside_a_direct_call_is_not_a_repair_start() -> Result<(), String> {
    let test = |second: &str| {
        format!(
            "use ar::dbl;\n\n#[tokio::test]\nasync fn t() {{\n    let first = dbl(1).await;\n    let v = {second};\n    assert!(v > first);\n}}\nfn block_on<T>(_f: impl std::future::Future<Output = T>) -> T {{\n    unimplemented!()\n}}\n"
        )
    };
    assert_repair_ready("direct-twin", ASYNC_OLD, ASYNC_NEW, &test("dbl(2).await"))?;
    assert_no_start(
        "executor-beside-direct",
        ASYNC_OLD,
        ASYNC_NEW,
        &test("block_on(dbl(2))"),
    )?;
    Ok(())
}

/// A test whose call result is discarded with no assertion has nothing to
/// strengthen yet: reach, activation and propagation are not all established,
/// so the weak twin starts a repair and the bare call does not.
#[test]
fn bare_call_without_an_assertion_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "asserted-twin",
        DBL_OLD,
        DBL_NEW,
        &dbl_test("    assert!(dbl(2) > 0);\n"),
    )?;
    assert_no_start("bare-call", DBL_OLD, DBL_NEW, &dbl_test("    dbl(2);\n"))?;
    Ok(())
}

const PARSE_OLD: &str = "pub enum E {\n    A,\n    B,\n}\n\npub fn parse(_x: i32) -> Result<i32, E> {\n    Err(E::A)\n}\n";
const PARSE_NEW: &str = "pub enum E {\n    A,\n    B,\n}\n\npub fn parse(_x: i32) -> Result<i32, E> {\n    Err(E::B)\n}\n";

/// A variant-exact `matches!` on the changed error already grips the variant;
/// the weak `is_err()` twin starts a repair and this one does not.
#[test]
fn variant_exact_error_assertion_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "is-err-twin",
        PARSE_OLD,
        PARSE_NEW,
        "use ar::parse;\n\n#[test]\nfn t() {\n    assert!(parse(-1).is_err());\n}\n",
    )?;
    assert_no_start(
        "matches-variant",
        PARSE_OLD,
        PARSE_NEW,
        "use ar::{parse, E};\n\n#[test]\nfn t() {\n    assert!(matches!(parse(-1), Err(E::B)));\n}\n",
    )?;
    Ok(())
}

fn loud_lib(extra: &str, ret: &str, factor: u32) -> String {
    format!(
        "pub struct Loud(pub u32);\nimpl Drop for Loud {{\n    fn drop(&mut self) {{}}\n}}\n{extra}fn make(n: u32) -> Loud {{\n    Loud(n)\n}}\npub fn mk(x: u32) -> {ret} {{\n    make(x * {factor})\n}}\n"
    )
}

const LOUD_TEST: &str = "use ar::mk;\n\n#[test]\nfn t() {\n    assert!(mk(2).0 > 0);\n}\n";

/// A returned type that is an alias cannot be resolved to its `Drop` status,
/// and a workspace holding a macro may generate a `Drop` impl no scan reads:
/// each withholds, differing from the repairable plain twin by one line.
#[test]
fn unresolved_alias_or_macro_workspace_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "plain-drop-twin",
        &loud_lib("", "Loud", 2),
        &loud_lib("", "Loud", 3),
        LOUD_TEST,
    )?;
    assert_no_start(
        "alias",
        &loud_lib("pub type Out = Loud;\n", "Out", 2),
        &loud_lib("pub type Out = Loud;\n", "Out", 3),
        LOUD_TEST,
    )?;
    // The same alias spelled across lines (and with a comment) is the same
    // alias: it must not slip past a substring match.
    for (spelling, alias) in [
        ("newline", "pub type\nOut = Loud;\n"),
        ("comment", "type /* alias */ Out\n    = Loud;\n"),
    ] {
        assert_no_start(
            spelling,
            &loud_lib(alias, "Out", 2),
            &loud_lib(alias, "Out", 3),
            LOUD_TEST,
        )?;
    }
    let with_macro = |factor| loud_lib("macro_rules! unused {\n    () => {};\n}\n", "Loud", factor);
    assert_no_start("macro", &with_macro(2), &with_macro(3), LOUD_TEST)?;
    Ok(())
}

/// Equality and closed-range pins over a value derived from the owner call
/// withhold a start; each differs from the weak control only by the shape.
#[test]
fn derived_value_equality_and_range_pins_are_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "weak-control",
        DBL_OLD,
        DBL_NEW,
        &dbl_test("    let v = dbl(2);\n    assert!(v >= 1);\n"),
    )?;
    for (name, body) in [
        (
            "transitive-local",
            "    let v = dbl(2);\n    let w = v;\n    assert!(w == 6);\n",
        ),
        ("eq-method", "    assert!(dbl(2).eq(&6));\n"),
        (
            "negated-inequality",
            "    let v = dbl(2);\n    assert!(!(v != 6));\n",
        ),
        (
            "closed-range",
            "    let v = dbl(2);\n    assert!(v >= 6 && v <= 6);\n",
        ),
    ] {
        assert_no_start(name, DBL_OLD, DBL_NEW, &dbl_test(body))?;
    }
    Ok(())
}

const PAIR_OLD: &str = "pub fn pair(x: i32) -> (i32, i32) {\n    (x * 2, 1)\n}\n";
const PAIR_NEW: &str = "pub fn pair(x: i32) -> (i32, i32) {\n    (x * 3, 1)\n}\n";

/// A tuple destructure of the owner call binds names that pin the value too.
#[test]
fn tuple_destructure_equality_is_not_a_repair_start() -> Result<(), String> {
    let test = |assertion: &str| {
        format!(
            "use ar::pair;\n\n#[test]\nfn t() {{\n    let (a, _b) = pair(2);\n    {assertion}\n}}\n"
        )
    };
    assert_repair_ready(
        "tuple-weak-control",
        PAIR_OLD,
        PAIR_NEW,
        &test("assert!(a > 0);"),
    )?;
    assert_no_start(
        "tuple-equality",
        PAIR_OLD,
        PAIR_NEW,
        &test("assert!(a == 6);"),
    )?;
    Ok(())
}

/// An exact-variant assertion for the OLD variant is contradicted by the
/// change, so a weak `is_err()` beside it is not a start.
#[test]
fn contradicted_error_variant_assertion_is_not_a_repair_start() -> Result<(), String> {
    assert_repair_ready(
        "is-err-only",
        PARSE_OLD,
        PARSE_NEW,
        "use ar::parse;\n\n#[test]\nfn t() {\n    assert!(parse(-1).is_err());\n}\n",
    )?;
    assert_no_start(
        "old-variant-beside-is-err",
        PARSE_OLD,
        PARSE_NEW,
        "use ar::{parse, E};\n\n#[test]\nfn t() {\n    assert!(parse(-1).is_err());\n    assert!(matches!(parse(-1), Err(E::A)));\n}\n",
    )?;
    // A variant whose name only starts with the new one (`AB` for `A`) is
    // still another variant: one extra letter flips the outcome.
    let prefixed = |old: &str, new: &str| {
        let lib = |variant: &str| {
            format!(
                "pub enum E {{\n    A,\n    AB,\n}}\n\npub fn parse(_x: i32) -> Result<i32, E> {{\n    Err(E::{variant})\n}}\n"
            )
        };
        (lib(old), lib(new))
    };
    let (old, new) = prefixed("AB", "A");
    assert_no_start(
        "prefix-collision",
        &old,
        &new,
        "use ar::{parse, E};\n\n#[test]\nfn t() {\n    assert!(parse(-1).is_err());\n    assert!(matches!(parse(-1), Err(E::AB)));\n}\n",
    )?;
    assert_repair_ready(
        "prefix-weak-control",
        &old,
        &new,
        "use ar::parse;\n\n#[test]\nfn t() {\n    assert!(parse(-1).is_err());\n}\n",
    )?;
    Ok(())
}

/// A rename of, a wrapper around, or a generic stand-in for a non-plain `Drop`
/// type withholds; each differs from a plain-`Drop` (or concrete) twin that
/// starts by one line.
#[test]
fn renamed_wrapped_or_generic_non_plain_drop_is_not_a_repair_start() -> Result<(), String> {
    let loud = |drop_body: &str, middle: &str, ret: &str, factor: u32| {
        format!(
            "pub struct Loud(pub u32);\nimpl Drop for Loud {{\n    fn drop(&mut self) {{{drop_body}}}\n}}\n{middle}fn make(n: u32) -> Loud {{\n    Loud(n)\n}}\npub fn mk(x: u32) -> {ret} {{\n    {body}\n}}\n",
            body = if ret == "Wrapper" {
                format!("Wrapper(make(x * {factor}))")
            } else {
                format!("make(x * {factor})")
            }
        )
    };
    let exit = " std::process::exit(0); ";
    let test = "use ar::mk;\n\n#[test]\nfn t() {\n    assert!(mk(2).0 > 0);\n}\n";
    let wrapper_test = "use ar::mk;\n\n#[test]\nfn t() {\n    assert!(mk(2).0.0 > 0);\n}\n";
    let rename = "use crate::Loud as Quiet;\n";
    let wrapper = "pub struct Wrapper(pub Loud);\n";
    assert_repair_ready(
        "rename-plain-twin",
        &loud("", rename, "Quiet", 2),
        &loud("", rename, "Quiet", 3),
        test,
    )?;
    assert_no_start(
        "rename",
        &loud(exit, rename, "Quiet", 2),
        &loud(exit, rename, "Quiet", 3),
        test,
    )?;
    assert_repair_ready(
        "wrapper-plain-twin",
        &loud("", wrapper, "Wrapper", 2),
        &loud("", wrapper, "Wrapper", 3),
        wrapper_test,
    )?;
    assert_no_start(
        "wrapper",
        &loud(exit, wrapper, "Wrapper", 2),
        &loud(exit, wrapper, "Wrapper", 3),
        wrapper_test,
    )?;
    let generic = |tail: &str| format!("pub fn mk{tail} {{\n    x + x + x\n}}\n");
    let old_generic = |tail: &str| format!("pub fn mk{tail} {{\n    x + x\n}}\n");
    let generic_test = "use ar::mk;\n\n#[test]\nfn t() {\n    assert!(mk(2) > 0);\n}\n";
    let bound = "<T: std::ops::Add<Output = T> + Copy>(x: T) -> T";
    assert_repair_ready(
        "concrete-twin",
        &old_generic("(x: i32) -> i32"),
        &generic("(x: i32) -> i32"),
        generic_test,
    )?;
    assert_no_start(
        "generic-return",
        &old_generic(bound),
        &generic(bound),
        generic_test,
    )?;
    Ok(())
}

/// Devin finding: a discarded owner result beside an unrelated relational
/// assertion does not observe the owner, so no repair start is offered; the
/// positive twin (the assertion reads a local bound from the call) still is.
#[test]
fn discarded_owner_result_with_unrelated_assertion_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start(
        "discarded-unrelated",
        DBL_OLD,
        DBL_NEW,
        "use ar::dbl;\n\n#[test]\nfn unrelated() {\n    let _ = dbl(2);\n    assert!(1 > 0);\n}\n",
    )?;
    Ok(())
}

const DBL: (&str, &str) = (DBL_OLD, DBL_NEW);
const W: (&str, &str) = (W_OLD, W_NEW);
const W_OLD: &str =
    "pub struct W;\nimpl W {\n    pub fn dbl(x: i32) -> i32 {\n        x * 2\n    }\n}\n";
const W_NEW: &str =
    "pub struct W;\nimpl W {\n    pub fn dbl(x: i32) -> i32 {\n        x * 3\n    }\n}\n";
/// Same-named functions that are not the owner, and name collisions.
const RIVALS: &str = "mod other { pub fn dbl(x: i32) -> i32 { x } }\nmod Other { pub fn dbl(x: i32) -> i32 { x } }\nstruct X;\nimpl X { fn dbl(x: i32) -> i32 { x } }\ntrait T { fn dbl(self) -> u32; }\nimpl T for u32 { fn dbl(self) -> u32 { self } }\nfn undbl(x: i32) -> i32 { x }\nfn xdbl(x: i32) -> i32 { x }\nfn dbl_x(x: i32) -> i32 { x }\nfn of(x: i32) -> i32 { x }\n";

/// `bad` must not start; the same fixture with `good` made genuine must.
fn refuses_unless_genuine(
    fx: (&str, &str),
    test: &str,
    bad: &str,
    good: &str,
) -> Result<(), String> {
    assert_no_start(bad, fx.0, fx.1, &test.replace("@", bad))?;
    assert_repair_ready(bad, fx.0, fx.1, &test.replace("@", good))
}

/// A non-owner call (collision, comment or string, shadowed local, other
/// module, type or primitive) beside a discarded real call is no observation.
#[test]
fn calls_that_are_not_the_owner_do_not_observe_it() -> Result<(), String> {
    let calls = [
        "undbl(2)",
        "xdbl(2)",
        "dbl_x(2)",
        "other::dbl(2)",
        "Other::dbl(2)",
        "X::dbl(2)",
        "u32::dbl(2)",
    ];
    for call in calls {
        for shape in ["assert!(@ > 0);", "let r = @;\n    assert!(r > 0);"] {
            for (fx, real) in [(DBL, "ar::dbl(2)"), (W, "ar::W::dbl(2)")] {
                let test =
                    format!("{RIVALS}\n#[test]\nfn t() {{\n    let _ = {real};\n    {shape}\n}}\n");
                refuses_unless_genuine(fx, &test, call, real)?;
            }
        }
    }
    let frame = "#[test]\nfn t() {\n    let _ = ar::dbl(2);\n    @\n}\n";
    for (bad, good) in [
        (
            "let v = ar::dbl(2);\n    let v = 1;\n    assert!(v > 0);",
            "let v = ar::dbl(2);\n    assert!(v > 0);",
        ),
        (
            "let v = ar::dbl(2);\n    let v;\n    v = 1;\n    assert!(v > 0);",
            "let v = ar::dbl(2);\n    assert!(v > 0);",
        ),
        (
            "assert!(1 > 0 /* dbl(2) */);",
            "assert!(ar::dbl(2) > 0 /* dbl(2) */);",
        ),
        (
            "assert!(1 > 0 // dbl(2)\n    );",
            "assert!(ar::dbl(2) > 0 // dbl(2)\n    );",
        ),
        (
            "assert!(1 > 0, \"dbl(2) wrong\");",
            "assert!(ar::dbl(2) > 0, \"dbl(2) wrong\");",
        ),
        (
            "assert_eq!(\"dbl(\", \"dbl(\");",
            "assert!(ar::dbl(2) > 0);",
        ),
        ("assert!(1 > 0);", "assert!(ar::dbl(2) > 0);"),
    ] {
        refuses_unless_genuine(DBL, frame, bad, good)?;
    }
    // A bare call of a name the test file defines itself may not be the owner.
    for local in [
        "fn dbl(x: i32) -> i32 { x }\n",
        "fn other() {\n    fn dbl(x: i32) -> i32 { x }\n}\n",
    ] {
        let test = format!(
            "{local}#[test]\nfn t() {{\n    let _ = ar::dbl(2);\n    assert!(@ > 0);\n}}\n"
        );
        refuses_unless_genuine(DBL, &test, "dbl(2)", "ar::dbl(2)")?;
    }
    // In an integration test `crate::` names the test crate, not the library.
    let test = "pub struct W;\nimpl W { pub fn dbl(x: i32) -> i32 { x } }\nfn dbl(x: i32) -> i32 { x }\n#[test]\nfn t() {\n    let _ = ar::W::dbl(2);\n    assert!(@ > 0);\n}\n";
    refuses_unless_genuine(W, test, "crate::W::dbl(2)", "ar::W::dbl(2)")?;
    let test = "fn dbl(x: i32) -> i32 { x }\n#[test]\nfn t() {\n    let _ = ar::dbl(2);\n    assert!(@ > 0);\n}\n";
    refuses_unless_genuine(DBL, test, "crate::dbl(2)", "ar::dbl(2)")?;
    // Another type of the same name with a same-named method is not the owner.
    let test = "mod other { pub struct W; impl W { pub fn dbl(x: i32) -> i32 { x } } }\n#[test]\nfn t() {\n    let _ = ar::W::dbl(2);\n    assert!(@ > 0);\n}\n";
    refuses_unless_genuine(W, test, "other::W::dbl(2)", "ar::W::dbl(2)")?;
    // A method call reaches only a method; a bare call only a free function.
    let frame = format!(
        "{RIVALS}fn dbl(x: i32) -> i32 {{ x }}\n#[test]\nfn t() {{\n    let _ = @R;\n    assert!(@ > 0);\n}}\n"
    );
    for (fx, real, bad) in [
        (DBL, "ar::dbl(2)", "2u32.dbl()"),
        (W, "ar::W::dbl(2)", "dbl(2)"),
    ] {
        refuses_unless_genuine(fx, &frame.replace("@R", real), bad, real)?;
    }
    let (old, new) = (
        "pub fn f(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn f(x: i32) -> i32 {\n    x * 3\n}\n",
    );
    let test =
        format!("{RIVALS}#[test]\nfn t() {{\n    let _ = ar::f(2);\n    assert!(@ > 0);\n}}\n");
    refuses_unless_genuine((old, new), &test, "of(2)", "ar::f(2)")
}

/// The owner's own spellings still start, with the rivals present.
#[test]
fn the_owners_own_calls_still_reach_repair() -> Result<(), String> {
    for body in [
        "assert!(ar::dbl(2) > 0);",
        "let r = ar::dbl(2);\n    assert!(r > 0);",
        "assert!(ar::dbl(2) > 0, \"dbl(2) wrong\");",
        "let v = ar::dbl(2);\n    let w = v;\n    assert!(w > 0);",
    ] {
        assert_repair_ready(
            body,
            DBL_OLD,
            DBL_NEW,
            &format!("{RIVALS}\n#[test]\nfn t() {{\n    {body}\n}}\n"),
        )?;
    }
    assert_repair_ready(
        "self-type",
        W_OLD,
        W_NEW,
        &format!("use ar::W;\n{RIVALS}\n#[test]\nfn t() {{\n    assert!(W::dbl(2) > 0);\n}}\n"),
    )
}

// Changed match arms (not-ready burn-down): the same gates as the return
// value, plus a check-side direct owner call and a related test that passes
// the arm's own literal. A struct-literal field keeps its own route: a weak
// assertion on a sibling field is still not a start.

const FIELD_OLD: &str = "pub struct Config {\n    pub retries: u32,\n    pub verbose: bool,\n}\n\npub fn default_config() -> Config {\n    Config {\n        retries: 3,\n        verbose: false,\n    }\n}\n";
const FIELD_NEW: &str = "pub struct Config {\n    pub retries: u32,\n    pub verbose: bool,\n}\n\npub fn default_config() -> Config {\n    Config {\n        retries: 5,\n        verbose: false,\n    }\n}\n";
const ARM_OLD: &str = "pub fn rate(kind: &str) -> u32 {\n    match kind {\n        \"gold\" => 10,\n        \"silver\" => 5,\n        _ => 0,\n    }\n}\n";
const ARM_NEW: &str = "pub fn rate(kind: &str) -> u32 {\n    match kind {\n        \"gold\" => 10,\n        \"silver\" => 7,\n        _ => 0,\n    }\n}\n";

/// A changed field or arm that must not start: `check` prints no repair start,
/// and naming the changed line refuses. An unscoped bare `repair` may still
/// start an unchanged sibling arm, which is a different subject.
fn assert_no_start_at(
    case: &str,
    old: &str,
    new: &str,
    test: &str,
    at: &str,
) -> Result<(), String> {
    let repo = Repo::create(old, new, test)?;
    let check = repo.check_text()?;
    assert!(
        printed_repair_seam(&check).is_none(),
        "{case}: check must not print a repair start:\n{check}"
    );
    let (ok, repair) = repo.repair_text(&[at])?;
    assert!(!ok, "{case}: repair {at} must refuse:\n{repair}");
    assert!(!repair.contains("Ready to repair."), "{case}: {repair}");
    Ok(())
}

/// [`assert_no_start_at`] for a case `check` reads as weak, so the refusal
/// is the match-arm start's own and not a stronger or unknown reading.
fn assert_weak_no_start_at(
    case: &str,
    old: &str,
    new: &str,
    test: &str,
    at: &str,
) -> Result<(), String> {
    let repo = Repo::create(old, new, test)?;
    let check = repo.check_text()?;
    assert!(
        check.contains("Static exposure: weak"),
        "{case}: precondition, check must report a weak finding:\n{check}"
    );
    drop(repo);
    assert_no_start_at(case, old, new, test, at)
}

#[test]
fn changed_match_arm_with_its_literal_and_weak_assert_reaches_repair() -> Result<(), String> {
    assert_repair_ready(
        "match-arm",
        ARM_OLD,
        ARM_NEW,
        "use ar::rate;\n\n#[test]\nfn silver_runs() {\n    let r = rate(\"silver\");\n    assert!(r < 100);\n}\n",
    )
}

#[test]
fn match_arm_reached_only_through_a_sibling_literal_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start_at(
        "match-arm-sibling",
        ARM_OLD,
        ARM_NEW,
        "use ar::rate;\n\n#[test]\nfn gold_runs() {\n    let r = rate(\"gold\");\n    assert!(r < 100);\n}\n",
        "src/lib.rs:4",
    )
}

#[test]
fn match_arm_without_an_observing_assertion_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start_at(
        "match-arm-no-assert",
        ARM_OLD,
        ARM_NEW,
        "use ar::rate;\n\n#[test]\nfn silver_runs() {\n    let _r = rate(\"silver\");\n}\n",
        "src/lib.rs:4",
    )
}

#[test]
fn exact_field_or_arm_assertion_and_sibling_field_are_not_repair_starts() -> Result<(), String> {
    assert_no_start_at(
        "field-exact",
        FIELD_OLD,
        FIELD_NEW,
        "use ar::default_config;\n\n#[test]\nfn retries() {\n    assert_eq!(default_config().retries, 5);\n}\n",
        "src/lib.rs:8",
    )?;
    assert_no_start_at(
        "arm-exact",
        ARM_OLD,
        ARM_NEW,
        "use ar::rate;\n\n#[test]\nfn silver() {\n    assert_eq!(rate(\"silver\"), 7);\n}\n",
        "src/lib.rs:4",
    )?;
    // A weak assertion on a sibling field does not tie the test to the
    // changed field (the field route's own rule), so it is not a start.
    assert_no_start_at(
        "field-sibling",
        FIELD_OLD,
        FIELD_NEW,
        "use ar::default_config;\n\n#[test]\nfn quiet() {\n    let c = default_config();\n    assert!(!c.verbose);\n}\n",
        "src/lib.rs:8",
    )
}

#[test]
fn field_result_discarded_beside_an_unrelated_assert_is_not_a_repair_start() -> Result<(), String> {
    assert_no_start_at(
        "field-discarded",
        FIELD_OLD,
        FIELD_NEW,
        "use ar::default_config;\n\n#[test]\nfn builds() {\n    let _c = default_config();\n    assert!(1 > 0);\n}\n",
        "src/lib.rs:8",
    )
}

/// #6951: a receiver bound to a test-module type that shadows the owner's type
/// is not a direct call of the changed owner, so the arm it seems to select is
/// not a repair start.
#[test]
fn shadowed_receiver_type_is_not_a_match_arm_repair_start() -> Result<(), String> {
    let old = "pub struct Window;\n\nimpl Window {\n    pub fn rate(&self, kind: &str) -> u32 {\n        match kind {\n            \"gold\" => 10,\n            \"silver\" => 5,\n            _ => 0,\n        }\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    struct Window;\n\n    impl Window {\n        fn rate(&self, _kind: &str) -> u32 {\n            1\n        }\n    }\n\n    #[test]\n    fn silver_runs() {\n        let window = Window;\n        assert!(window.rate(\"silver\") > 0);\n    }\n}\n";
    let new = old.replace("\"silver\" => 5,", "\"silver\" => 7,");
    assert_no_start_at(
        "shadowed-arm",
        old,
        &new,
        "// no integration tests\n",
        "src/lib.rs:7",
    )
}

// Review probes for the match-arm start: every gate must hold for one and the
// same test, and the literal must be the argument the arm's own match reads.

/// One test passes the arm's literal but ignores the result; another observes
/// the result of a sibling arm. No single test does both, so no start.
#[test]
fn arm_literal_and_observing_assertion_in_different_tests_is_not_a_start() -> Result<(), String> {
    assert_no_start_at(
        "arm-split-tests",
        ARM_OLD,
        ARM_NEW,
        "use ar::rate;\n\n#[test]\nfn silver_ignored() {\n    let _r = rate(\"silver\");\n}\n\n#[test]\nfn gold_runs() {\n    let r = rate(\"gold\");\n    assert!(r < 100);\n}\n",
        "src/lib.rs:4",
    )
}

const TWO_PARAM_OLD: &str = "pub fn rate(a: u32, b: u32) -> u32 {\n    match b {\n        0 => 10,\n        1 => 5,\n        _ => a,\n    }\n}\n";

/// The arm's literal in a different argument than the scrutinee is no
/// evidence that the arm runs.
#[test]
fn arm_literal_in_another_argument_is_not_a_start() -> Result<(), String> {
    assert_no_start_at(
        "arm-other-argument",
        TWO_PARAM_OLD,
        &TWO_PARAM_OLD.replace("1 => 5", "1 => 7"),
        "use ar::rate;\n\n#[test]\nfn runs() {\n    assert!(rate(1, 9) < 100);\n}\n",
        "src/lib.rs:4",
    )
}

/// The same owner with the literal in the scrutinee's own argument starts.
#[test]
fn arm_literal_in_the_scrutinee_argument_reaches_repair() -> Result<(), String> {
    let old = "pub fn rate(scale: u32, kind: &str) -> u32 {\n    match kind {\n        \"gold\" => 10,\n        \"silver\" => 5,\n        _ => scale,\n    }\n}\n";
    assert_repair_ready(
        "arm-scrutinee-argument",
        old,
        &old.replace("\"silver\" => 5", "\"silver\" => 7"),
        "use ar::rate;\n\n#[test]\nfn runs() {\n    assert!(rate(9, \"silver\") < 100);\n}\n",
    )
}

/// A derived scrutinee (`kind.trim()`) does not equal the argument; the bare
/// `match kind` twin with the same test starts, so only the scrutinee differs.
/// (The whole-body reader's own refusal is isolated by a unit test.)
#[test]
fn derived_scrutinee_is_not_a_start() -> Result<(), String> {
    let old = |scrutinee: &str| {
        format!(
            "pub fn rate(kind: &str) -> u32 {{\n    match {scrutinee} {{\n        \"gold\" => 10,\n        \"silver\" => 5,\n        _ => 0,\n    }}\n}}\n"
        )
    };
    let new = |scrutinee: &str| old(scrutinee).replace("\"silver\" => 5", "\"silver\" => 7");
    let test = "use ar::rate;\n\n#[test]\nfn runs() {\n    assert!(rate(\"silver\") < 100);\n}\n";
    assert_repair_ready("arm-bare-scrutinee", &old("kind"), &new("kind"), test)?;
    assert_no_start_at(
        "arm-derived-scrutinee",
        &old("kind.trim()"),
        &new("kind.trim()"),
        test,
        "src/lib.rs:4",
    )
}

/// An inner arm under an outer arm the test does not take.
#[test]
fn nested_arm_under_an_untaken_outer_arm_is_not_a_start() -> Result<(), String> {
    let old = "pub fn rate(kind: &str, tier: u32) -> u32 {\n    match tier {\n        0 => match kind {\n            \"silver\" => 5,\n            _ => 0,\n        },\n        _ => 1,\n    }\n}\n";
    assert_no_start_at(
        "arm-nested",
        old,
        &old.replace("\"silver\" => 5", "\"silver\" => 7"),
        "use ar::rate;\n\n#[test]\nfn runs() {\n    assert!(rate(\"silver\", 1) < 100);\n}\n",
        "src/lib.rs:4",
    )
}

/// An earlier guard arm may take the input first.
#[test]
fn earlier_guard_arm_is_not_a_start() -> Result<(), String> {
    let old = "pub fn rate(kind: &str) -> u32 {\n    match kind {\n        s if s.len() > 3 => 1,\n        \"silver\" => 5,\n        _ => 0,\n    }\n}\n";
    assert_no_start_at(
        "arm-guard-first",
        old,
        &old.replace("\"silver\" => 5", "\"silver\" => 7"),
        "use ar::rate;\n\n#[test]\nfn runs() {\n    assert!(rate(\"silver\") < 100);\n}\n",
        "src/lib.rs:4",
    )
}

/// A builder call naming the literal never reaches the owner's argument: only
/// the owner call's own argument counts, so the twin that differs only by
/// passing `"silver"` to `rate` starts and the builder one does not.
#[test]
fn builder_literal_that_never_reaches_the_owner_is_not_a_start() -> Result<(), String> {
    let test = |argument: &str| {
        format!(
            "use ar::rate;\n\nstruct B;\nimpl B {{\n    fn silver(self, _s: &str) -> Self {{\n        self\n    }}\n}}\n\n#[test]\nfn runs() {{\n    let _b = B.silver(\"silver\");\n    let k = String::from(\"gold\");\n    assert!(rate({argument}) < 100);\n}}\n"
        )
    };
    assert_repair_ready("arm-builder-twin", ARM_OLD, ARM_NEW, &test("\"silver\""))?;
    assert_no_start_at(
        "arm-builder-literal",
        ARM_OLD,
        ARM_NEW,
        &test("&k"),
        "src/lib.rs:4",
    )
}

fn rate_test(body: &str) -> String {
    format!("use ar::rate;\n\n#[test]\nfn runs() {{\n{body}}}\n")
}

/// Only the arm's literal written in the call itself counts: a local first
/// bound to it and then rebound by a second `let` or reassigned through `mut`
/// gives the call another value, so neither starts, while the twin passing the
/// literal directly does.
#[test]
fn rebound_or_reassigned_let_literal_is_not_a_match_arm_start() -> Result<(), String> {
    assert_repair_ready(
        "arm-direct-literal",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    let _k = \"silver\";\n    assert!(rate(\"silver\") < 100);\n"),
    )?;
    for (case, body) in [
        (
            "arm-let-rebound",
            "    let k = \"silver\";\n    let k = if k.is_empty() { k } else { \"gold\" };\n    assert!(rate(k) < 100);\n",
        ),
        (
            "arm-let-reassigned",
            "    let mut k = \"silver\";\n    k = \"gold\";\n    assert!(rate(k) < 100);\n",
        ),
        (
            "arm-let-once",
            "    let k = \"silver\";\n    assert!(rate(k) < 100);\n",
        ),
    ] {
        assert_no_start_at(case, ARM_OLD, ARM_NEW, &rate_test(body), "src/lib.rs:4")?;
    }
    Ok(())
}

/// The literal and the asserted result must come from the same one call: a
/// second call of the owner, a call that may never run (inside a branch or a
/// closure), or a same-named call on another receiver refuses.
#[test]
fn literal_and_asserted_result_from_different_calls_are_not_a_start() -> Result<(), String> {
    assert_repair_ready(
        "arm-one-call",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    let r = rate(\"silver\");\n    assert!(r < 100);\n"),
    )?;
    for (case, body) in [
        (
            "arm-second-call",
            "    let _r = rate(\"silver\");\n    let r = rate(\"gold\");\n    assert!(r < 100);\n",
        ),
        (
            "arm-dead-branch",
            "    let r = match false {\n        true => rate(\"silver\"),\n        false => 0,\n    };\n    assert!(r < 100);\n",
        ),
        (
            "arm-branch-only",
            "    let r = if false { rate(\"silver\") } else { 0 };\n    assert!(r < 100);\n",
        ),
        (
            "arm-closure",
            "    let f = || rate(\"silver\");\n    assert!(f() < 100);\n",
        ),
    ] {
        assert_no_start_at(case, ARM_OLD, ARM_NEW, &rate_test(body), "src/lib.rs:4")?;
    }
    Ok(())
}

const TWO_ARG_METHOD_OLD: &str = "pub struct Window {\n    pub tier: u32,\n}\n\nimpl Window {\n    pub fn rate(&self, tier: &str, kind: &str) -> u32 {\n        match kind {\n            \"gold\" => 10,\n            \"silver\" => 5,\n            _ => self.tier + tier.len() as u32,\n        }\n    }\n}\n";

fn method_test(body: &str) -> String {
    format!(
        "use ar::Window;\n\nstruct Other;\nimpl Other {{\n    fn rate(&self, _a: &str, _k: &str) -> u32 {{\n        1\n    }}\n}}\n\n#[test]\nfn runs() {{\n    let window = Window {{ tier: 1 }};\n{body}}}\n"
    )
}

/// `Window::rate(&window, ..)` keeps the receiver as its first argument, so
/// its arguments do not line up with the method's parameters: `"silver"` there
/// is `tier`, not `kind`. The method-call twin passing `"silver"` as `kind`
/// starts; the path call and a same-named call on another receiver do not.
#[test]
fn path_qualified_method_call_or_other_receiver_is_not_a_start() -> Result<(), String> {
    let new = TWO_ARG_METHOD_OLD.replace("\"silver\" => 5", "\"silver\" => 7");
    assert_repair_ready(
        "method-call-twin",
        TWO_ARG_METHOD_OLD,
        &new,
        &method_test("    assert!(window.rate(\"gold\", \"silver\") < 100);\n"),
    )?;
    for (case, body) in [
        (
            "path-qualified-method",
            "    assert!(Window::rate(&window, \"silver\", \"gold\") < 100);\n",
        ),
        (
            "other-receiver",
            "    let _o = Other.rate(\"x\", \"silver\");\n    assert!(window.rate(\"gold\", \"gold\") < 100);\n",
        ),
    ] {
        assert_no_start_at(
            case,
            TWO_ARG_METHOD_OLD,
            &new,
            &method_test(body),
            "src/lib.rs:9",
        )?;
    }
    Ok(())
}

const FN_ARG_OLD: &str = "pub fn rate(scale: u32, tier: &str, kind: &str) -> u32 {\n    match kind {\n        \"gold\" => 10,\n        \"silver\" => 5,\n        _ => scale + tier.len() as u32,\n    }\n}\n";

/// A closure argument's own commas (`|a, b|`) would shift the argument count,
/// so `"silver"` would be read as `kind` while the call passes it as `tier`:
/// only simple-token arguments count. The twin passing a plain value in the
/// same place, with `"silver"` really as `kind`, starts. (Only the argument
/// text is read, so the closure need not fit the parameter type.)
#[test]
fn closure_argument_with_commas_is_not_a_match_arm_start() -> Result<(), String> {
    let new = FN_ARG_OLD.replace("\"silver\" => 5", "\"silver\" => 7");
    let test = |call: &str| {
        format!(
            "use ar::rate;\n\nfn add(a: u32, b: u32) -> u32 {{\n    a + b\n}}\n\n#[test]\nfn runs() {{\n    assert!({call} < 100);\n}}\n"
        )
    };
    assert_repair_ready(
        "arm-plain-argument",
        FN_ARG_OLD,
        &new,
        &test("rate(9, \"gold\", \"silver\")"),
    )?;
    assert_no_start_at(
        "arm-closure-argument",
        FN_ARG_OLD,
        &new,
        &test("rate(|a, b| a + b, \"silver\", \"gold\")"),
        "src/lib.rs:4",
    )
}

/// The call must run whenever the test does and its result must be the
/// assertion's condition: a `#[cfg]`-gated assertion or a test gated by an
/// inner `#![cfg]`, a call only in the
/// assertion's failure message, and an earlier statement that can end the
/// test first all refuse; the plain twin starts.
#[test]
fn compiled_out_message_only_or_skippable_call_is_not_a_match_arm_start() -> Result<(), String> {
    assert_repair_ready(
        "arm-plain-assert",
        ARM_OLD,
        ARM_NEW,
        &rate_test(
            "    let ok = true;\n    assert!(rate(\"silver\") < 100, \"rate\");\n    assert!(ok);\n",
        ),
    )?;
    for (case, body) in [
        (
            "arm-cfg-gated",
            "    let ok = true;\n    #[cfg(any())]\n    assert!(rate(\"silver\") < 100, \"rate\");\n    assert!(ok);\n",
        ),
        (
            "arm-inner-cfg",
            "    #![cfg(any())]\n    let ok = true;\n    assert!(rate(\"silver\") < 100, \"rate\");\n    assert!(ok);\n",
        ),
        (
            "arm-message-only",
            "    let ok = true;\n    assert!(ok, \"rate {}\", rate(\"silver\"));\n",
        ),
        (
            "arm-early-return",
            "    let ok = true;\n    let Some(_x) = ok.then_some(1) else {\n        return;\n    };\n    assert!(rate(\"silver\") < 100, \"rate\");\n",
        ),
    ] {
        assert_no_start_at(case, ARM_OLD, ARM_NEW, &rate_test(body), "src/lib.rs:4")?;
    }
    Ok(())
}

const PLAIN_PARAM_OLD: &str = "pub fn rate(t: u32, kind: &str, tier: &str) -> u32 {\n    match kind {\n        \"gold\" => 10,\n        \"silver\" => 5,\n        _ => t + tier.len() as u32,\n    }\n}\n";

fn arm_new(old: &str) -> String {
    old.replace("\"silver\" => 5", "\"silver\" => 7")
}

/// A generic parameter type holds its own commas (`HashMap<u32, a::B>`), so
/// a comma-split parameter list would read a fake parameter and shift `kind`
/// onto the `tier` argument: an owner with a generic parameter type refuses,
/// while the plain-type twin passing `"silver"` as `kind` starts.
#[test]
fn generic_parameter_type_is_not_a_match_arm_start() -> Result<(), String> {
    assert_repair_ready(
        "arm-plain-parameter",
        PLAIN_PARAM_OLD,
        &arm_new(PLAIN_PARAM_OLD),
        &rate_test("    let t = 9;\n    assert!(rate(t, \"silver\", \"gold\") < 100);\n"),
    )?;
    for (case, kind) in [
        ("arm-generic-path", "HashMap<u32, std::string::String>"),
        ("arm-generic-result", "Result<u8, std::io::Error>"),
    ] {
        let old = format!(
            "use std::collections::HashMap;\n\npub fn rate(t: {kind}, kind: &str, tier: &str) -> u32 {{\n    match kind {{\n        \"gold\" => 10,\n        \"silver\" => 5,\n        _ => tier.len() as u32,\n    }}\n}}\n"
        );
        assert_no_start_at(
            case,
            &old,
            &arm_new(&old),
            "use ar::rate;\n\n#[test]\nfn runs() {\n    let t = Default::default();\n    assert!(rate(t, \"gold\", \"silver\") < 100);\n}\n",
            "src/lib.rs:6",
        )?;
    }
    Ok(())
}

/// A quote inside a comment must not hide an arm: the guard arm between
/// `"gold"` and `"silver"` takes six-letter input first, so no start; the
/// twin with a literal arm in its place (same comments) starts.
#[test]
fn quote_in_a_comment_does_not_hide_an_earlier_guard_arm() -> Result<(), String> {
    let lib = |second: &str| {
        format!(
            "pub fn rate(kind: &str) -> u32 {{\n    match kind {{\n        \"gold\" => 1 /* \" */,\n        {second} => 2 /* \" */,\n        \"silver\" => 5,\n        _ => 0,\n    }}\n}}\n"
        )
    };
    let test = rate_test("    assert!(rate(\"silver\") < 100);\n");
    let twin = lib("\"bronze\"");
    assert_repair_ready("arm-comment-twin", &twin, &arm_new(&twin), &test)?;
    let guarded = lib("s if s.len() == 6");
    assert_no_start_at(
        "arm-comment-quote-guard",
        &guarded,
        &arm_new(&guarded),
        &test,
        "src/lib.rs:5",
    )
}

/// A macro around the owner call may drop or rewrite it (any `name!(..)`
/// refuses; `dbg!` stands in so no file-local macro changes how assertions
/// are read), and `debug_assert!` is compiled out of release test builds:
/// each refuses, while the twin binding the call directly starts.
#[test]
fn macro_wrapped_call_or_debug_assert_is_not_a_match_arm_start() -> Result<(), String> {
    assert_repair_ready(
        "arm-macro-twin",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    let r = rate(\"silver\");\n    assert!(r < 100);\n"),
    )?;
    assert_no_start_at(
        "arm-macro-wrapper",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    let r = dbg!(rate(\"silver\"));\n    assert!(r < 100);\n"),
        "src/lib.rs:4",
    )?;
    assert_no_start_at(
        "arm-debug-assert",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    debug_assert!(rate(\"silver\") < 100);\n"),
        "src/lib.rs:4",
    )
}

/// A macro call spaced around its `!` (`stringify ! (..)`) is still a macro
/// around the owner call, so it refuses; the twin binding the call directly
/// starts. (The comment-spaced `keep /**/ !(..)` form is covered by the
/// reader's unit test: a file-local `macro_rules!` already withholds here.)
#[test]
fn spaced_macro_around_the_call_is_not_a_match_arm_start() -> Result<(), String> {
    assert_repair_ready(
        "arm-spaced-macro-twin",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    let r = rate(\"silver\");\n    assert!(r < 100);\n"),
    )?;
    assert_weak_no_start_at(
        "arm-spaced-stringify",
        ARM_OLD,
        ARM_NEW,
        &rate_test("    let r = stringify ! (rate(\"silver\"));\n    assert!(r.len() < 100);\n"),
        "src/lib.rs:4",
    )
}

/// An `assert!` that is not the standard macro may never evaluate the call:
/// a `use .. as assert` at file or body level, or a file-level
/// `macro_rules! assert`, refuses; the twin with the same exported no-op
/// macro but the standard `assert!` starts.
#[test]
fn rebound_assert_macro_is_not_a_match_arm_start() -> Result<(), String> {
    let lib =
        format!("{ARM_OLD}\n#[macro_export]\nmacro_rules! noop {{\n    ($($t:tt)*) => {{}};\n}}\n");
    let new = lib.replace("\"silver\" => 5", "\"silver\" => 7");
    let assertion = "    assert!(rate(\"silver\") < 100);\n";
    assert_repair_ready(
        "arm-standard-assert-twin",
        &lib,
        &new,
        &format!("use ar::rate;\n\n#[test]\nfn runs() {{\n{assertion}}}\n"),
    )?;
    for (case, test) in [
        (
            "arm-use-as-assert",
            format!(
                "use ar::noop as assert;\nuse ar::rate;\n\n#[test]\nfn runs() {{\n{assertion}}}\n"
            ),
        ),
        (
            "arm-body-use-as-assert",
            format!(
                "use ar::rate;\n\n#[test]\nfn runs() {{\n    use ar::noop as assert;\n{assertion}}}\n"
            ),
        ),
        (
            "arm-macro-rules-assert",
            format!(
                "use ar::rate;\n\nmacro_rules! assert {{\n    ($($t:tt)*) => {{}};\n}}\n\n#[test]\nfn runs() {{\n{assertion}}}\n"
            ),
        ),
    ] {
        // Check itself now reads all three as unrevealed (it cannot establish
        // that the standard `assert!` runs), so this pins the outcome; the
        // rebound check in the start is a second, independent refusal.
        assert_no_start_at(case, &lib, &new, &test, "src/lib.rs:4")?;
    }
    Ok(())
}

const TWIN_A: &str = "pub mod a {\n    pub fn rate(kind: &str) -> u32 {\n        match kind {\n            \"gold\" => 10,\n            \"silver\" => 5,\n            _ => 0,\n        }\n    }\n}\n";
const TWIN_B: &str = "\npub mod b {\n    pub fn rate(_kind: &str) -> u32 {\n        1\n    }\n}\n";

/// A free owner `a::rate` beside a same-named `b::rate`: a bare `rate(..)`
/// may reach either through a file-level import, so the start withholds,
/// even when the test imports the real owner. The workspace with only
/// `a::rate` starts.
#[test]
fn same_named_free_function_elsewhere_is_not_a_match_arm_start() -> Result<(), String> {
    let test = |import: &str| {
        format!(
            "use ar::{import}::rate;\n\n#[test]\nfn runs() {{\n    let r = rate(\"silver\");\n    assert!(r < 100);\n}}\n"
        )
    };
    let arm_new = |lib: &str| lib.replace("\"silver\" => 5", "\"silver\" => 7");
    assert_repair_ready("arm-unique-free-name", TWIN_A, &arm_new(TWIN_A), &test("a"))?;
    let both = format!("{TWIN_A}{TWIN_B}");
    for (case, import) in [("arm-twin-owner-imported", "a"), ("arm-twin-imported", "b")] {
        assert_no_start_at(case, &both, &arm_new(&both), &test(import), "src/lib.rs:5")?;
    }
    Ok(())
}

/// An earlier arm naming the same literal, alone or in a `|` alternation,
/// takes the input first; the twin without it starts.
#[test]
fn earlier_arm_repeating_the_literal_is_not_a_start() -> Result<(), String> {
    let test = rate_test("    assert!(rate(\"silver\") < 100);\n");
    let lib = |first: &str| {
        format!(
            "pub fn rate(kind: &str) -> u32 {{\n    match kind {{\n        {first} => 1,\n        \"silver\" => 5,\n        _ => 0,\n    }}\n}}\n"
        )
    };
    assert_repair_ready(
        "arm-distinct-earlier",
        &lib("\"gold\""),
        &lib("\"gold\"").replace("\"silver\" => 5", "\"silver\" => 7"),
        &test,
    )?;
    for (case, first) in [
        ("arm-repeated-earlier", "\"silver\""),
        ("arm-alternation-earlier", "\"gold\" | \"silver\""),
    ] {
        assert_no_start_at(
            case,
            &lib(first),
            &lib(first).replace("\"silver\" => 5", "\"silver\" => 7"),
            &test,
            "src/lib.rs:4",
        )?;
    }
    Ok(())
}

const METHOD_ARM_OLD: &str = "pub struct Window {\n    pub tier: u32,\n}\n\nimpl Window {\n    pub fn rate(&self, kind: &str) -> u32 {\n        match kind {\n            \"gold\" => 10,\n            \"silver\" => 5,\n            _ => self.tier,\n        }\n    }\n}\n";
const METHOD_ARM_TEST: &str = "use ar::Window;\n\n#[test]\nfn runs() {\n    let window = Window { tier: 1 };\n    assert!(window.rate(\"silver\") < 100);\n}\n";

/// Positive control for the impl-identity guard: a method arm on a type the
/// workspace declares once starts.
#[test]
fn method_arm_on_a_uniquely_declared_type_reaches_repair() -> Result<(), String> {
    assert_repair_ready(
        "method-arm",
        METHOD_ARM_OLD,
        &METHOD_ARM_OLD.replace("\"silver\" => 5", "\"silver\" => 7"),
        METHOD_ARM_TEST,
    )
}

/// The same method arm with a second `struct Window` elsewhere in the
/// workspace: which `Window` a test names is not resolved, so no start.
#[test]
fn method_arm_on_a_type_declared_twice_is_not_a_start() -> Result<(), String> {
    let old = format!(
        "{METHOD_ARM_OLD}\npub mod other {{\n    pub struct Window {{\n        pub tier: u32,\n    }}\n}}\n"
    );
    assert_no_start_at(
        "method-arm-two-windows",
        &old,
        &old.replace("\"silver\" => 5", "\"silver\" => 7"),
        METHOD_ARM_TEST,
        "src/lib.rs:9",
    )
}
