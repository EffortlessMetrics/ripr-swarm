//! #12831: when several repair starts are eligible for the change and
//! `ripr check` names one of them, bare `ripr repair` starts that one instead
//! of asking the user to repeat check's choice. Real-binary tests on feature
//! branches (so the change is scoped), with discriminating negatives.

use std::path::{Path, PathBuf};
use std::process::Output;

struct Repo {
    root: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
        ])
        .args(args)
        .output()
        .map_err(|error| format!("spawn git {args:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ripr(root: &Path, args: &[&str]) -> Result<Output, String> {
    std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| format!("spawn ripr {args:?}: {error}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A repository whose `main` holds `base` and whose current branch holds
/// `changed` (`feature_branch`) or whose change is committed on `main`.
fn repo(
    label: &str,
    base: &str,
    changed: &str,
    test: &str,
    feature_branch: bool,
) -> Result<Repo, String> {
    let root = std::env::temp_dir().join(format!("ripr-recstart-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(root.join("tests")).map_err(|e| e.to_string())?;
    let repo = Repo { root: root.clone() };
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(root.join(".gitignore"), "/target/\n").map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/lib.rs"), base).map_err(|e| e.to_string())?;
    std::fs::write(root.join("tests/t.rs"), test).map_err(|e| e.to_string())?;
    git(&root, &["init", "-q"])?;
    git(&root, &["config", "core.autocrlf", "false"])?;
    git(&root, &["branch", "-M", "main"])?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "base"])?;
    if feature_branch {
        git(&root, &["checkout", "-q", "-b", "feature"])?;
    }
    std::fs::write(root.join("src/lib.rs"), changed).map_err(|e| e.to_string())?;
    git(&root, &["add", "."])?;
    git(&root, &["commit", "-q", "-m", "change"])?;
    Ok(repo)
}

/// The boundary change on a feature branch, for the contrast case.
fn repo_with(label: &str, base: &str, changed: &str, test: &str) -> Result<Repo, String> {
    repo(label, base, changed, test, true)
}

const BOUNDARY_BASE: &str = "pub fn is_adult(age: u32) -> bool {\n    age > 18\n}\n";
const BOUNDARY_NEW: &str = "pub fn is_adult(age: u32) -> bool {\n    age >= 18\n}\n";
const BOUNDARY_TEST: &str =
    "use fx::is_adult;\n\n#[test]\nfn adult_far_above() {\n    assert!(is_adult(40));\n}\n";

const ERR_BASE: &str = "#[derive(Debug, PartialEq)]\npub enum ParseError {\n    Empty,\n    TooLong,\n}\n\npub fn parse(s: &str) -> Result<usize, ParseError> {\n    if s.is_empty() {\n        return Err(ParseError::Empty);\n    }\n    if s.len() > 10 {\n        return Err(ParseError::Empty);\n    }\n    Ok(s.len())\n}\n";
const ERR_TEST: &str = "use fx::parse;\n\n#[test]\nfn long_input_is_error() {\n    assert!(parse(\"abcdefghijklmnop\").is_err());\n}\n";

const FLIP_BASE: &str =
    "pub fn can_ship(stock: u32, blocked: bool) -> bool {\n    stock > 0 && !blocked\n}\n";
const FLIP_NEW: &str =
    "pub fn can_ship(stock: u32, blocked: bool) -> bool {\n    stock > 0 && blocked\n}\n";
const FLIP_TEST: &str = "use fx::can_ship;\n\n#[test]\nfn ships_when_in_stock() {\n    assert!(can_ship(3, false));\n}\n";

/// The seam id of the `ripr repair <id> --root ..` line check prints.
fn check_start_id(root: &Path) -> Result<Option<String>, String> {
    let check = ripr(root, &["check", "--base", "main"])?;
    Ok(text(&check.stdout).lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("ripr") && words.next() == Some("repair"))
            .then(|| words.next().map(str::to_string))
            .flatten()
    }))
}

fn attempts(root: &Path) -> usize {
    std::fs::read_dir(root.join("target/ripr/repair-attempts"))
        .map(|dir| {
            dir.flatten()
                .filter(|entry| entry.path().join("attempt.json").is_file())
                .count()
        })
        .unwrap_or(0)
}

/// Precondition for the narrowing claim: the same change, read without a diff
/// scope, leaves more than one eligible start, so bare repair can only ask.
fn assert_several_eligible_without_scope(
    label: &str,
    base: &str,
    changed: &str,
    test: &str,
) -> Result<(), String> {
    let repo = repo(&format!("{label}-pre"), base, changed, test, false)?;
    let repair = ripr(&repo.root, &["repair"])?;
    let err = text(&repair.stderr);
    let listed = err
        .lines()
        .filter(|line| line.trim_start().starts_with("ripr repair "))
        .count();
    if repair.status.code() != Some(3) || listed < 2 {
        return Err(format!(
            "{label}: precondition needs more than one eligible start:\n{err}"
        ));
    }
    Ok(())
}

