//! Named floor under the fixture gates' denominators (#15925).
//!
//! `cargo xtask fixtures` and `goldens check` compare an optional golden
//! surface only when its `expected/` file exists, and the corpus-driven gates
//! (`dogfood` and the corpus validators here) iterate whatever `cases` a
//! corpus still lists. Deleting a golden, or dropping a case, therefore removed
//! the comparison with it and every gate stayed green: the subject that would
//! have failed was no longer a subject.
//!
//! `policy/fixture-denominator-floor.txt` names each retained subject. A named
//! subject that is no longer in the tree is a violation, so shrinking a
//! denominator takes a reviewed edit to that file in the same change. Subjects
//! above the floor are allowed and disclosed; they are unprotected until the
//! floor is raised over them, which is why the count is printed on every run.
//!
//! The floor proves presence only. Whether a present golden or case still
//! discriminates belongs to the gate that reads it.

use crate::reports::{ANNOTATION_GOLDEN_SURFACES, HUMAN_GOLDEN_SURFACES};
use crate::{PolicyDisclosure, is_manifest_only_fixture_dir, normalize_path};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub(crate) const FLOOR_PATH: &str = "policy/fixture-denominator-floor.txt";
/// The regenerated floor, written beside the other reports when it differs.
pub(crate) const PROPOSAL_REPORT: &str = "fixture-denominator-floor.txt";

const GOLDEN: &str = "golden";
const CASE: &str = "case";
/// A corpus case is named by the first of these keys holding a string.
const CASE_IDENTITY_KEYS: [&str; 2] = ["id", "name"];
/// Directories that hold analyzed input or build output, never a gate corpus.
const SKIPPED_DIRECTORIES: [&str; 4] = ["input", "target", "node_modules", ".git"];
const LISTED_VIOLATIONS: usize = 20;

const HEADER: &str = "\
# Named floor under the fixture gates' denominators (#15925).
#
# `golden <dir> <surface>...`  the optional golden surfaces of one fixture; the
#                       runner compares each only while its file exists.
# `case <corpus>#<id>`  a case in a corpus `cases` array that a gate iterates.
#
# `cargo xtask check-fixture-contracts` fails when a line here names something
# that is no longer in the tree, so a denominator shrinks only through a
# reviewed edit to this file. Remove a line in the same change that retires
# its subject, and say why in the PR. Never remove one to turn a gate green.
#
# Subjects above the floor pass and are counted in the check output. Raise the
# floor over them by copying target/ripr/reports/fixture-denominator-floor.txt
# here after a passing `cargo xtask check-fixture-contracts`.
";

/// Every subject present under `fixtures_dir`, as floor lines.
pub(crate) fn observe(fixtures_dir: &Path) -> Result<BTreeSet<String>, String> {
    let mut entries = BTreeSet::new();
    if !fixtures_dir.is_dir() {
        return Ok(entries);
    }
    for path in sorted_children(fixtures_dir)? {
        if !path.is_dir() || is_manifest_only_fixture_dir(&path) {
            continue;
        }
        for surface in HUMAN_GOLDEN_SURFACES
            .iter()
            .chain(ANNOTATION_GOLDEN_SURFACES.iter())
        {
            let golden = path.join("expected").join(surface);
            if golden.is_file() {
                entries.insert(format!("{GOLDEN} {}", nameable(fixtures_dir, &golden)?));
            }
        }
    }
    collect_corpus_cases(fixtures_dir, fixtures_dir, &mut entries)?;
    Ok(entries)
}

/// A subject's path as a floor line can carry it. One the line format cannot
/// name is an error: skipping it would leave it outside the floor unannounced.
fn nameable(fixtures_dir: &Path, path: &Path) -> Result<String, String> {
    let text = normalize_path(path);
    // Judged below the fixtures root, which is the part a fixture author names.
    let below_root = path.strip_prefix(fixtures_dir).unwrap_or(path);
    if floor_path_is_plain(&normalize_path(below_root)) {
        return Ok(text);
    }
    Err(format!(
        "{text}: a fixture gate subject's path cannot contain whitespace, `#` or `\\`, because {FLOOR_PATH} could not name it; rename it"
    ))
}

