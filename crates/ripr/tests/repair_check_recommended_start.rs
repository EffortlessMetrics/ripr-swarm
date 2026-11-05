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
    repo_with_src_files(label, base, changed, test, feature_branch, &[])
}

/// [`repo`] plus extra files under `src/` (`(name, text)`), present in both
/// commits.
fn repo_with_src_files(
    label: &str,
    base: &str,
    changed: &str,
    test: &str,
    feature_branch: bool,
    src_files: &[(&str, &str)],
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
    for (name, text) in src_files {
        std::fs::write(root.join("src").join(name), text).map_err(|e| e.to_string())?;
    }
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

const BUILDER_BASE: &str = "pub struct Req {\n    pub url: String,\n    pub timeout: u32,\n}\n\nimpl Req {\n    pub fn new(url: &str) -> Self {\n        Req {\n            url: url.to_string(),\n            timeout: 30,\n        }\n    }\n\n    pub fn timeout(mut self, t: u32) -> Self {\n        self.timeout = t;\n        self\n    }\n}\n";
const BUILDER_TEST: &str = "use fx::Req;\n\n#[test]\nfn builder_sets_url() {\n    let r = Req::new(\"http://x\").timeout(10);\n    assert_eq!(r.url, \"http://x\");\n}\n";

/// #12959: a refused changed-field seam names its next step: the working stub
/// command, or the stub's own reason when it cannot write one. Nothing new
/// starts, and a refusal with other missing facts keeps the bare label.
#[test]
fn a_refused_field_seam_names_the_stub_or_why_there_is_none() -> Result<(), String> {
    let field = repo("field hint", FIELD_BASE, FIELD_NEW, FIELD_TEST, true)?;
    let repair = ripr(&field.root, &["repair"])?;
    let err = text(&repair.stderr);
    let stub_line = err
        .lines()
        .find(|line| line.contains("`retries: 5`") && line.contains("--kind field_construction"))
        .ok_or_else(|| format!("field seam must name the stub route:\n{err}"))?;
    if repair.status.code() != Some(3) || !err.contains("code=no_eligible_seam") {
        return Err(format!("still a refusal, no start:\n{err}"));
    }
    // The named command works. It is split like a shell would (the root may
    // hold spaces, so the bound root arrives quoted).
    let command = stub_line
        .split('`')
        .find(|part| part.starts_with("ripr agent stub"))
        .ok_or_else(|| format!("no stub command in: {stub_line}"))?;
    let words = shell_split(command)?;
    let args: Vec<&str> = words.iter().skip(1).map(String::as_str).collect();
    if !args.iter().any(|arg| arg.contains("field hint")) {
        return Err(format!("the root must keep its space: {args:?}"));
    }
    let stub = ripr(&field.root, &args)?;
    if stub.status.code() != Some(0) || !text(&stub.stdout).contains("default_config") {
        return Err(format!(
            "the named stub command must work:\n{}",
            text(&stub.stderr)
        ));
    }
    // By-value setter: the stub refuses, and the refusal says why.
    let changed = BUILDER_BASE.replace("self.timeout = t;", "self.timeout = t * 2;");
    let builder = repo("builderhint", BUILDER_BASE, &changed, BUILDER_TEST, true)?;
    let repair = ripr(&builder.root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3)
        || !err.contains("ripr cannot write a fill-in skeleton here because the change sets a field of a by-value `self`")
        || err.contains("--kind field_construction")
    {
        return Err(format!("builder must say why there is no stub:\n{err}"));
    }
    // Two field seams in one state with different next steps (a stub
    // command for the constructor, the stub's refusal for the by-value
    // setter) group, and the group line says the others may differ.
    let both = changed.replace("timeout: 30,", "timeout: 31,");
    let mixed = repo("buildermixed", BUILDER_BASE, &both, BUILDER_TEST, true)?;
    let err = text(&ripr(&mixed.root, &["repair"])?.stderr);
    let grouped = err.lines().any(|line| {
        line.contains(
            "other gaps in the same state; the next step may differ for them, see `ripr check`",
        )
    });
    if !grouped {
        return Err(format!("mixed hints must be flagged in the group:\n{err}"));
    }
    // Same hint kind: no flag.
    if text(&ripr(&builder.root, &["repair"])?.stderr).contains("may differ for them") {
        return Err("identical hints must not claim they differ".to_string());
    }
    // Negative: a field seam with more missing than the field value (no
    // tests at all) keeps the bare label and names no field stub.
    let bare = repo("fieldbare", FIELD_BASE, FIELD_NEW, "", true)?;
    // No test file at all (the helper writes an empty one): no test home.
    std::fs::remove_file(bare.root.join("tests/t.rs")).map_err(|e| e.to_string())?;
    git(&bare.root, &["add", "-A"])?;
    git(
        &bare.root,
        &["commit", "-q", "-m", "drop the empty test file"],
    )?;
    let err = text(&ripr(&bare.root, &["repair"])?.stderr);
    if !err.contains("`retries: 5`: repair route not ready (missing evidence: a test assertion that checks the changed field value; safe test target")
        || err.contains("--kind field_construction")
    {
        return Err(format!("extra missing facts keep the bare label:\n{err}"));
    }
    // Negative: a refusal about something else keeps its own label.
    let notest = repo("fieldneg", BOUNDARY_BASE, BOUNDARY_NEW, "", true)?;
    let err = text(&ripr(&notest.root, &["repair"])?.stderr);
    if err.contains("field_construction") {
        return Err(format!(
            "unrelated refusal must not name the field stub:\n{err}"
        ));
    }
    Ok(())
}

const WRITE_BASE: &str = "pub struct Counter {\n    pub count: u32,\n    pub other: u32,\n}\n\nimpl Counter {\n    pub fn new() -> Self {\n        Counter { count: 0, other: 0 }\n    }\n\n    pub fn bump(&mut self) {\n        self.count = 1;\n    }\n}\n";

fn write_test(body: &str) -> String {
    format!("use fx::Counter;\n\n#[test]\nfn t() {{\n{body}\n}}\n")
}

/// The classification of every finding `ripr check --format json` reports.
fn finding_classes(root: &Path) -> Result<Vec<String>, String> {
    let check = ripr(root, &["check", "--base", "main", "--format", "json"])?;
    let json: serde_json::Value =
        serde_json::from_slice(&check.stdout).map_err(|e| format!("check json: {e}"))?;
    Ok(json["findings"]
        .as_array()
        .ok_or("check json has no findings")?
        .iter()
        .filter_map(|f| f["classification"].as_str().map(str::to_string))
        .collect())
}

/// No start anywhere: check offers none, bare repair is refused and starts no
/// attempt. Returns what repair printed. `class` is the one finding the
/// change must produce (so a fixture that analyzed nothing cannot pass).
fn assert_no_write_start(
    label: &str,
    base: &str,
    changed: &str,
    test: &str,
    class: &str,
) -> Result<String, String> {
    let repo = repo(label, base, changed, test, true)?;
    let classes = finding_classes(&repo.root)?;
    if classes != [class] {
        return Err(format!(
            "{label}: expected one `{class}` finding, found {classes:?}"
        ));
    }
    if let Some(id) = check_start_id(&repo.root)? {
        return Err(format!("{label}: check must offer no start, offered {id}"));
    }
    let repair = ripr(&repo.root, &["repair"])?;
    let err = text(&repair.stderr);
    if repair.status.code() != Some(3) || attempts(&repo.root) != 0 {
        return Err(format!("{label}: repair must start nothing:\n{err}"));
    }
    Ok(err)
}

/// #11870: a changed `self.f = ..` write whose related test calls the owner
/// directly and reads the written field only with a relational check starts a
/// repair, and `ripr check` names the same start. Shared policy: the strong
/// read stays "nothing to do", and a read the policy cannot place (no read, a
/// sibling field, another binding, a later write, a by-value owner, an
/// equality, a skipped test) starts nothing in either command.
#[test]
fn a_weak_read_of_a_written_field_starts_the_repair_check_recommends() -> Result<(), String> {
    let changed = WRITE_BASE.replace("self.count = 1;", "self.count = 2;");
    let run = "    let mut c = Counter::new();\n    c.bump();\n";
    let weak = write_test(&format!("{run}    assert!(c.count > 0);"));
    let repo = repo("writeweak", WRITE_BASE, &changed, &weak, true)?;
    if finding_classes(&repo.root)? != ["weakly_exposed"] {
        return Err("the weak read must stay one weakly_exposed finding".to_string());
    }
    assert_starts_check_recommendation(&repo, "written-field-weak-read")?;
    // The started packet asks for the written field's FieldValue check.
    let packet: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo.root.join("target/ripr/workflow/agent-packet.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let missing = &packet["packets"][0]["missing_discriminators"][0];
    if missing["value"] != "count"
        || !missing["reason"]
            .as_str()
            .is_some_and(|r| r.contains("written field `count` is read only by a weak check"))
    {
        return Err(format!(
            "the packet must carry the FieldValue fact:\n{missing}"
        ));
    }
    if packet["packets"][0]["assertion_shape"]["kind"] != "field_equality" {
        return Err("the packet must ask for a field equality".to_string());
    }
    // Strong read: already checked, repair says so.
    let strong = write_test(&format!("{run}    assert_eq!(c.count, 2);"));
    let err = assert_no_write_start("writestrong", WRITE_BASE, &changed, &strong, "exposed")?;
    if !err.contains("Nothing to do") {
        return Err(format!("a strong read is nothing to do:\n{err}"));
    }
    // Checks that reach the field without an assertion fact: the weak
    // assertion is present in each, the exact check is not an assertion on
    // the field, and the start is still withheld (#11870 review).
    let verify = "fn verify(c: &Counter) {\n    assert_eq!(c.count, 2);\n}\n";
    for (label, body, tail) in [
        (
            "writealias",
            "    assert!(c.count > 0);\n    let n = c.count;\n    assert_eq!(n, 2);",
            "",
        ),
        (
            "writehelper",
            "    assert!(c.count > 0);\n    verify(&c);",
            verify,
        ),
        (
            "writedestructure",
            "    assert!(c.count > 0);\n    let Counter { count, .. } = c;\n    assert_eq!(count, 2);",
            "",
        ),
        (
            "writerefalias",
            "    assert!(c.count > 0);\n    let r = &c.count;\n    assert_eq!(*r, 2);",
            "",
        ),
        (
            "writeformat",
            "    assert!(c.count > 0);\n    assert_eq!(format!(\"{c:?}\"), \"Counter { count: 2, other: 0 }\");",
            "",
        ),
        (
            "writeformatplain",
            "    assert!(c.count > 0);\n    assert_eq!(format!(\"{c:#?}\"), \"x\");",
            "",
        ),
        (
            "writeifpanic",
            "    assert!(c.count > 0);\n    if c.count != 2 {\n        panic!(\"bad\");\n    }",
            "",
        ),
    ] {
        let test = format!("{}{tail}", write_test(&format!("{run}{body}")));
        assert_no_write_start(label, WRITE_BASE, &changed, &test, "static_unknown")?;
    }
    // A `Drop` that may exit while a failed assertion unwinds: no repair.
    let drop_impl = "\nimpl Drop for Counter {\n    fn drop(&mut self) {\n        std::process::exit(1);\n    }\n}\n";
    let (drop_base, drop_new) = (
        format!("{WRITE_BASE}{drop_impl}"),
        format!("{changed}{drop_impl}"),
    );
    let drop_repo = repo_with("writedrop", &drop_base, &drop_new, &weak)?;
    if check_start_id(&drop_repo.root)?.is_some() {
        return Err("a non-plain Drop owner type must start nothing".to_string());
    }
    // The related-test set is the same for `check` and `repair`: an exact
    // check in a second test withholds the start in both commands.
    let two = format!(
        "{}\n#[test]\nfn u() {{\n{run}    let n = c.count;\n    assert_eq!(n, 2);\n}}\n",
        write_test(&format!("{run}    assert!(c.count > 0);"))
    );
    assert_no_write_start("writetwo", WRITE_BASE, &changed, &two, "static_unknown")?;
    for (label, base, new, body) in [
        // The field is written and never read: nothing weak to strengthen.
        ("writeonly", WRITE_BASE, changed.clone(), format!("{run}    assert!(c.other == 0);")),
        ("writesibling", WRITE_BASE, changed.clone(), format!("{run}    assert!(c.other > 0);")),
        (
            "writeotherbinding",
            WRITE_BASE,
            changed.clone(),
            "    let mut c = Counter::new();\n    let d = Counter::new();\n    c.bump();\n    assert!(d.count > 0);".to_string(),
        ),
        (
            "writelate",
            WRITE_BASE,
            changed.clone(),
            format!("{run}    c.count = 9;\n    assert!(c.count > 0);"),
        ),
        (
            "writeeq",
            WRITE_BASE,
            changed.clone(),
            format!("{run}    assert!(c.count == 2);"),
        ),
        (
            "writecompound",
            WRITE_BASE,
            changed.clone(),
            format!("{run}    assert!(c.count > 0);\n    assert_eq!(c.count, 2);"),
        ),
    ] {
        assert_no_write_start(label, base, &new, &write_test(&body), "static_unknown")?;
    }
    // A by-value owner writes a moved copy: a read of the binding sees nothing.
    let by_value_base = WRITE_BASE.replace(
        "pub fn bump(&mut self) {\n        self.count = 1;\n    }",
        "pub fn bump(mut self) -> Self {\n        self.count = 1;\n        self\n    }",
    );
    let by_value_new = by_value_base.replace("self.count = 1;", "self.count = 2;");
    assert_no_write_start(
        "writebyvalue",
        &by_value_base,
        &by_value_new,
        &weak,
        "static_unknown",
    )?;
    // Skipped tests never run the read.
    for (label, attr) in [
        ("writeignored", "#[ignore]"),
        ("writepanics", "#[should_panic]"),
    ] {
        let skipped = weak.replace("#[test]", &format!("#[test]\n{attr}"));
        assert_no_write_start(label, WRITE_BASE, &changed, &skipped, "static_unknown")?;
    }
    Ok(())
}

/// #11870 remainder: a private or `pub(crate)` field written by a changed
/// setter starts a repair from a weak read only where the related test can
/// name the field and repair can edit the test: an inline test module. A
/// private field read from a child module file, a `pub(crate)` field read from
/// `tests/` (its own crate) and every read the policy cannot place start
/// nothing in either command.
#[test]
fn a_weak_read_of_a_private_or_crate_field_starts_only_where_the_test_can_name_it()
-> Result<(), String> {
    let private = WRITE_BASE.replace("pub count", "count");
    let crate_visible = WRITE_BASE.replace("pub count", "pub(crate) count");
    let inline = |lib: &str, read: &str| {
        format!(
            "{lib}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn t() {{\n        let mut c = Counter::new();\n        c.bump();\n{read}\n    }}\n}}\n"
        )
    };
    let weak = "        assert!(c.count > 0);";
    let bump = |lib: &str| lib.replace("self.count = 1;", "self.count = 2;");
    for (label, lib) in [("privinline", &private), ("crateinline", &crate_visible)] {
        let (base, new) = (inline(lib, weak), inline(&bump(lib), weak));
        let started = repo(label, &base, &new, "", true)?;
        if finding_classes(&started.root)? != ["weakly_exposed"] {
            return Err(format!("{label}: expected one weakly_exposed finding"));
        }
        assert_starts_check_recommendation(&started, label)?;
        // The same write read exactly, through a getter, sibling or not at
        // all, starts nothing.
        for (variant, read) in [
            ("strong", "        assert_eq!(c.count, 2);"),
            ("sibling", "        assert!(c.other > 0);"),
            ("none", "        assert!(c.other == 0);"),
            ("eq", "        assert!(c.count == 2);"),
            (
                "alias",
                "        assert!(c.count > 0);\n        let n = c.count;\n        assert_eq!(n, 2);",
            ),
        ] {
            let (base, new) = (inline(lib, read), inline(&bump(lib), read));
            let none = repo(&format!("{label}{variant}"), &base, &new, "", true)?;
            if check_start_id(&none.root)?.is_some() {
                return Err(format!("{label} {variant}: must start nothing"));
            }
            ripr(&none.root, &["repair"])?;
            if attempts(&none.root) != 0 {
                return Err(format!("{label} {variant}: repair started an attempt"));
            }
        }
    }
    // A private field observed through a getter in an inline test: no read of
    // the field, so no seam and no start.
    let getter_lib = private.replace(
        "    pub fn bump",
        "    pub fn count(&self) -> u32 {\n        self.count\n    }\n\n    pub fn bump",
    );
    let getter = "        assert!(c.count() > 0);";
    let (base, new) = (
        inline(&getter_lib, getter),
        inline(&bump(&getter_lib), getter),
    );
    let repo_getter = repo("privgetter", &base, &new, "", true)?;
    if check_start_id(&repo_getter.root)?.is_some() {
        return Err("a getter read of a private field must start nothing".to_string());
    }
    // A test the field is out of reach for: private from a child module file,
    // `pub(crate)` from the integration crate.
    let tests_mod = "use super::*;\n\n#[test]\nfn t() {\n    let mut c = Counter::new();\n    c.bump();\n    assert!(c.count > 0);\n}\n";
    let child = |lib: &str| format!("{lib}\n#[cfg(test)]\nmod tests;\n");
    let (base, new) = (child(&private), child(&bump(&private)));
    let private_child = repo_with_src_files(
        "privchild",
        &base,
        &new,
        "",
        true,
        &[("tests.rs", tests_mod)],
    )?;
    if check_start_id(&private_child.root)?.is_some() {
        return Err("a private field read from a child module file must start nothing".to_string());
    }
    let (base, new) = (child(&crate_visible), child(&bump(&crate_visible)));
    let crate_child = repo_with_src_files(
        "cratechild",
        &base,
        &new,
        "",
        true,
        &[("tests.rs", tests_mod)],
    )?;
    // The field is nameable there, but a child module file is no test surface
    // repair may edit, so check offers no start either.
    if check_start_id(&crate_child.root)?.is_some() {
        return Err("a child module test file is no editable test surface".to_string());
    }
    let integration =
        write_test("    let mut c = Counter::new();\n    c.bump();\n    assert!(c.count > 0);");
    let crate_integration = repo(
        "crateintegration",
        &crate_visible,
        &bump(&crate_visible),
        &integration,
        true,
    )?;
    if check_start_id(&crate_integration.root)?.is_some() {
        return Err("a pub(crate) field read from tests/ must start nothing".to_string());
    }
    Ok(())
}

/// Splits a command line the way the bound-root quoting writes it: single
/// quotes group, `'\''` is a literal quote, whitespace separates.
fn shell_split(command: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    for ch in command.chars() {
        match (ch, quoted) {
            ('\'', _) => {
                quoted = !quoted;
                started = true;
            }
            (c, false) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (c, _) => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(format!("unbalanced quote in: {command}"));
    }
    if started {
        words.push(current);
    }
    Ok(words)
}