fn assert_starts_check_recommendation(repo: &Repo, label: &str) -> Result<(), String> {
    let root = &repo.root;
    let id = check_start_id(root)?.ok_or_else(|| format!("{label}: check printed no start"))?;
    let repair = ripr(root, &["repair"])?;
    let (out, err) = (text(&repair.stdout), text(&repair.stderr));
    if repair.status.code() != Some(0) {
        return Err(format!("{label}: bare repair must start:\n{out}\n{err}"));
    }
    if !out.contains("Ready to repair") || err.contains("several_eligible_seams") {
        return Err(format!("{label}: not a started repair:\n{out}\n{err}"));
    }
    if attempts(root) != 1 {
        return Err(format!(
            "{label}: expected one attempt, found {}",
            attempts(root)
        ));
    }
    // The started attempt is the seam check recommended, not another one.
    let attempt_dir = std::fs::read_dir(root.join("target/ripr/repair-attempts"))
        .map_err(|e| e.to_string())?
        .flatten()
        .find(|entry| entry.path().join("attempt.json").is_file())
        .ok_or_else(|| format!("{label}: no attempt directory"))?
        .path();
    let attempt = std::fs::read_to_string(attempt_dir.join("attempt.json"))
        .map_err(|e| format!("{label}: attempt.json: {e}"))?;
    if !attempt.contains(&id) {
        return Err(format!(
            "{label}: attempt does not name check's seam {id}:\n{attempt}"
        ));
    }
    Ok(())
}

#[test]
fn boundary_change_starts_the_seam_check_recommends() -> Result<(), String> {
    assert_several_eligible_without_scope("boundary", BOUNDARY_BASE, BOUNDARY_NEW, BOUNDARY_TEST)?;
    let repo = repo("boundary", BOUNDARY_BASE, BOUNDARY_NEW, BOUNDARY_TEST, true)?;
    assert_starts_check_recommendation(&repo, "boundary-weak")
}

/// Guard: in scoped mode the unchanged `Err(Empty)` seam is already left off
/// the list, so this passes without the narrowing too.
#[test]
fn error_variant_change_starts_the_changed_branch_check_recommends() -> Result<(), String> {
    let changed = ERR_BASE.replace(
        "if s.len() > 10 {\n        return Err(ParseError::Empty);",
        "if s.len() > 10 {\n        return Err(ParseError::TooLong);",
    );
    assert_several_eligible_without_scope("errvariant", ERR_BASE, &changed, ERR_TEST)?;
    let repo = repo("errvariant", ERR_BASE, &changed, ERR_TEST, true)?;
    assert_starts_check_recommendation(&repo, "error-variant-weak")
}

/// #12849: check reads one finding as noticed (the returned value is pinned)
/// and the other as unknown, and offers no start. Bare repair agrees: it lists
/// no start, says why for both spots, and starts no attempt. Without a diff
/// scope the same change still lists two starts (the discriminating contrast).
#[test]
fn a_change_check_reads_as_noticed_and_unknown_lists_no_start() -> Result<(), String> {
    assert_several_eligible_without_scope("flip", FLIP_BASE, FLIP_NEW, FLIP_TEST)?;
    let repo = repo("flip", FLIP_BASE, FLIP_NEW, FLIP_TEST, true)?;
    let root = &repo.root;
    if let Some(id) = check_start_id(root)? {
        return Err(format!(
            "precondition: check must offer no start, offered {id}"
        ));
    }
    let repair = ripr(root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3)
        || !err.contains("code=no_eligible_seam")
        || err.contains("several_eligible_seams")
        || err.contains("Pick one")
    {
        return Err(format!("must refuse without listing starts:\n{err}"));
    }
    if !err.contains("already noticed by a test") || !err.contains("cannot tell from reading") {
        return Err(format!("must say why for both spots:\n{err}"));
    }
    if attempts(root) != 0 {
        return Err("no attempt may start".to_string());
    }
    Ok(())
}

/// Guard (passes with or without the narrowing): with the change committed on the default branch there is no
/// diff to scope to, so check's recommendation does not apply to bare repair
/// and the prompt stays.
#[test]
fn an_unscoped_change_keeps_the_selection() -> Result<(), String> {
    let repo = repo(
        "unscoped",
        BOUNDARY_BASE,
        BOUNDARY_NEW,
        BOUNDARY_TEST,
        false,
    )?;
    let root = &repo.root;
    let repair = ripr(root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3) || !err.contains("code=several_eligible_seams") {
        return Err(format!("unscoped bare repair must keep the prompt:\n{err}"));
    }
    if attempts(root) != 0 {
        return Err("no attempt may start without a selection".to_string());
    }
    Ok(())
}

const FIELD_BASE: &str = "pub struct Config {\n    pub retries: u32,\n    pub verbose: bool,\n}\n\npub fn default_config() -> Config {\n    Config {\n        retries: 3,\n        verbose: false,\n    }\n}\n";
const FIELD_NEW: &str = "pub struct Config {\n    pub retries: u32,\n    pub verbose: bool,\n}\n\npub fn default_config() -> Config {\n    Config {\n        retries: 5,\n        verbose: false,\n    }\n}\n";
const FIELD_TEST: &str = "use fx::default_config;\n\n#[test]\nfn default_config_is_quiet() {\n    let c = default_config();\n    assert!(!c.verbose);\n}\n";

