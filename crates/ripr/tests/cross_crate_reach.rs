//! #11401: a test in one workspace crate that reaches a change through that
//! crate's function, which calls a function of another workspace crate.
//!
//! The audit case: `wscore::discount` changed `price > 100` to `price >= 100`,
//! `app::checkout` returns `wscore::discount(price)`, and the only test is
//! `assert_eq!(checkout(100), 90)` in `app/tests`. The boundary input 100 goes
//! straight through `checkout` and the exact `== 90` pin notices the change,
//! so `ripr check` reads it as noticed instead of "no static path".
//!
//! Every negative changes ONE thing from that positive and keeps the changed
//! owner and line the same, so none of them can pass for a different reason.
//! The first test of each family asserts the generated fixture itself (the
//! finding is on `wscore/src/lib.rs:2`, `price >= 100`) before any claim.
//!
//! Bare `ripr repair` reads the same change through its own seam producer,
//! which does not follow a call into another crate. It asks `ripr check` about
//! the changed spot (slice 2 of #11401), so repair says the change is already
//! noticed exactly when check does, and never starts beside that test. Where
//! check does not read the change as noticed, repair keeps its own answer.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const ROOT_TOML: &str = "[workspace]\nmembers = [\"wscore\", \"app\"]\nresolver = \"2\"\n";
const WSCORE_TOML: &str = "[package]\nname = \"wscore\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const APP_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nwscore = { path = \"../wscore\" }\n";

const WSCORE_OLD: &str = "pub fn discount(price: u32) -> u32 {\n    if price > 100 {\n        price - 10\n    } else {\n        price\n    }\n}\n";
const WSCORE_NEW: &str = "pub fn discount(price: u32) -> u32 {\n    if price >= 100 {\n        price - 10\n    } else {\n        price\n    }\n}\n";
const APP_LIB: &str = "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price)\n}\n";
const APP_TEST: &str = "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(100), 90);\n}\n";

/// The plain summary fragments the check screen prints.
const NOTICED: &str = "1 appear noticed by tests";
const NOT_NOTICED: &str = "0 appear noticed by tests";
const NO_PATH: &str = "1 no test path";

/// The files of one workspace on `main`; `wscore/src/lib.rs` changes on
/// `feature` from `WSCORE_OLD` to `WSCORE_NEW`.
struct Case {
    files: Vec<(&'static str, String)>,
}

impl Case {
    fn audit() -> Self {
        Self {
            files: vec![
                ("Cargo.toml", ROOT_TOML.to_string()),
                ("wscore/Cargo.toml", WSCORE_TOML.to_string()),
                ("wscore/src/lib.rs", WSCORE_OLD.to_string()),
                ("app/Cargo.toml", APP_TOML.to_string()),
                ("app/src/lib.rs", APP_LIB.to_string()),
                ("app/tests/t.rs", APP_TEST.to_string()),
            ],
        }
    }

    /// Replace (or add) one file of the base commit.
    fn with(mut self, path: &'static str, text: &str) -> Self {
        match self.files.iter_mut().find(|(name, _)| *name == path) {
            Some(entry) => entry.1 = text.to_string(),
            None => self.files.push((path, text.to_string())),
        }
        self
    }

    /// Drop one file from the base commit.
    fn without(mut self, path: &str) -> Self {
        self.files.retain(|(name, _)| *name != path);
        self
    }
}

struct Repo(PathBuf);

impl Repo {
    fn create(case: &Case) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-cross-crate-reach-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        for (path, text) in &case.files {
            repo.write(path, text)?;
        }
        repo.git(&["-c", "init.templateDir=", "init", "-q"])?;
        repo.git(&["config", "user.email", "t@example.com"])?;
        repo.git(&["config", "user.name", "t"])?;
        repo.git(&["config", "commit.gpgSign", "false"])?;
        repo.git(&["add", "-A"])?;
        repo.git(&["commit", "-qm", "base"])?;
        repo.git(&["branch", "-M", "main"])?;
        repo.git(&["checkout", "-qb", "feature"])?;
        repo.write("wscore/src/lib.rs", WSCORE_NEW)?;
        repo.git(&["commit", "-qam", "change"])?;
        Ok(repo)
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
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

    /// The `ripr check` screen with `extra` flags added.
    fn check(&self, extra: &[&str]) -> Result<String, String> {
        let mut args = vec!["check", "--base", "main"];
        args.extend_from_slice(extra);
        Ok(text(&self.ripr(&args)?))
    }