fn sorted_children(dir: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut children = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", normalize_path(dir)))?
    {
        let entry =
            entry.map_err(|err| format!("failed to read {}: {err}", normalize_path(dir)))?;
        children.push(entry.path());
    }
    children.sort();
    Ok(children)
}

fn collect_corpus_cases(
    fixtures_dir: &Path,
    dir: &Path,
    entries: &mut BTreeSet<String>,
) -> Result<(), String> {
    for path in sorted_children(dir)? {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !SKIPPED_DIRECTORIES.contains(&name) {
                collect_corpus_cases(fixtures_dir, &path, entries)?;
            }
            continue;
        }
        if !name.ends_with(".json") {
            continue;
        }
        // An unreadable or non-UTF-8 file names no case; a floor line that
        // pointed into it is then reported as missing.
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for id in corpus_case_ids(&text) {
            entries.insert(format!("{CASE} {}#{id}", nameable(fixtures_dir, &path)?));
        }
    }
    Ok(())
}

/// The identities in a top-level `cases` array. Anything else names no case.
pub(crate) fn corpus_case_ids(text: &str) -> Vec<String> {
    if !text.contains("\"cases\"") {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(cases) = value.get("cases").and_then(Value::as_array) else {
        return Vec::new();
    };
    cases
        .iter()
        .filter_map(|case| {
            CASE_IDENTITY_KEYS
                .iter()
                .find_map(|key| case.get(key).and_then(Value::as_str))
        })
        // One floor line per case: an identity that could not round-trip
        // through a line is not named, and so is not protected.
        .filter(|id| !id.is_empty() && id.trim() == *id && !id.chars().any(char::is_control))
        .map(str::to_string)
        .collect()
}

/// Parse a floor file. A line that is not a well-formed subject is an error:
/// a floor that silently skipped a line would protect less than it shows.
pub(crate) fn parse_floor(text: &str) -> Result<BTreeSet<String>, String> {
    let mut entries = BTreeSet::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let malformed = || {
            format!(
                "line {}: expected `golden <expected-dir> <surface>...` or `case <corpus>#<id>`, found `{raw}`",
                index + 1
            )
        };
        if line != raw {
            return Err(malformed());
        }
        let subjects = match line.split_once(' ') {
            // One line per fixture keeps the file reviewable: the directory,
            // then each optional surface it retains.
            Some((GOLDEN, rest)) => {
                let mut parts = rest.split(' ');
                let dir = parts.next().filter(|dir| floor_path_is_plain(dir));
                let surfaces: Vec<&str> = parts.collect();
                let (Some(dir), false) = (dir, surfaces.is_empty()) else {
                    return Err(malformed());
                };
                if surfaces
                    .iter()
                    .any(|surface| !floor_path_is_plain(surface) || surface.contains('/'))
                {
                    return Err(malformed());
                }
                surfaces
                    .iter()
                    .map(|surface| format!("{GOLDEN} {dir}/{surface}"))
                    .collect()
            }
            Some((CASE, subject))
                if subject
                    .split_once('#')
                    .is_some_and(|(path, id)| floor_path_is_plain(path) && !id.is_empty()) =>
            {
                vec![line.to_string()]
            }
            _ => return Err(malformed()),
        };
        for subject in subjects {
            if !entries.insert(subject.clone()) {
                return Err(format!("line {}: `{subject}` is listed twice", index + 1));
            }
        }
    }
    Ok(entries)
}