/// #12861: a scoped change with open spots that cannot start closes on the
/// per-spot reasons, not on a repository tour that sends the reader back to
/// check. The unscoped no-tests refusal keeps the repository route.
#[test]
fn open_spots_on_the_change_close_on_the_per_spot_reasons() -> Result<(), String> {
    let scoped = repo("nx-scoped", FLIP_BASE, FLIP_NEW, FLIP_TEST, true)?;
    let repair = ripr(&scoped.root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3)
        || !err.contains("Next: each changed spot below says what it still needs")
        || err.contains("explores the whole repository")
        || !err.contains("code=no_eligible_seam")
    {
        return Err(format!("scoped refusal must close on the reasons:\n{err}"));
    }
    let unscoped = repo("nx-unscoped", FLIP_BASE, FLIP_NEW, FLIP_TEST, false)?;
    let repair = ripr(&unscoped.root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3) || err.contains("each changed spot below says") {
        return Err(format!("unscoped read must not use the new step:\n{err}"));
    }
    // The unscoped FLIP fixture lists starts instead of refusing, so the
    // old-route contrast uses a refusing fixture read without a diff scope.
    let field = repo("nx-field", FIELD_BASE, FIELD_NEW, FIELD_TEST, false)?;
    let repair = ripr(&field.root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3)
        || !err.contains("code=no_eligible_seam")
        || !err.contains("explores the whole repository")
        || err.contains("each changed spot below says")
    {
        return Err(format!("unscoped refusal keeps the old route:\n{err}"));
    }
    Ok(())
}

/// #12867: a seam whose repair would begin in a proposed new test file has no
/// admitted test target, so the handoff card cannot be produced. The start is
/// refused up front (bare and by id) and leaves no attempt behind, instead of
/// publishing an attempt and then saying the card is unavailable.
#[test]
fn a_start_whose_card_cannot_be_produced_creates_no_attempt() -> Result<(), String> {
    let unit = repo("nocard", BOUNDARY_BASE, BOUNDARY_NEW, "", false)?;
    let root = &unit.root;
    let repair = ripr(root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3)
        || !err.contains("code=no_eligible_seam")
        || !err.contains("found no existing test to extend")
        || err.contains("handoff card is unavailable")
    {
        return Err(format!("bare repair must refuse up front:\n{err}"));
    }
    // Precondition: the exposure fixture produced the boundary seam and no
    // test reaches it, so its repair target can only be a proposed new file.
    let exposure = ripr(
        root,
        &["check", "--mode", "draft", "--format", "repo-exposure-json"],
    )?;
    let json: serde_json::Value = serde_json::from_slice(&exposure.stdout)
        .map_err(|error| format!("exposure JSON: {error}"))?;
    let seam = json["seams"]
        .as_array()
        .and_then(|seams| {
            seams.iter().find(|seam| {
                seam["kind"] == "predicate_boundary" && seam["expression"] == "age >= 18"
            })
        })
        .ok_or_else(|| "precondition: the boundary seam must exist".to_string())?;
    if seam["related_tests_total"] != 0 {
        return Err(format!("precondition: no test may reach the seam: {seam}"));
    }
    let seam_id = seam["seam_id"]
        .as_str()
        .ok_or_else(|| "precondition: the seam has an id".to_string())?
        .to_string();
    let by_id = ripr(root, &["repair", &seam_id])?;
    let err = text(&by_id.stderr);
    if by_id.status.code() != Some(3)
        || !err.contains("code=seam_not_repair_eligible")
        || !err.contains("found no existing test to extend")
        || err.contains("handoff card is unavailable")
    {
        return Err(format!("start by id must refuse up front:\n{err}"));
    }
    if attempts(root) != 0 {
        return Err("no attempt may exist when the card cannot be produced".to_string());
    }
    // The check screen agrees from the other side: with no test reaching the
    // change the finding is static-limited (`no path`), so it offers no start
    // and stays silent about the no-card refusal (the Silent branch of
    // `check_repair_route`). A finding that is not static-limited cannot have
    // a proposed target here, since it needs a related test.
    let scoped = repo("nocard-check", BOUNDARY_BASE, BOUNDARY_NEW, "", true)?;
    let check = ripr(&scoped.root, &["check", "--base", "main"])?;
    let screen = text(&check.stdout);
    if check_start_id(&scoped.root)?.is_some()
        || screen.contains("ripr repair ")
        || screen.contains("not complete enough to name a safe test target")
        || !screen.contains("Why no path")
    {
        return Err(format!(
            "check must offer no start and stay silent:\n{screen}"
        ));
    }
    // Contrast: with an existing test to extend, the same change starts.
    let with_test = repo_with("hascard", BOUNDARY_BASE, BOUNDARY_NEW, BOUNDARY_TEST)?;
    assert_starts_check_recommendation(&with_test, "existing test target")
}
