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

const RENAMED_APP_TOML: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndisc = { path = \"../wscore\", package = \"wscore\" }\n";
const RENAMED_APP_LIB: &str =
    "pub fn checkout(price: u32) -> u32 {\n    disc::discount(price)\n}\n";

/// The audit caller with the callee imported by name and called bare.
fn bare_lib(import: &str) -> String {
    format!("{import}\n\npub fn checkout(price: u32) -> u32 {{\n    discount(price)\n}}\n")
}

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
    /// The file `feature` rewrites, and its text there.
    changed: (&'static str, String),
    /// Where the finding must sit on the check screen.
    location: &'static str,
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
            changed: ("wscore/src/lib.rs", WSCORE_NEW.to_string()),
            location: "./wscore/src/lib.rs:2",
        }
    }

    /// The audit case with the callee in `wscore/src/<file>` under the public
    /// module declared by `lib_rs`, called as `wscore::<path>::discount`.
    fn in_file(lib_rs: &str, file: &'static str, path: &str) -> Self {
        let mut case = Self::audit()
            .with("wscore/src/lib.rs", lib_rs)
            .with(file, WSCORE_OLD)
            .with(
                "app/src/lib.rs",
                &format!("pub fn checkout(price: u32) -> u32 {{\n    wscore::{path}::discount(price)\n}}\n"),
            );
        case.changed = (file, WSCORE_NEW.to_string());
        case.location = match file {
            "wscore/src/pricing.rs" => "./wscore/src/pricing.rs:2",
            _ => "./wscore/src/pricing/rules.rs:2",
        };
        case
    }

    /// app -> mid -> wscore: `app` depends on `mid` only, `mid` on `wscore`.
    fn chain() -> Self {
        Self::audit()
            .with(
                "Cargo.toml",
                "[workspace]\nmembers = [\"wscore\", \"mid\", \"app\"]\nresolver = \"2\"\n",
            )
            .with(
                "mid/Cargo.toml",
                "[package]\nname = \"mid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nwscore = { path = \"../wscore\" }\n",
            )
            .with("mid/src/lib.rs", APP_LIB)
            .with(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmid = { path = \"../mid\" }\n",
            )
            .with(
                "app/src/lib.rs",
                "pub fn run(price: u32) -> u32 {\n    mid::checkout(price)\n}\n",
            )
            .with(
                "app/tests/t.rs",
                "use app::run;\n\n#[test]\nfn run_discounts() {\n    assert_eq!(run(100), 90);\n}\n",
            )
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
        repo.write(case.changed.0, &case.changed.1)?;
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
    assert_changed_subject_at(case, check, "./wscore/src/lib.rs:2")
}

fn assert_changed_subject_at(case: &str, check: &str, location: &str) -> Result<(), String> {
    if !check.contains(&format!("File: {location}"))
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
    assert_changed_subject_at(case, &check, base.location)?;
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
    assert_changed_subject_at(case, &check, base.location)?;
    if !check.contains(NOT_NOTICED) || check.contains(NOTICED) || !check.contains(summary_part) {
        return Err(format!(
            "{case}: must not read as noticed, and must read `{summary_part}`:\n{check}"
        ));
    }
    assert_no_repair_start(case, &repo, &check)
}

/// A positive: check reads the changed line as noticed and exposed, offers no
/// start, and bare repair says the change is already noticed.
fn assert_noticed(case: &str, base: Case) -> Result<(), String> {
    let repo = Repo::create(&base)?;
    let check = repo.check(&[])?;
    assert_changed_subject_at(case, &check, base.location)?;
    if !check.contains(NOTICED) || !check.contains("Static exposure: exposed") {
        return Err(format!("{case}: must read noticed and exposed:\n{check}"));
    }
    assert_no_repair_start(case, &repo, &check)?;
    assert_repair_says_noticed(case, &repo)
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
fn an_unrelated_test_beside_a_renamed_loose_check_still_starts_the_same_repair()
-> Result<(), String> {
    // The renamed dependency is followed now; one change from the noticed
    // renamed case below (the pin is loose) keeps the gap and the start.
    assert_open_gap_starts(
        "renamed dependency, loose check",
        Case::audit()
            .with("wscore/tests/other.rs", OWNER_PACKAGE_TEST)
            .with("app/Cargo.toml", RENAMED_APP_TOML)
            .with("app/src/lib.rs", RENAMED_APP_LIB)
            .with(
                "app/tests/t.rs",
                "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert!(checkout(100) > 0);\n}\n",
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
fn a_renamed_dependency_is_noticed_by_check_and_repair() -> Result<(), String> {
    // `disc = { path = "../wscore", package = "wscore" }`: the call is spelled
    // through the key, `disc::discount(price)`.
    let repo = Repo::create(
        &Case::audit()
            .with("app/Cargo.toml", RENAMED_APP_TOML)
            .with("app/src/lib.rs", RENAMED_APP_LIB),
    )?;
    let check = repo.check(&[])?;
    assert_changed_subject("renamed dependency", &check)?;
    if !check.contains(NOTICED) || !check.contains("Static exposure: exposed") {
        return Err(format!("a renamed dependency must read noticed:\n{check}"));
    }
    assert_no_repair_start("renamed dependency", &repo, &check)?;
    assert_repair_says_noticed("renamed dependency", &repo)
}

#[test]
fn a_renamed_dependency_spelled_by_its_package_name_is_not_followed() -> Result<(), String> {
    // Same renamed manifest, but the call still says `wscore::`, which the
    // rename removed from the caller's namespace.
    assert_not_noticed(
        "package name after rename",
        Case::audit().with("app/Cargo.toml", RENAMED_APP_TOML),
        NO_PATH,
    )
}

#[test]
fn a_dependency_reachable_under_two_names_is_ambiguous() -> Result<(), String> {
    assert_not_noticed(
        "two names for one package",
        Case::audit()
            .with(
                "app/Cargo.toml",
                &format!("{APP_TOML}disc = {{ path = \"../wscore\", package = \"wscore\" }}\n"),
            )
            .with("app/src/lib.rs", RENAMED_APP_LIB),
        NO_PATH,
    )
}

#[test]
fn a_rename_that_points_at_another_package_is_not_followed() -> Result<(), String> {
    // `disc` is the other crate; `wscore` is not in the caller's namespace as
    // the changed package.
    assert_not_noticed(
        "rename to another package",
        Case::audit()
            .with(
                "Cargo.toml",
                "[workspace]\nmembers = [\"wscore\", \"app\", \"legacy\"]\nresolver = \"2\"\n",
            )
            .with(
                "legacy/Cargo.toml",
                "[package]\nname = \"legacy\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            )
            .with("legacy/src/lib.rs", "pub fn other() {}\n")
            .with(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndisc = { path = \"../legacy\", package = \"legacy\" }\n",
            )
            .with("app/src/lib.rs", RENAMED_APP_LIB),
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
fn a_bare_call_after_a_use_by_name_is_noticed_by_check_and_repair() -> Result<(), String> {
    for (label, import) in [
        ("single use", "use wscore::discount;"),
        ("list use", "use wscore::{self, discount};"),
    ] {
        let repo = Repo::create(&Case::audit().with("app/src/lib.rs", &bare_lib(import)))?;
        let check = repo.check(&[])?;
        assert_changed_subject(label, &check)?;
        if !check.contains(NOTICED) || !check.contains("Static exposure: exposed") {
            return Err(format!(
                "{label}: the imported bare call must read noticed:\n{check}"
            ));
        }
        assert_no_repair_start(label, &repo, &check)?;
        assert_repair_says_noticed(label, &repo)?;
    }
    Ok(())
}

#[test]
fn a_bare_call_after_a_use_of_a_renamed_dependency_is_noticed() -> Result<(), String> {
    let repo = Repo::create(
        &Case::audit()
            .with("app/Cargo.toml", RENAMED_APP_TOML)
            .with("app/src/lib.rs", &bare_lib("use disc::discount;")),
    )?;
    let check = repo.check(&[])?;
    assert_changed_subject("renamed and imported", &check)?;
    if !check.contains(NOTICED) {
        return Err(format!("renamed + imported must read noticed:\n{check}"));
    }
    assert_repair_says_noticed("renamed and imported", &repo)
}

#[test]
fn an_imported_bare_call_keeps_the_argument_and_pin_rules() -> Result<(), String> {
    let lib = bare_lib("use wscore::discount;");
    let shifted = lib.replace("discount(price)\n", "discount(price + 1)\n");
    assert_ne!(lib, shifted, "the fixture must change the argument");
    assert_not_noticed(
        "imported, transformed argument",
        Case::audit().with("app/src/lib.rs", &shifted),
        "1 found",
    )?;
    assert_not_noticed(
        "imported, off the boundary",
        Case::audit().with("app/src/lib.rs", &lib).with(
            "app/tests/t.rs",
            "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(150), 140);\n}\n",
        ),
        "1 found",
    )
}

#[test]
fn a_glob_import_does_not_name_the_function() -> Result<(), String> {
    assert_not_noticed(
        "glob import",
        Case::audit().with("app/src/lib.rs", &bare_lib("use wscore::*;")),
        NO_PATH,
    )
}

#[test]
fn an_aliased_or_nested_import_does_not_name_the_function() -> Result<(), String> {
    for (label, lib) in [
        (
            "alias",
            "use wscore::discount as disc;\n\npub fn checkout(price: u32) -> u32 {\n    disc(price)\n}\n".to_string(),
        ),
        (
            "alias, original name called",
            bare_lib("use wscore::discount as disc;"),
        ),
        (
            "import inside the function",
            "pub fn checkout(price: u32) -> u32 {\n    use wscore::discount;\n    discount(price)\n}\n".to_string(),
        ),
        (
            "import under cfg",
            bare_lib("#[cfg(test)]\nuse wscore::discount;"),
        ),
    ] {
        assert_not_noticed(label, Case::audit().with("app/src/lib.rs", &lib), NO_PATH)?;
    }
    Ok(())
}

#[test]
fn a_name_that_rebinds_the_crate_segment_stops_the_imported_call() -> Result<(), String> {
    for (label, prefix) in [
        ("use as crate name", "use legacy as wscore;\n"),
        (
            "use crate path as crate name",
            "use crate::shim as wscore;\n",
        ),
        ("struct of the crate name", "struct wscore;\n"),
        ("enum of the crate name", "enum wscore {\n    One,\n}\n"),
    ] {
        let lib = bare_lib(&format!("{prefix}use wscore::discount;"));
        assert_not_noticed(label, Case::audit().with("app/src/lib.rs", &lib), NO_PATH)?;
    }
    Ok(())
}

#[test]
fn a_struct_field_named_like_the_function_does_not_stop_the_imported_call() -> Result<(), String> {
    let lib = format!(
        "{}\npub struct Rates {{\n    pub discount: u32,\n}}\n",
        bare_lib("use wscore::discount;")
    );
    let repo = Repo::create(&Case::audit().with("app/src/lib.rs", &lib))?;
    let check = repo.check(&[])?;
    assert_changed_subject("field named like the callee", &check)?;
    if !check.contains(NOTICED) {
        return Err(format!("a field is not a rebinding:\n{check}"));
    }
    assert_repair_says_noticed("field named like the callee", &repo)
}

#[test]
fn a_local_function_of_the_same_name_shadows_the_import() -> Result<(), String> {
    // The call may mean the local `discount`, which `price` never reaches
    // `wscore` through.
    assert_not_noticed(
        "local shadow",
        Case::audit().with(
            "app/src/lib.rs",
            &format!(
                "{}\nfn discount(price: u32) -> u32 {{\n    price\n}}\n",
                bare_lib("use wscore::discount;")
            ),
        ),
        NO_PATH,
    )?;
    // A local binding of the name inside the caller.
    assert_not_noticed(
        "local binding",
        Case::audit().with(
            "app/src/lib.rs",
            "use wscore::discount;\n\npub fn checkout(price: u32) -> u32 {\n    let discount = |p: u32| p;\n    discount(price)\n}\n",
        ),
        NO_PATH,
    )
}

#[test]
fn a_name_that_exists_in_two_crates_is_ambiguous_even_when_imported() -> Result<(), String> {
    assert_not_noticed(
        "same name elsewhere, imported",
        Case::audit()
            .with("app/src/lib.rs", &bare_lib("use wscore::discount;"))
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
fn an_import_of_the_function_from_an_undeclared_dependency_is_not_followed() -> Result<(), String> {
    assert_not_noticed(
        "import without dependency edge",
        Case::audit()
            .with("app/src/lib.rs", &bare_lib("use wscore::discount;"))
            .with(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dev-dependencies]\nwscore = { path = \"../wscore\" }\n",
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
    // The virtual workspace root is a manifest besides `wscore`'s own, and
    // `discount` is a top-level `pub fn` of `wscore/src/lib.rs`, so the
    // pre-test passes and repair agrees with check.
    assert_repair_says_noticed("same package", &repo)
}

// Slice 4: public submodule paths and chains of more than one crate.

const PRICING_LIB: &str = "pub mod pricing;\n";
const PRICING_FILE: &str = "wscore/src/pricing.rs";

fn pricing_case() -> Case {
    Case::in_file(PRICING_LIB, PRICING_FILE, "pricing")
}

/// The same callee with the caller spelling it differently: one change from
/// `pricing_case`, the app library text.
fn pricing_with_app(app_lib: &str) -> Case {
    pricing_case().with("app/src/lib.rs", app_lib)
}

#[test]
fn a_call_through_a_public_file_module_is_noticed_by_check_and_repair() -> Result<(), String> {
    assert_noticed("file module", pricing_case())?;
    // `pricing/mod.rs` is the same module.
    let mut nested_layout =
        Case::in_file(PRICING_LIB, "wscore/src/pricing/mod.rs", "pricing").without(PRICING_FILE);
    nested_layout.location = "./wscore/src/pricing/mod.rs:2";
    assert_noticed("mod.rs module", nested_layout)?;
    // Two levels, the second declared in `pricing/mod.rs`.
    let two_levels = Case::in_file(PRICING_LIB, "wscore/src/pricing/rules.rs", "pricing::rules")
        .with("wscore/src/pricing/mod.rs", "pub mod rules;\n");
    assert_noticed("two file modules", two_levels)
}

#[test]
fn a_call_through_an_inline_public_module_is_noticed() -> Result<(), String> {
    let indented = WSCORE_OLD
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>();
    let mut case = Case::audit()
        .with(
            "wscore/src/lib.rs",
            &format!("pub mod pricing {{\n{indented}}}\n"),
        )
        .with(
            "app/src/lib.rs",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::pricing::discount(price)\n}\n",
        );
    let changed = WSCORE_NEW
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>();
    case.changed = (
        "wscore/src/lib.rs",
        format!("pub mod pricing {{\n{changed}}}\n"),
    );
    case.location = "./wscore/src/lib.rs:3";
    assert_noticed("inline module", case)
}

#[test]
fn a_bare_call_after_a_use_through_a_public_module_is_noticed() -> Result<(), String> {
    for (label, import) in [
        ("single use", "use wscore::pricing::discount;"),
        ("list use", "use wscore::pricing::{self, discount};"),
    ] {
        assert_noticed(label, pricing_with_app(&bare_lib(import)))?;
    }
    Ok(())
}

#[test]
fn a_renamed_dependency_reaches_through_a_public_module() -> Result<(), String> {
    assert_noticed(
        "renamed, qualified",
        pricing_with_app(
            "pub fn checkout(price: u32) -> u32 {\n    disc::pricing::discount(price)\n}\n",
        )
        .with("app/Cargo.toml", RENAMED_APP_TOML),
    )?;
    assert_noticed(
        "renamed, imported",
        pricing_with_app(&bare_lib("use disc::pricing::discount;"))
            .with("app/Cargo.toml", RENAMED_APP_TOML),
    )
}

#[test]
fn a_path_that_does_not_name_the_module_the_function_is_in_is_not_followed() -> Result<(), String> {
    for (label, app_lib) in [
        (
            "module left out",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price)\n}\n".to_string(),
        ),
        (
            "another module",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::other::discount(price)\n}\n".to_string(),
        ),
        (
            "module in the wrong place",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::pricing::more::discount(price)\n}\n".to_string(),
        ),
        (
            "module path used without the crate name",
            "pub fn checkout(price: u32) -> u32 {\n    pricing::discount(price)\n}\n".to_string(),
        ),
        ("import without the module", bare_lib("use wscore::discount;")),
        (
            "import of the module only",
            "use wscore::pricing;\n\npub fn checkout(price: u32) -> u32 {\n    pricing::discount(price)\n}\n".to_string(),
        ),
        (
            "aliased import",
            "use wscore::pricing::discount as d;\n\npub fn checkout(price: u32) -> u32 {\n    d(price)\n}\n".to_string(),
        ),
        ("glob of the module", bare_lib("use wscore::pricing::*;")),
    ] {
        assert_not_noticed(label, pricing_with_app(&app_lib), NO_PATH)?;
    }
    Ok(())
}

#[test]
fn a_module_that_is_not_plainly_public_is_not_followed() -> Result<(), String> {
    // Each case changes the declaration of `pricing` and nothing else.
    for (label, lib) in [
        ("private module", "mod pricing;\n"),
        ("crate-visible module", "pub(crate) mod pricing;\n"),
        ("restricted-in module", "pub(in crate) mod pricing;\n"),
        ("cfg module", "#[cfg(feature = \"x\")]\npub mod pricing;\n"),
        (
            "remapped module",
            "#[path = \"pricing.rs\"]\npub mod pricing;\n",
        ),
        (
            "module declared twice",
            "pub mod pricing;\npub mod pricing;\n",
        ),
        (
            "macro that could declare modules",
            "macro_rules! m { () => {}; }\npub mod pricing;\n",
        ),
    ] {
        assert_not_noticed(label, Case::in_file(lib, PRICING_FILE, "pricing"), NO_PATH)?;
    }
    Ok(())
}

#[test]
fn a_module_with_two_files_or_a_restricted_function_is_not_followed() -> Result<(), String> {
    assert_not_noticed(
        "both module files",
        pricing_case().with("wscore/src/pricing/mod.rs", "pub fn other() {}\n"),
        NO_PATH,
    )?;
    let restricted = WSCORE_OLD.replacen("pub fn", "pub(crate) fn", 1);
    let mut case = pricing_case().with(PRICING_FILE, &restricted);
    case.changed = (
        PRICING_FILE,
        WSCORE_NEW.replacen("pub fn", "pub(crate) fn", 1),
    );
    assert_not_noticed("crate-visible function", case, NO_PATH)
}

#[test]
fn a_public_module_call_keeps_the_argument_and_pin_rules() -> Result<(), String> {
    assert_not_noticed(
        "module path, transformed argument",
        pricing_with_app(
            "pub fn checkout(price: u32) -> u32 {\n    wscore::pricing::discount(price + 1)\n}\n",
        ),
        "1 found",
    )?;
    assert_not_noticed(
        "module path, off the boundary",
        pricing_case().with(
            "app/tests/t.rs",
            "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(150), 140);\n}\n",
        ),
        "1 found",
    )?;
    assert_not_noticed(
        "module path, bound then shifted",
        pricing_with_app(
            "pub fn checkout(price: u32) -> u32 {\n    let net = wscore::pricing::discount(price);\n    net + 1\n}\n",
        )
        .with(
            "app/tests/t.rs",
            "use app::checkout;\n\n#[test]\nfn checkout_discounts() {\n    assert_eq!(checkout(100), 91);\n}\n",
        ),
        "1 found",
    )
}

#[test]
fn a_local_module_named_like_the_dependency_stops_the_module_path() -> Result<(), String> {
    assert_not_noticed(
        "local wscore module",
        pricing_with_app(
            "mod wscore {\n    pub mod pricing {\n        pub fn discount(price: u32) -> u32 {\n            price\n        }\n    }\n}\n\npub fn checkout(price: u32) -> u32 {\n    wscore::pricing::discount(price)\n}\n",
        ),
        NO_PATH,
    )
}

#[test]
fn an_import_group_that_rebinds_the_crate_name_stops_the_imported_call() -> Result<(), String> {
    assert_not_noticed(
        "in-group rebind",
        Case::audit().with(
            "app/src/lib.rs",
            &bare_lib("use wscore::{discount, inner as wscore};"),
        ),
        NO_PATH,
    )?;
    assert_not_noticed(
        "in-group rebind, module path",
        pricing_with_app(&bare_lib(
            "use wscore::pricing::{discount, inner as wscore};",
        )),
        NO_PATH,
    )?;
    assert_not_noticed(
        "sibling import ends in the crate name",
        Case::audit().with(
            "app/src/lib.rs",
            &bare_lib("use crate::shim::wscore;\nuse wscore::discount;"),
        ),
        NO_PATH,
    )
}

#[test]
fn another_plain_import_from_the_same_crate_does_not_stop_the_imported_call() -> Result<(), String>
{
    assert_noticed(
        "sibling import of the crate",
        Case::audit().with(
            "app/src/lib.rs",
            &bare_lib("use wscore::other;\nuse wscore::discount;"),
        ),
    )?;
    assert_noticed(
        "sibling import of the crate, module path",
        pricing_with_app(&bare_lib(
            "use wscore::more::{a, b};\nuse wscore::pricing::discount;",
        )),
    )
}

#[test]
fn a_call_chain_over_three_crates_is_noticed_by_check_and_repair() -> Result<(), String> {
    assert_noticed("app -> mid -> wscore", Case::chain())?;
    assert_noticed(
        "chain, middle crate imports by name",
        Case::chain().with("mid/src/lib.rs", &bare_lib("use wscore::discount;")),
    )?;
    assert_noticed(
        "chain, top crate imports by name",
        Case::chain().with(
            "app/src/lib.rs",
            "use mid::checkout;\n\npub fn run(price: u32) -> u32 {\n    checkout(price)\n}\n",
        ),
    )?;
    let mut through_module = Case::chain()
        .with("wscore/src/lib.rs", PRICING_LIB)
        .with(PRICING_FILE, WSCORE_OLD)
        .with(
            "mid/src/lib.rs",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::pricing::discount(price)\n}\n",
        );
    through_module.changed = (PRICING_FILE, WSCORE_NEW.to_string());
    through_module.location = "./wscore/src/pricing.rs:2";
    assert_noticed("chain through a public module", through_module)
}

#[test]
fn a_call_chain_stops_at_the_first_hop_that_does_not_forward() -> Result<(), String> {
    for (label, case) in [
        (
            "middle transforms the argument",
            Case::chain().with(
                "mid/src/lib.rs",
                "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price + 1)\n}\n",
            ),
        ),
        (
            "top transforms the argument",
            Case::chain().with(
                "app/src/lib.rs",
                "pub fn run(price: u32) -> u32 {\n    mid::checkout(price + 1)\n}\n",
            ),
        ),
        (
            "top shifts the result",
            Case::chain()
                .with(
                    "app/src/lib.rs",
                    "pub fn run(price: u32) -> u32 {\n    mid::checkout(price) + 1\n}\n",
                )
                .with(
                    "app/tests/t.rs",
                    "use app::run;\n\n#[test]\nfn run_discounts() {\n    assert_eq!(run(100), 91);\n}\n",
                ),
        ),
        (
            "top has no dependency edge to the middle crate",
            Case::chain().with(
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dev-dependencies]\nmid = { path = \"../mid\" }\n",
            ),
        ),
        (
            "middle has no dependency edge to the callee crate",
            Case::chain().with(
                "mid/Cargo.toml",
                "[package]\nname = \"mid\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dev-dependencies]\nwscore = { path = \"../wscore\" }\n",
            ),
        ),
    ] {
        let repo = Repo::create(&case)?;
        let check = repo.check(&[])?;
        assert_changed_subject_at(label, &check, case.location)?;
        if check.contains(NOTICED) {
            return Err(format!("{label}: must not read as noticed:\n{check}"));
        }
        assert_no_repair_start(label, &repo, &check)?;
    }
    Ok(())
}

#[test]
fn a_function_name_shared_along_the_chain_is_ambiguous() -> Result<(), String> {
    // A second `checkout` anywhere in the workspace makes the middle name
    // ambiguous; the chain must not pick one.
    let repo = Repo::create(&Case::chain().with("app/src/extra.rs", "pub fn checkout() {}\n"))?;
    let check = repo.check(&[])?;
    assert_changed_subject("ambiguous middle name", &check)?;
    if check.contains(NOTICED) {
        return Err(format!(
            "ambiguous middle name must not read noticed:\n{check}"
        ));
    }
    Ok(())
}

/// `pricing_case` with `prefix` lines added above the function in `pricing.rs`
/// (on the base and the changed commit alike), so the finding sits that many
/// lines lower.
fn pricing_with_prefix(prefix: &str, lines_added: usize, finding_line: usize) -> Case {
    let mut case = pricing_case().with(PRICING_FILE, &format!("{prefix}{WSCORE_OLD}"));
    case.changed = (PRICING_FILE, format!("{prefix}{WSCORE_NEW}"));
    assert_eq!(finding_line, 2 + lines_added);
    case.location = match finding_line {
        3 => "./wscore/src/pricing.rs:3",
        _ => "./wscore/src/pricing.rs:4",
    };
    case
}

#[test]
fn an_inner_attribute_on_a_module_file_stops_the_module_path() -> Result<(), String> {
    assert_not_noticed(
        "inner cfg on the module file",
        pricing_with_prefix("#![cfg(feature = \"x\")]\n", 1, 3),
        NO_PATH,
    )?;
    // The same shape without the gate is the noticed positive.
    assert_noticed("no inner attribute", pricing_with_prefix("\n", 1, 3))
}

#[test]
fn an_attribute_that_can_remove_the_function_stops_the_module_path() -> Result<(), String> {
    assert_not_noticed(
        "cfg on the function",
        pricing_with_prefix("#[cfg(feature = \"x\")]\n", 1, 3),
        NO_PATH,
    )?;
    assert_not_noticed(
        "cfg_attr that adds a cfg",
        pricing_with_prefix("#[cfg_attr(unix, cfg(feature = \"x\"))]\n", 1, 3),
        NO_PATH,
    )?;
    assert_not_noticed(
        "raw-identifier cfg",
        pricing_with_prefix("#[r#cfg(feature = \"x\")]\n", 1, 3),
        NO_PATH,
    )?;
    assert_not_noticed(
        "nested cfg_attr that adds a cfg",
        pricing_with_prefix(
            "#[cfg_attr(unix, cfg_attr(test, cfg(feature = \"x\")))]\n",
            1,
            3,
        ),
        NO_PATH,
    )?;
    // A `cfg_attr` that only documents does not change presence.
    assert_noticed(
        "cfg_attr that only documents",
        pricing_with_prefix("#[cfg_attr(docsrs, doc(hidden))]\n", 1, 3),
    )
}

#[test]
fn a_re_export_of_a_private_module_is_not_followed() -> Result<(), String> {
    // `wscore::discount` is a real public path through `pub use`, but the
    // declarations this reads name only the module path, which is private.
    assert_not_noticed(
        "re-export",
        Case::in_file(
            "mod pricing;\npub use pricing::discount;\n",
            PRICING_FILE,
            "pricing",
        )
        .with("app/src/lib.rs", APP_LIB),
        NO_PATH,
    )
}