fn floor_path_is_plain(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(char::is_whitespace)
        && !path.contains('#')
        && !path.contains('\\')
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub(crate) fn render_floor(entries: &BTreeSet<String>) -> String {
    let mut out = String::from(HEADER);
    // `case` sorts before `golden`, and a directory's surfaces are adjacent.
    let mut open_dir: Option<&str> = None;
    for entry in entries {
        let golden = entry
            .strip_prefix("golden ")
            .and_then(|path| path.rsplit_once('/'));
        match golden {
            Some((dir, surface)) if open_dir == Some(dir) => {
                out.push(' ');
                out.push_str(surface);
            }
            Some((dir, surface)) => {
                if open_dir.is_some() {
                    out.push('\n');
                }
                out.push_str(&format!("{GOLDEN} {dir} {surface}"));
                open_dir = Some(dir);
            }
            None => {
                out.push_str(entry);
                out.push('\n');
            }
        }
    }
    if open_dir.is_some() {
        out.push('\n');
    }
    out
}

pub(crate) struct FloorOutcome {
    pub(crate) violations: Vec<String>,
    pub(crate) disclosure: PolicyDisclosure,
    /// Subjects in the tree that the floor does not name.
    pub(crate) above: usize,
    /// The regenerated floor, when the tree holds subjects the floor does not
    /// name and nothing named is missing. A floor regenerated while a named
    /// subject is missing would drop that line, so none is offered then.
    pub(crate) proposal: Option<String>,
}

/// Compare the tree against the floor text. `floor_text` is `Err` when the
/// floor could not be read; that is a violation, never an empty floor.
pub(crate) fn evaluate(
    floor_text: Result<String, String>,
    observed: &BTreeSet<String>,
) -> FloorOutcome {
    let heading = "Fixture denominator floor".to_string();
    let floor = match floor_text
        .map_err(|err| format!("{FLOOR_PATH}: cannot read the denominator floor: {err}"))
        .and_then(|text| parse_floor(&text).map_err(|err| format!("{FLOOR_PATH}: {err}")))
    {
        Ok(floor) => floor,
        Err(violation) => {
            return FloorOutcome {
                violations: vec![violation],
                disclosure: PolicyDisclosure {
                    heading,
                    intro: "NOT_ESTABLISHED: the denominator floor could not be read, so no retained subject was checked.".to_string(),
                    items: Vec::new(),
                },
                above: observed.len(),
                proposal: Some(render_floor(observed)),
            };
        }
    };
    let mut violations = Vec::new();
    if floor.is_empty() {
        violations.push(format!(
            "{FLOOR_PATH} names no subject; an empty floor checks nothing"
        ));
    }
    let missing: Vec<&String> = floor.difference(observed).collect();
    for entry in missing.iter().take(LISTED_VIOLATIONS) {
        violations.push(format!(
            "{FLOOR_PATH}: `{entry}` is no longer in the tree; restore it, or remove this line in the same change that retires it and say why"
        ));
    }
    if missing.len() > LISTED_VIOLATIONS {
        violations.push(format!(
            "{FLOOR_PATH}: {} more named subjects are no longer in the tree",
            missing.len() - LISTED_VIOLATIONS
        ));
    }
    let above = observed.difference(&floor).count();
    let count = |kind: &str, set: &BTreeSet<String>| {
        set.iter()
            .filter(|entry| entry.split(' ').next() == Some(kind))
            .count()
    };
    let mut items = vec![
        format!(
            "golden surfaces: {} in the tree, {} named by the floor",
            count(GOLDEN, observed),
            count(GOLDEN, &floor)
        ),
        format!(
            "corpus cases: {} in the tree, {} named by the floor",
            count(CASE, observed),
            count(CASE, &floor)
        ),
    ];
    if above > 0 && missing.is_empty() {
        items.push(format!(
            "{above} subjects are above the floor and unprotected; copy target/ripr/reports/{PROPOSAL_REPORT} to {FLOOR_PATH} to raise it"
        ));
    } else if above > 0 {
        items.push(format!(
            "{above} subjects are above the floor and unprotected; no regenerated floor is offered while a named subject is missing"
        ));
    }
    let proposal = (above > 0 && missing.is_empty()).then(|| render_floor(observed));
    FloorOutcome {
        violations,
        disclosure: PolicyDisclosure {
            heading,
            intro: "Presence of each named golden surface and corpus case. Presence is not evidence that the subject still discriminates.".to_string(),
            items,
        },
        above,
        proposal,
    }
}

/// Read the committed floor and compare it with the tree under `fixtures_dir`.
pub(crate) fn check(fixtures_dir: &Path, floor_path: &Path) -> Result<FloorOutcome, String> {
    let observed = observe(fixtures_dir)?;
    let floor_text = fs::read_to_string(floor_path).map_err(|err| err.to_string());
    Ok(evaluate(floor_text, &observed))
}

/// A test that runs the production gate over a tree it built names that tree
/// as its floor. The committed floor describes the repository, not a
/// synthetic copy whose cases the test rewrites.
#[cfg(test)]
pub(crate) fn seed_floor_in_cwd() -> Result<(), String> {
    let floor = Path::new(FLOOR_PATH);
    if let Some(parent) = floor.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(floor, render_floor(&observe(Path::new("fixtures"))?)).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-denominator-floor-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        Ok(root)
    }

    fn write_file(path: &Path, text: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::write(path, text).map_err(|err| err.to_string())
    }

    /// One BDD fixture with two optional goldens, and one corpus of two cases.
    fn seed(root: &Path) -> Result<PathBuf, String> {
        let fixtures = root.join("fixtures");
        write_file(&fixtures.join("alpha/expected/check.json"), "{}\n")?;
        write_file(&fixtures.join("alpha/expected/human.txt"), "human\n")?;
        write_file(&fixtures.join("alpha/expected/sarif.json"), "{}\n")?;
        write_file(
            &fixtures.join("alpha/expected/panel/corpus.json"),
            r#"{"cases":[{"id":"first"},{"name":"second case"}]}"#,
        )?;
        // Analyzed input is not a gate corpus, whatever it contains.
        write_file(
            &fixtures.join("alpha/input/data.json"),
            r#"{"cases":[{"id":"not-a-subject"}]}"#,
        )?;
        Ok(fixtures)
    }

    /// `observe`, with lines relative to `root` as the gate sees them from
    /// the repository root. The floor refuses absolute paths.
    fn observe_in(root: &Path) -> Result<BTreeSet<String>, String> {
        let prefix = format!("{}/", normalize_path(root));
        Ok(observe(&root.join("fixtures"))?
            .into_iter()
            .map(|entry| entry.replacen(&prefix, "", 1))
            .collect())
    }

    #[test]
    fn observe_names_optional_goldens_and_corpus_cases_only() -> Result<(), String> {
        let root = temp_root("observe")?;
        seed(&root)?;
        let observed = observe_in(&root)?;
        let _ = fs::remove_dir_all(&root);
        assert_eq!(
            observed.into_iter().collect::<Vec<_>>(),
            [
                "case fixtures/alpha/expected/panel/corpus.json#first",
                "case fixtures/alpha/expected/panel/corpus.json#second case",
                "golden fixtures/alpha/expected/human.txt",
                "golden fixtures/alpha/expected/sarif.json",
            ]
        );
        Ok(())
    }

    #[test]
    fn a_deleted_golden_and_a_dropped_case_are_each_named() -> Result<(), String> {
        let root = temp_root("shrink")?;
        let fixtures = seed(&root)?;
        let floor = render_floor(&observe_in(&root)?);
        fs::remove_file(fixtures.join("alpha/expected/human.txt")).map_err(|e| e.to_string())?;
        write_file(
            &fixtures.join("alpha/expected/panel/corpus.json"),
            r#"{"cases":[{"id":"first"}]}"#,
        )?;
        let outcome = evaluate(Ok(floor.clone()), &observe_in(&root)?);
        // The control: the same floor over an untouched tree is clean.
        seed(&root)?;
        let control = evaluate(Ok(floor), &observe_in(&root)?);
        let _ = fs::remove_dir_all(&root);
        assert!(control.violations.is_empty(), "{:?}", control.violations);
        assert!(control.proposal.is_none());
        assert_eq!(outcome.violations.len(), 2, "{:?}", outcome.violations);
        assert!(
            outcome.violations[0].contains("corpus.json#second case` is no longer in the tree"),
            "{:?}",
            outcome.violations
        );
        assert!(
            outcome.violations[1].contains("expected/human.txt` is no longer in the tree"),
            "{:?}",
            outcome.violations
        );
        Ok(())
    }

    #[test]
    fn a_corpus_that_stops_parsing_loses_every_named_case() -> Result<(), String> {
        let root = temp_root("unparsed")?;
        let fixtures = seed(&root)?;
        let floor = render_floor(&observe_in(&root)?);
        write_file(
            &fixtures.join("alpha/expected/panel/corpus.json"),
            r#"{"cases":{}}"#,
        )?;
        let outcome = evaluate(Ok(floor), &observe_in(&root)?);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(outcome.violations.len(), 2, "{:?}", outcome.violations);
        Ok(())
    }

    #[test]
    fn subjects_above_the_floor_pass_and_are_disclosed() -> Result<(), String> {
        let root = temp_root("above")?;
        let fixtures = seed(&root)?;
        let floor = render_floor(&observe_in(&root)?);
        write_file(&fixtures.join("alpha/expected/github.txt"), "note\n")?;
        let outcome = evaluate(Ok(floor), &observe_in(&root)?);
        let _ = fs::remove_dir_all(&root);
        assert!(outcome.violations.is_empty(), "{:?}", outcome.violations);
        assert!(
            outcome
                .disclosure
                .items
                .iter()
                .any(|item| item.starts_with("1 subjects are above the floor")),
            "{:?}",
            outcome.disclosure.items
        );
        let proposal = outcome
            .proposal
            .ok_or("a differing tree proposes a floor")?;
        assert!(
            proposal.contains("alpha/expected github.txt human.txt sarif.json\n"),
            "{proposal}"
        );
        Ok(())
    }

    /// Copying a regenerated floor must never be the way a removal is
    /// accepted: with a named subject missing, none is offered.
    #[test]
    fn a_removal_beside_an_addition_offers_no_regenerated_floor() -> Result<(), String> {
        let root = temp_root("mixed")?;
        let fixtures = seed(&root)?;
        let floor = render_floor(&observe_in(&root)?);
        fs::remove_file(fixtures.join("alpha/expected/human.txt")).map_err(|e| e.to_string())?;
        write_file(&fixtures.join("alpha/expected/github.txt"), "note\n")?;
        let outcome = evaluate(Ok(floor), &observe_in(&root)?);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(outcome.violations.len(), 1, "{:?}", outcome.violations);
        assert_eq!(outcome.above, 1);
        assert!(outcome.proposal.is_none(), "{:?}", outcome.proposal);
        assert!(
            outcome
                .disclosure
                .items
                .iter()
                .any(|item| item.contains("no regenerated floor is offered")),
            "{:?}",
            outcome.disclosure.items
        );
        Ok(())
    }

    /// The verdict of the production command, not of `evaluate` alone: a
    /// deleted golden beside an unprotected one fails `check-fixture-contracts`,
    /// and the proposal an earlier passing run wrote does not survive it.
    #[test]
    fn the_production_gate_refuses_a_deleted_golden() -> Result<(), String> {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        crate::tests::with_temp_cwd("denominator-floor-production", |root| {
            crate::tests::copy_dir_recursive(&repo.join("fixtures"), &root.join("fixtures"))?;
            seed_floor_in_cwd()?;
            let proposal = root.join("target/ripr/reports").join(PROPOSAL_REPORT);
            let golden = root.join("fixtures/boundary_gap/expected/human.txt");
            let kept = fs::read_to_string(&golden).map_err(|err| err.to_string())?;
            // Lower the floor by its last line, so one subject sits above it.
            let floor = fs::read_to_string(FLOOR_PATH).map_err(|err| err.to_string())?;
            let lowered = floor
                .trim_end()
                .rsplit_once('\n')
                .ok_or("one-line floor")?
                .0;
            write_file(Path::new(FLOOR_PATH), &format!("{lowered}\n"))?;
            // Control: a subject above the floor passes and proposes a raise.
            crate::fixture_contracts::check_fixture_contracts()?;
            assert!(proposal.is_file());
            fs::remove_file(&golden).map_err(|err| err.to_string())?;
            let refused = crate::fixture_contracts::check_fixture_contracts();
            assert!(
                refused.as_ref().is_err_and(|err| err.contains(
                    "`golden fixtures/boundary_gap/expected/human.txt` is no longer in the tree"
                )),
                "{refused:?}"
            );
            assert!(
                !proposal.exists(),
                "a stale proposal remained after a refusal"
            );
            write_file(&golden, &kept)?;
            crate::fixture_contracts::check_fixture_contracts()
        })
    }

    #[test]
    fn a_subject_the_floor_cannot_name_is_refused_not_skipped() -> Result<(), String> {
        let root = temp_root("unnameable")?;
        let fixtures = seed(&root)?;
        write_file(&fixtures.join("two words/expected/human.txt"), "human\n")?;
        let refused = observe_in(&root);
        let _ = fs::remove_dir_all(&root);
        assert!(
            refused
                .as_ref()
                .is_err_and(|err| err.contains("two words/expected/human.txt")),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_missing_empty_or_absent_tree_floor_never_passes() -> Result<(), String> {
        let observed = BTreeSet::from(["golden fixtures/a/expected/human.txt".to_string()]);
        let unreadable = evaluate(Err("No such file".to_string()), &observed);
        assert_eq!(unreadable.violations.len(), 1);
        assert!(unreadable.disclosure.intro.starts_with("NOT_ESTABLISHED"));
        let empty = evaluate(Ok(HEADER.to_string()), &observed);
        assert!(
            empty.violations[0].contains("an empty floor checks nothing"),
            "{:?}",
            empty.violations
        );
        // No fixtures directory at all: every named subject is missing.
        let root = temp_root("absent")?;
        let absent = evaluate(Ok(render_floor(&observed)), &observe_in(&root)?);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(absent.violations.len(), 1, "{:?}", absent.violations);
        Ok(())
    }

    #[test]
    fn malformed_and_repeated_floor_lines_are_refused() {
        for text in [
            "golden\n",
            "golden fixtures/a/expected\n",
            "golden fixtures/a/../b human.txt\n",
            "golden /abs human.txt\n",
            "golden fixtures/a/expected sub/human.txt\n",
            "case fixtures/a/corpus.json\n",
            "case fixtures/a/corpus.json#\n",
            "subject fixtures/a\n",
            " golden fixtures/a/expected human.txt\n",
        ] {
            assert!(parse_floor(text).is_err(), "accepted {text:?}");
        }
        let repeated = "golden fixtures/a/expected human.txt human.txt\n";
        assert!(
            parse_floor(repeated).is_err_and(|err| err.contains("listed twice")),
            "repeated line accepted"
        );
    }

    #[test]
    fn the_rendered_floor_round_trips() -> Result<(), String> {
        let entries = BTreeSet::from([
            "case fixtures/a/corpus.json#one".to_string(),
            "golden fixtures/a/expected/human.txt".to_string(),
        ]);
        assert_eq!(parse_floor(&render_floor(&entries))?, entries);
        Ok(())
    }

    /// The committed floor names only subjects present in this tree, read
    /// independently of `observe` and of the process working directory.
    #[test]
    fn the_committed_floor_names_only_present_subjects() -> Result<(), String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask has no parent directory")?;
        let floor = parse_floor(
            &fs::read_to_string(root.join(FLOOR_PATH)).map_err(|err| err.to_string())?,
        )?;
        let (mut goldens, mut cases) = (0, 0);
        for entry in &floor {
            let (kind, subject) = entry.split_once(' ').ok_or("unsplit floor line")?;
            if kind == GOLDEN {
                goldens += 1;
                assert!(root.join(subject).is_file(), "{entry} is not in the tree");
                continue;
            }
            cases += 1;
            let (path, id) = subject.split_once('#').ok_or("case line without an id")?;
            let text =
                fs::read_to_string(root.join(path)).map_err(|err| format!("{entry}: {err}"))?;
            assert!(
                corpus_case_ids(&text).iter().any(|found| found == id),
                "{entry} is not in the tree"
            );
        }
        assert!(goldens > 0 && cases > 0, "{goldens} goldens, {cases} cases");
        Ok(())
    }
}