    /// Bare `ripr repair`: whether it started, and its text.
    fn repair(&self) -> Result<(bool, String), String> {
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

/// The id after `ripr repair ` on the check screen, if check offers a start.
fn check_start_id(check: &str) -> Option<&str> {
    check.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix("ripr repair ")?;
        let id = rest.split_whitespace().next()?;
        (id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
    })
}

/// The fixture built what the test says it did: one finding, on the changed
/// comparison in `wscore`.
fn assert_changed_subject(case: &str, check: &str) -> Result<(), String> {
    if !check.contains("File: ./wscore/src/lib.rs:2")
        || !check.contains("Changed behavior: price >= 100")
        || !check.contains("1 changed behavior checked")
    {
        return Err(format!(
            "{case}: the finding must be the changed `price >= 100` in wscore:\n{check}"
        ));
    }
    Ok(())
}

/// Neither command may start a repair: check offers no start, bare repair
/// does not start.
fn assert_no_repair_start(case: &str, repo: &Repo, check: &str) -> Result<(), String> {
    if let Some(id) = check_start_id(check) {
        return Err(format!("{case}: check must not offer `ripr repair {id}`"));
    }
    let (started, repair) = repo.repair()?;
    if started {
        return Err(format!("{case}: bare repair must not start:\n{repair}"));
    }
    Ok(())
}

/// The unit test of the owner's package: it calls the changed function far from
/// the boundary, so it reaches the change without pinning it.
const OWNER_PACKAGE_TEST: &str = "use wscore::discount;\n\n#[test]\nfn small_price_is_unchanged() {\n    assert_eq!(discount(5), 5);\n}\n";

/// Bare repair does not start and says the changed code is already noticed,
/// in the words check's agreement uses, with no leftover "could not establish"
/// or "missing evidence" claim.
fn assert_repair_says_noticed(case: &str, repo: &Repo) -> Result<(), String> {
    let (started, repair) = repo.repair()?;
    if started
        || !repair.contains("Nothing to do for this change")
        || !repair.contains("reads this changed code as already noticed by a test")
        || repair.contains("could not establish")
        || repair.contains("missing evidence")
        || repair.contains("repair route not ready")
    {
        return Err(format!(
            "{case}: bare repair must say the change is already noticed and not start:\n{repair}"
        ));
    }
    Ok(())
}

/// A negative with an open gap: check reads the change as partly noticed and
/// offers a start, and bare repair starts that same repair.
fn assert_open_gap_starts(case: &str, base: Case) -> Result<(), String> {
    let repo = Repo::create(&base)?;
    let check = repo.check(&[])?;
    assert_changed_subject(case, &check)?;
    if check.contains(NOTICED) || !check.contains("1 partly noticed") {
        return Err(format!("{case}: check must read a partial gap:\n{check}"));
    }
    let Some(id) = check_start_id(&check) else {
        return Err(format!("{case}: check must offer a repair start:\n{check}"));
    };
    let id = id.to_string();
    let (started, repair) = repo.repair()?;
    if !started || !repair.contains(&id) {
        return Err(format!(
            "{case}: bare repair must start the repair check names ({id}):\n{repair}"
        ));
    }
    Ok(())
}

/// A negative: the same changed line, read as `summary_part`, never as noticed,
/// and no repair start anywhere.
fn assert_not_noticed(case: &str, base: Case, summary_part: &str) -> Result<(), String> {
    let repo = Repo::create(&base)?;
    let check = repo.check(&[])?;
    assert_changed_subject(case, &check)?;
    if !check.contains(NOT_NOTICED) || check.contains(NOTICED) || !check.contains(summary_part) {
        return Err(format!(
            "{case}: must not read as noticed, and must read `{summary_part}`:\n{check}"
        ));
    }
    assert_no_repair_start(case, &repo, &check)
}

#[test]
fn boundary_pin_through_another_crates_function_is_noticed_by_check() -> Result<(), String> {
    let repo = Repo::create(&Case::audit())?;
    let check = repo.check(&[])?;
    assert_changed_subject("audit", &check)?;
    if !check.contains(NOTICED)
        || !check.contains("Static exposure: exposed")
        || !check.contains("Related test: app/tests/t.rs:4 checkout_discounts")
    {
        return Err(format!(
            "the checkout(100) == 90 pin must read noticed:\n{check}"
        ));
    }
    // Nothing is left to repair: no start from check, none from bare repair.
    assert_no_repair_start("audit", &repo, &check)
}

#[test]
fn narrow_check_modes_that_leave_the_test_out_claim_nothing() -> Result<(), String> {
    // The unchanged `app` test is outside these modes' evidence, so they keep
    // the earlier no-path reading and, as always, offer no start.
    for extra in [&["--no-unchanged-tests"][..], &["--mode", "instant"][..]] {
        let repo = Repo::create(&Case::audit())?;
        let check = repo.check(extra)?;
        assert_changed_subject(&format!("audit {extra:?}"), &check)?;
        if check.contains(NOTICED) || !check.contains(NO_PATH) {
            return Err(format!(
                "{extra:?}: the unread test must not read as noticed:\n{check}"
            ));
        }
        assert_no_repair_start(&format!("audit {extra:?}"), &repo, &check)?;
        // Bare repair takes no mode: it reads the whole workspace, as the
        // default check does, so it names the same noticed change.
        assert_repair_says_noticed(&format!("audit {extra:?}"), &repo)?;
    }
    Ok(())
}

#[test]
fn bare_repair_agrees_the_audit_case_is_already_noticed() -> Result<(), String> {
    let repo = Repo::create(&Case::audit())?;
    let check = repo.check(&[])?;
    assert_changed_subject("audit", &check)?;
    if !check.contains(NOTICED) {
        return Err(format!(
            "precondition: check reads the pin as noticed:\n{check}"
        ));
    }
    assert_repair_says_noticed("audit", &repo)
}

#[test]
fn an_unrelated_test_in_the_owners_package_does_not_start_a_repair_beside_the_pin()
-> Result<(), String> {
    // `wscore/tests/other.rs` calls `discount` at an input far from the
    // boundary. It reaches the changed code but does not pin it, so the seam
    // looked open and repair started beside the `checkout(100) == 90` pin.
    let case = Case::audit().with("wscore/tests/other.rs", OWNER_PACKAGE_TEST);
    let repo = Repo::create(&case)?;
    let check = repo.check(&[])?;
    assert_changed_subject("unrelated test", &check)?;
    if !check.contains(NOTICED) || check_start_id(&check).is_some() {
        return Err(format!(
            "check must read the pin as noticed and offer no start:\n{check}"
        ));
    }
    assert_repair_says_noticed("unrelated test", &repo)
}

#[test]
fn an_unrelated_test_beside_a_pin_off_the_boundary_still_starts_the_same_repair()
-> Result<(), String> {
    // One change from the test above: the cross-crate pin is at 150, not 100.
    // Nothing notices the boundary, so check and repair both offer the start.
    assert_open_gap_starts(
        "off the boundary",
        Case::audit()
            .with("wscore/tests/other.rs", OWNER_PACKAGE_TEST)
            .with(
                "app/tests/t.rs",
                "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(150), 140);\n}\n",
            ),
    )
}

#[test]
fn an_unrelated_test_beside_a_loose_cross_crate_check_still_starts_the_same_repair()
-> Result<(), String> {
    assert_open_gap_starts(
        "loose check",
        Case::audit()
            .with("wscore/tests/other.rs", OWNER_PACKAGE_TEST)
            .with(
                "app/tests/t.rs",
                "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert!(checkout(100) > 0);\n}\n",
            ),
    )
}

#[test]
fn an_unrelated_test_beside_a_renamed_dependency_still_starts_the_same_repair() -> Result<(), String>
{
    assert_open_gap_starts(
        "renamed dependency",
        Case::audit()
            .with("wscore/tests/other.rs", OWNER_PACKAGE_TEST)
            .with(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndisc = { path = \"../wscore\", package = \"wscore\" }\n",
            )
            .with(
                "app/src/lib.rs",
                "pub fn checkout(price: u32) -> u32 {\n    disc::discount(price)\n}\n",
            ),
    )
}

#[test]
fn an_unrelated_test_beside_a_changed_argument_still_starts_the_same_repair() -> Result<(), String>
{
    assert_open_gap_starts(
        "transformed argument",
        Case::audit()
            .with("wscore/tests/other.rs", OWNER_PACKAGE_TEST)
            .with(
                "app/src/lib.rs",
                "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price + 1)\n}\n",
            ),
    )
}

#[test]
fn an_argument_the_intermediate_function_changes_is_not_noticed() -> Result<(), String> {
    // `checkout(100)` hands 101 to `discount`, so the boundary input is not
    // the one the test pins.
    assert_not_noticed(
        "transformed argument",
        Case::audit().with(
            "app/src/lib.rs",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price + 1)\n}\n",
        ),
        "1 found",
    )
}

#[test]
fn a_result_the_intermediate_function_changes_is_not_noticed() -> Result<(), String> {
    // The pin `== 91` is on `discount(..) + 1`, not on what `discount` returns.
    assert_not_noticed(
        "transformed result",
        Case::audit()
            .with(
                "app/src/lib.rs",
                "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price) + 1\n}\n",
            )
            .with(
                "app/tests/t.rs",
                "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(100), 91);\n}\n",
            ),
        "1 found",
    )
}

#[test]
fn a_loose_check_is_not_an_exact_pin() -> Result<(), String> {
    assert_not_noticed(
        "loose check",
        Case::audit().with(
            "app/tests/t.rs",
            "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert!(checkout(100) > 0);\n}\n",
        ),
        "1 found",
    )
}

#[test]
fn an_input_off_the_boundary_is_not_noticed() -> Result<(), String> {
    assert_not_noticed(
        "off the boundary",
        Case::audit().with(
            "app/tests/t.rs",
            "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(150), 140);\n}\n",
        ),
        "1 found",
    )
}

#[test]
fn a_renamed_dependency_keeps_the_earlier_no_path_answer() -> Result<(), String> {
    assert_not_noticed(
        "renamed dependency",
        Case::audit()
            .with(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndisc = { path = \"../wscore\", package = \"wscore\" }\n",
            )
            .with(
                "app/src/lib.rs",
                "pub fn checkout(price: u32) -> u32 {\n    disc::discount(price)\n}\n",
            ),
        NO_PATH,
    )
}

#[test]
fn a_dev_dependency_edge_keeps_the_earlier_no_path_answer() -> Result<(), String> {
    // Library code cannot see a dev-dependency, so the call is not an edge.
    assert_not_noticed(
        "dev dependency only",
        Case::audit().with(
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dev-dependencies]\nwscore = { path = \"../wscore\" }\n",
        ),
        NO_PATH,
    )
}

#[test]
fn a_same_named_function_in_another_crate_keeps_the_earlier_no_path_answer() -> Result<(), String> {
    assert_not_noticed(
        "same name elsewhere",
        Case::audit()
            .with(
                "Cargo.toml",
                "[workspace]\nmembers = [\"wscore\", \"app\", \"legacy\"]\nresolver = \"2\"\n",
            )
            .with(
                "legacy/Cargo.toml",
                "[package]\nname = \"legacy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            )
            .with(
                "legacy/src/lib.rs",
                "pub fn discount(price: u32) -> u32 {\n    price\n}\n",
            ),
        NO_PATH,
    )
}

#[test]
fn a_bare_import_of_the_function_is_not_followed_yet() -> Result<(), String> {
    // `use wscore::discount;` then `discount(price)` is a later slice: the bare
    // name is not callee identity across a crate edge.
    assert_not_noticed(
        "bare import",
        Case::audit().with(
            "app/src/lib.rs",
            "use wscore::discount;\n\npub fn checkout(price: u32) -> u32 {\n    discount(price)\n}\n",
        ),
        NO_PATH,
    )
}

#[test]
fn a_local_module_named_like_the_dependency_keeps_the_earlier_no_path_answer() -> Result<(), String>
{
    assert_not_noticed(
        "local module shadow",
        Case::audit().with(
            "app/src/lib.rs",
            "mod wscore {\n    pub fn discount(price: u32) -> u32 {\n        price\n    }\n}\n\npub fn checkout(price: u32) -> u32 {\n    wscore::discount(price)\n}\n",
        ),
        NO_PATH,
    )
}

#[test]
fn a_call_that_is_not_the_whole_tail_is_not_followed_as_a_forward() -> Result<(), String> {
    // The call result is bound and then shifted: the pin is not on it.
    assert_not_noticed(
        "result bound and shifted",
        Case::audit()
            .with(
                "app/src/lib.rs",
                "pub fn checkout(price: u32) -> u32 {\n    let net = wscore::discount(price);\n    net + 1\n}\n",
            )
            .with(
                "app/tests/t.rs",
                "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(100), 91);\n}\n",
            ),
        "1 found",
    )
}

#[test]
fn a_wrapper_in_the_same_member_package_is_noticed_through_its_own_crate_name() -> Result<(), String>
{
    // The identity half of the fix without a crate edge: `checkout` and
    // `discount` both live in `wscore`, and its integration test imports
    // `wscore::checkout`. The virtual workspace root holds no package named
    // `wscore`, so this import read as foreign before.
    let repo = Repo::create(
        &Case::audit()
            .with(
                "Cargo.toml",
                "[workspace]\nmembers = [\"wscore\"]\nresolver = \"2\"\n",
            )
            .with(
                "wscore/src/lib.rs",
                &format!("{WSCORE_OLD}\npub fn checkout(price: u32) -> u32 {{\n    discount(price)\n}}\n"),
            )
            .without("app/Cargo.toml")
            .without("app/src/lib.rs")
            .without("app/tests/t.rs")
            .with(
                "wscore/tests/t.rs",
                "use wscore::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(100), 90);\n}\n",
            ),
    )?;
    // The change on `feature` rewrites the whole file, so rebuild it here.
    repo.write(
        "wscore/src/lib.rs",
        &format!("{WSCORE_NEW}\npub fn checkout(price: u32) -> u32 {{\n    discount(price)\n}}\n"),
    )?;
    repo.git(&["commit", "-qam", "same package"])?;
    let check = repo.check(&[])?;
    assert_changed_subject("same package", &check)?;
    if !check.contains(NOTICED) {
        return Err(format!(
            "a same-package wrapper pin must read noticed:\n{check}"
        ));
    }
    Ok(())
}
