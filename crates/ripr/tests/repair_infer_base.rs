//! #11246: bare `ripr repair` infers its diff base the way `ripr check` does,
//! so a committed change scopes repair's refusal to the changed lines. The
//! inference only narrows: with no resolvable base, repair keeps its unscoped
//! selection and says plainly that it could not tell which lines changed; an
//! explicit `--base` wins over inference.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const ERR_BASE: &str = "#[derive(Debug, PartialEq)]\npub enum ParseError {\n    Empty,\n    TooLong,\n}\n\npub fn parse(s: &str) -> Result<usize, ParseError> {\n    if s.is_empty() {\n        return Err(ParseError::Empty);\n    }\n    if s.len() > 10 {\n        return Err(ParseError::Empty);\n    }\n    Ok(s.len())\n}\n";
const ERR_TEST: &str = "use fx::{parse, ParseError};\n\n#[test]\nfn long_input_is_too_long() {\n    assert_eq!(parse(\"abcdefghijklmnop\"), Err(ParseError::TooLong));\n}\n";
const DISCOUNT_BASE: &str = "pub fn discount(price: u32) -> u32 {\n    if price > 100 {\n        price - 10\n    } else {\n        price\n    }\n}\n";
const DISCOUNT_TEST: &str = "use wscore::discount;\n\n#[test]\nfn discount_at_boundary() {\n    assert_eq!(discount(100), 90);\n    assert_eq!(discount(50), 50);\n}\n";

fn err_new() -> String {
    ERR_BASE.replace(
        "if s.len() > 10 {\n        return Err(ParseError::Empty);",
        "if s.len() > 10 {\n        return Err(ParseError::TooLong);",
    )
}

fn discount_new() -> String {
    DISCOUNT_BASE.replace("price > 100", "price >= 100")
}

/// How the change reaches the repository.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// Committed on a `feature` branch off `main`: `ripr check` infers
    /// `main` as the base.
    FeatureBranch,
    /// Left in the working tree on top of the base commit.
    Uncommitted,
    /// Committed on `main` itself: the inferred base is `HEAD`, which differs
    /// from nothing.
    OnDefaultBranch,
    /// Committed on `trunk`, a branch name `ripr check` does not resolve as a
    /// default base, with no remote: no base resolves.
    NoResolvableBase,
}

struct Repo(PathBuf);

impl Repo {
    /// A single-package repository: `src/lib.rs` changes from `old` to `new`.
    fn package(old: &str, new: &str, test: &str, form: Form) -> Result<Self, String> {
        let repo = Self::empty()?;
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        repo.write("src/lib.rs", old)?;
        repo.write("tests/t.rs", test)?;
        repo.commit_base(form)?;
        repo.write("src/lib.rs", new)?;
        repo.land_change(form)?;
        Ok(repo)
    }

    /// A package with no Rust seam whose only change is to a Python file.
    #[cfg(feature = "lang-python")]
    fn python_only(form: Form) -> Result<Self, String> {
        let repo = Self::empty()?;
        repo.write(
            "Cargo.toml",
            "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )?;
        repo.write("src/lib.rs", "pub struct Marker;\n")?;
        repo.write("tools/x.py", "def f(x):\n    return x\n")?;
        // Zero-config detection does not enable Python for `tools/x.py`
        // (#13571); this fixture tests the enabled-language guidance.
        repo.write(
            "ripr.toml",
            "[languages]\nenabled = [\"rust\", \"python\"]\n",
        )?;
        repo.commit_base(form)?;
        repo.write("tools/x.py", "def f(x):\n    return x + 1\n")?;
        repo.land_change(form)?;
        Ok(repo)
    }

    /// A workspace: `wscore` changes, `app` depends on it, and the test lives
    /// with the changed crate.
    fn workspace(form: Form) -> Result<Self, String> {
        let repo = Self::empty()?;
        let manifest = |name: &str, extra: &str| {
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{extra}"
            )
        };
        repo.write(
            "Cargo.toml",
            "[workspace]\nmembers = [\"wscore\", \"app\"]\nresolver = \"2\"\n",
        )?;
        repo.write("wscore/Cargo.toml", &manifest("wscore", ""))?;
        repo.write(
            "app/Cargo.toml",
            &manifest(
                "app",
                "\n[dependencies]\nwscore = { path = \"../wscore\" }\n",
            ),
        )?;
        repo.write(
            "app/src/lib.rs",
            "pub fn checkout(price: u32) -> u32 {\n    wscore::discount(price)\n}\n",
        )?;
        repo.write("wscore/src/lib.rs", DISCOUNT_BASE)?;
        repo.write("wscore/tests/t.rs", DISCOUNT_TEST)?;
        repo.commit_base(form)?;
        repo.write("wscore/src/lib.rs", &discount_new())?;
        repo.land_change(form)?;
        Ok(repo)
    }

    fn empty() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-repair-infer-base-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        repo.write(".gitignore", "/target/\n")?;
        Ok(repo)
    }

    fn commit_base(&self, form: Form) -> Result<(), String> {
        let trunk = if form == Form::NoResolvableBase {
            "trunk"
        } else {
            "main"
        };
        self.git(&["-c", "init.templateDir=", "init", "-q"])?;
        self.git(&["symbolic-ref", "HEAD", &format!("refs/heads/{trunk}")])?;
        self.git(&["config", "user.email", "t@example.com"])?;
        self.git(&["config", "user.name", "t"])?;
        self.git(&["config", "commit.gpgSign", "false"])?;
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--no-gpg-sign", "-m", "base"])
    }

    fn land_change(&self, form: Form) -> Result<(), String> {
        match form {
            Form::Uncommitted => Ok(()),
            Form::FeatureBranch => {
                self.git(&["checkout", "-q", "-b", "feature"])?;
                self.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])
            }
            Form::OnDefaultBranch | Form::NoResolvableBase => {
                self.git(&["commit", "-q", "--no-gpg-sign", "-am", "change"])
            }
        }
    }

    fn write(&self, name: &str, text: &str) -> Result<(), String> {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        self.git_out(args).map(|_| ())
    }

    fn git_out(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// stdout and stderr of `ripr <args>`, with this repository's root
    /// replaced by `<root>` so two repositories compare equal.
    fn ripr(&self, args: &[&str]) -> Result<String, String> {
        let output = self.run(args)?;
        let text = format!("{}\n{}", output.stdout, output.stderr);
        // The product prints a Windows root with `/` separators, so mask both
        // spellings of this repository's own root.
        let native = self.0.to_string_lossy().into_owned();
        Ok(text
            .replace(&native, "<root>")
            .replace(&native.replace('\\', "/"), "<root>"))
    }

    /// Assert the fixture holds the intended change before any downstream
    /// claim: the one changed path, as the base `form` reads it.
    fn assert_changed_paths(&self, form: Form, expected: &str) -> Result<(), String> {
        let names = match form {
            Form::Uncommitted => self.git_out(&["diff", "--name-only", "HEAD"])?,
            Form::FeatureBranch => self.git_out(&["diff", "--name-only", "main...HEAD"])?,
            Form::OnDefaultBranch | Form::NoResolvableBase => {
                self.git_out(&["diff", "--name-only", "HEAD~1", "HEAD"])?
            }
        };
        assert_eq!(names.trim(), expected, "fixture must hold the one change");
        if form == Form::Uncommitted {
            assert_eq!(
                self.git_out(&["rev-list", "--count", "HEAD"])?.trim(),
                "1",
                "uncommitted form keeps the change out of history"
            );
        }
        Ok(())
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn is_hex_id(word: &str) -> bool {
    word.len() == 16 && word.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The seam ids a `ripr repair <id>` start line offers.
fn offered_starts(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("ripr") && words.next() == Some("repair"))
                .then(|| words.next())
                .flatten()
                .filter(|id| is_hex_id(id))
                .map(str::to_string)
        })
        .collect()
}

/// The seam id of the `ripr repair <id>` start listed right after the line
/// that names `location` in a selection list.
fn start_offered_for(text: &str, location: &str) -> Option<String> {
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.contains(location)
            && let Some(next) = lines.next()
        {
            return offered_starts(next).into_iter().next();
        }
    }
    None
}

fn check_says_nothing_to_repair(repo: &Repo, args: &[&str]) -> Result<(), String> {
    let check = repo.ripr(args)?;
    assert!(
        check.contains("Static exposure: exposed")
            && check.contains("no repair start offered: nothing to repair here"),
        "precondition, check reads the change as already exposed:\n{check}"
    );
    Ok(())
}

/// Committed and uncommitted forms of one change give one refusal.
fn committed_scopes_like_uncommitted(
    committed: &Repo,
    uncommitted: &Repo,
    changed_path: &str,
) -> Result<(), String> {
    committed.assert_changed_paths(Form::FeatureBranch, changed_path)?;
    uncommitted.assert_changed_paths(Form::Uncommitted, changed_path)?;
    // A bare `ripr check` reads each form with no flag at all.
    check_says_nothing_to_repair(committed, &["check"])?;
    check_says_nothing_to_repair(uncommitted, &["check"])?;
    let committed_repair = committed.ripr(&["repair", "--root", "."])?;
    let uncommitted_repair = uncommitted.ripr(&["repair", "--root", "."])?;
    assert_eq!(
        committed_repair, uncommitted_repair,
        "a committed change must scope repair exactly as the uncommitted one does"
    );
    assert!(
        !committed_repair.contains("could not tell which lines you changed"),
        "an inferred base must not read as unscoped:\n{committed_repair}"
    );
    assert!(
        offered_starts(&committed_repair).is_empty(),
        "nothing to repair here, so no start:\n{committed_repair}"
    );
    Ok(())
}

#[test]
fn committed_error_variant_change_scopes_repair_like_the_uncommitted_one() -> Result<(), String> {
    let new = err_new();
    let committed = Repo::package(ERR_BASE, &new, ERR_TEST, Form::FeatureBranch)?;
    let uncommitted = Repo::package(ERR_BASE, &new, ERR_TEST, Form::Uncommitted)?;
    committed_scopes_like_uncommitted(&committed, &uncommitted, "src/lib.rs")
}

#[test]
fn committed_workspace_change_scopes_repair_like_the_uncommitted_one() -> Result<(), String> {
    let committed = Repo::workspace(Form::FeatureBranch)?;
    let uncommitted = Repo::workspace(Form::Uncommitted)?;
    committed_scopes_like_uncommitted(&committed, &uncommitted, "wscore/src/lib.rs")
}

#[test]
fn a_spot_outside_the_changed_lines_is_not_offered_as_the_start() -> Result<(), String> {
    // The error-variant change is already gripped; the boundary spot on
    // `s.len() > 10` is not part of this diff.
    let on_default = Repo::package(ERR_BASE, &err_new(), ERR_TEST, Form::OnDefaultBranch)?;
    on_default.assert_changed_paths(Form::OnDefaultBranch, "src/lib.rs")?;
    let unscoped = on_default.ripr(&["repair", "--root", "."])?;
    assert!(
        unscoped.contains("ripr looked at ") && unscoped.contains("repair route not ready"),
        "control: with nothing to scope by, repair lists the repository's spots:\n{unscoped}"
    );

    let branch = Repo::package(ERR_BASE, &err_new(), ERR_TEST, Form::FeatureBranch)?;
    branch.assert_changed_paths(Form::FeatureBranch, "src/lib.rs")?;
    check_says_nothing_to_repair(&branch, &["check"])?;
    let scoped = branch.ripr(&["repair", "--root", "."])?;
    assert!(
        offered_starts(&scoped).is_empty(),
        "a spot outside the changed lines must not become a start:\n{scoped}"
    );
    assert!(
        !scoped.contains("could not tell which lines you changed"),
        "an inferred base scopes the refusal:\n{scoped}"
    );
    Ok(())
}

#[test]
fn no_resolvable_base_keeps_the_unscoped_list_and_says_so() -> Result<(), String> {
    let repo = Repo::package(ERR_BASE, &err_new(), ERR_TEST, Form::NoResolvableBase)?;
    repo.assert_changed_paths(Form::NoResolvableBase, "src/lib.rs")?;
    let check = repo.ripr(&["check"])?;
    assert!(
        check.contains("could not resolve a default base"),
        "precondition, check itself finds no base here:\n{check}"
    );
    // Behaviour is unchanged: the same repository-wide list as before, led
    // by the plain statement that nothing could be scoped.
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("ripr looked at ")
            && repair.contains("repair route not ready")
            && repair.contains("could not tell which lines you changed"),
        "no base means no narrowing:\n{repair}"
    );
    assert!(offered_starts(&repair).is_empty(), "{repair}");
    Ok(())
}

/// A repository whose every spot is gripped, so the refusal text is the whole
/// output and carries the unscoped note when there is one.
fn all_gripped(form: Form) -> Result<Repo, String> {
    Repo::package(
        "pub fn dbl(x: i32) -> i32 {\n    x * 2\n}\n",
        "pub fn dbl(x: i32) -> i32 {\n    x * 3\n}\n",
        "use fx::dbl;\n\n#[test]\nfn exact() {\n    assert_eq!(dbl(2), 6);\n}\n",
        form,
    )
}

#[test]
fn refusal_with_no_resolvable_base_says_it_could_not_tell_which_lines_changed() -> Result<(), String>
{
    let repo = all_gripped(Form::NoResolvableBase)?;
    repo.assert_changed_paths(Form::NoResolvableBase, "src/lib.rs")?;
    check_says_nothing_to_repair(&repo, &["check", "--base", "HEAD~1"])?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("could not tell which lines you changed")
            && repair.contains("no base to compare against could be found"),
        "the refusal must say plainly it could not scope:\n{repair}"
    );
    assert!(offered_starts(&repair).is_empty(), "{repair}");
    Ok(())
}

#[test]
fn refusal_on_the_default_branch_says_nothing_differs_from_the_base() -> Result<(), String> {
    let repo = all_gripped(Form::OnDefaultBranch)?;
    repo.assert_changed_paths(Form::OnDefaultBranch, "src/lib.rs")?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("could not tell which lines you changed")
            && repair.contains("nothing differs from the base"),
        "{repair}"
    );
    assert!(offered_starts(&repair).is_empty(), "{repair}");
    Ok(())
}

#[test]
fn an_explicit_base_wins_over_inference() -> Result<(), String> {
    let repo = all_gripped(Form::NoResolvableBase)?;
    let inferred = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        inferred.contains("could not tell which lines you changed"),
        "control: inference finds no base here:\n{inferred}"
    );
    let explicit = repo.ripr(&["repair", "--root", ".", "--base", "HEAD~1"])?;
    assert!(
        !explicit.contains("could not tell which lines you changed"),
        "an explicit base scopes the refusal:\n{explicit}"
    );
    assert!(
        explicit.contains("already check it"),
        "the gripped change is named:\n{explicit}"
    );
    assert!(offered_starts(&explicit).is_empty(), "{explicit}");
    // A base that names nothing is not silently ignored: the diff is
    // unreadable, so the refusal says it could not scope.
    let bogus = repo.ripr(&["repair", "--root", ".", "--base", "no-such-ref"])?;
    assert!(
        bogus.contains("could not tell which lines you changed")
            && bogus.contains("could not read a diff against `no-such-ref`")
            && !bogus.contains("no base to compare against could be found"),
        "an explicit ref that fails is named, not blamed on inference:\n{bogus}"
    );
    Ok(())
}

#[test]
fn repair_base_is_refused_with_an_item() -> Result<(), String> {
    let repo = all_gripped(Form::FeatureBranch)?;
    let text = repo.ripr(&["repair", "some-item", "--base", "main"])?;
    assert!(text.contains("drop `--base` when naming an item"), "{text}");
    Ok(())
}

const TWO_SPOTS_BASE: &str = "pub fn chk(x: i32) -> i32 {\n    if x < 0 {\n        return -1;\n    }\n    if x > 100 {\n        return 1;\n    }\n    0\n}\n";
const TWO_SPOTS_TEST: &str = "use fx::chk;\n\n#[test]\nfn t() {\n    assert_eq!(chk(5), 0);\n}\n";

#[test]
fn an_explicit_base_narrows_a_selection_to_the_changed_spot() -> Result<(), String> {
    let new = TWO_SPOTS_BASE.replace("x > 100", "x >= 100");
    let repo = Repo::package(TWO_SPOTS_BASE, &new, TWO_SPOTS_TEST, Form::NoResolvableBase)?;
    repo.assert_changed_paths(Form::NoResolvableBase, "src/lib.rs")?;
    let wide = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        wide.contains("different repairs")
            && wide.contains("src/lib.rs:2")
            && wide.contains("src/lib.rs:5"),
        "control: unscoped, both spots are offered:\n{wide}"
    );
    let changed_spot = start_offered_for(&wide, "src/lib.rs:5")
        .ok_or_else(|| format!("the unscoped list must offer the changed spot:\n{wide}"))?;
    let other_spot = start_offered_for(&wide, "src/lib.rs:2")
        .ok_or_else(|| format!("the unscoped list must offer the other spot:\n{wide}"))?;
    let narrow = repo.ripr(&["repair", "--root", ".", "--base", "HEAD~1"])?;
    assert!(
        narrow.contains(&format!("for gap `{changed_spot}`")),
        "the one start must be the changed spot's seam:\n{narrow}"
    );
    assert!(!narrow.contains(&other_spot), "{narrow}");
    assert!(
        !narrow.contains("different repairs") && !narrow.contains("src/lib.rs:2"),
        "the spot outside the changed line must not be offered or started:\n{narrow}"
    );
    Ok(())
}

#[test]
fn an_explicit_base_reads_the_dirty_working_tree_too() -> Result<(), String> {
    let repo = all_gripped(Form::NoResolvableBase)?;
    // An uncommitted edit makes check's default source the working tree, whose
    // diff is read against the explicit base.
    repo.write(
        "tests/t.rs",
        "use fx::dbl;\n\n#[test]\nfn exact() {\n    assert_eq!(dbl(2), 6);\n    assert_eq!(dbl(3), 9);\n}\n",
    )?;
    assert_eq!(
        repo.git_out(&["status", "--porcelain", "--untracked-files=no"])?
            .trim(),
        "M tests/t.rs",
        "fixture must hold the uncommitted edit"
    );
    let dirty = repo.ripr(&["repair", "--root", ".", "--base", "HEAD~1"])?;
    assert!(
        !dirty.contains("could not tell which lines you changed")
            && dirty.contains("already check it"),
        "the working-tree diff against the explicit base scopes the refusal:\n{dirty}"
    );
    let unscoped = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        unscoped.contains("could not tell which lines you changed"),
        "control: without it no base resolves:\n{unscoped}"
    );
    Ok(())
}

#[test]
#[cfg(feature = "lang-python")]
fn zero_seam_refusal_leads_with_the_unscoped_note() -> Result<(), String> {
    let repo = Repo::python_only(Form::NoResolvableBase)?;
    repo.assert_changed_paths(Form::NoResolvableBase, "tools/x.py")?;
    let repair = repo.ripr(&["repair", "--root", "."])?;
    assert!(
        repair.contains("no gaps visible under ")
            && repair.contains("could not tell which lines you changed"),
        "{repair}"
    );
    assert!(!repair.contains("seam flow is Rust only"), "{repair}");
    Ok(())
}

#[test]
#[cfg(feature = "lang-python")]
fn an_explicit_base_reaches_the_non_rust_guidance() -> Result<(), String> {
    let repo = Repo::python_only(Form::NoResolvableBase)?;
    let explicit = repo.ripr(&["repair", "--root", ".", "--base", "HEAD~1"])?;
    assert!(
        explicit.contains("is Python") && explicit.contains("seam flow is Rust only"),
        "the named base finds the Python change:\n{explicit}"
    );
    Ok(())
}

/// One `ripr` run with its streams kept apart, for the JSON documents.
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Repo {
    fn run(&self, args: &[&str]) -> Result<Run, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(Run {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn json_document(run: &Run) -> Result<serde_json::Value, String> {
    serde_json::from_str(&run.stdout).map_err(|error| {
        format!(
            "stdout must be one JSON document ({error}):\n{}\nstderr:\n{}",
            run.stdout, run.stderr
        )
    })
}

/// The `--base` rerun an unscoped start or menu names (#13701), with the
/// absolute bound root between the two halves.
const BASE_RERUN_HEAD: &str = "(for example `ripr repair --root ";
const BASE_RERUN_TAIL: &str = " --base <ref>`)";

fn names_base_rerun(text: &str) -> bool {
    text.split_once(BASE_RERUN_HEAD)
        .is_some_and(|(_, rest)| rest.lines().next().unwrap_or("").contains(BASE_RERUN_TAIL))
}
const NOT_NARROWED_MENU: &str = "so the repairs ripr offers here are not narrowed to your change";
const NOT_NARROWED_START: &str = "so the spot this repair starts on is not narrowed to your change";

/// The `rust/private-helper` repair-start case: one changed line inside a
/// private helper; the only repairable spot is the public caller's return.
const HELPER_BASE: &str =
    "fn tax(v: u32) -> u32 {\n    v / 10\n}\n\npub fn gross(v: u32) -> u32 {\n    v + tax(v)\n}\n";
const HELPER_TEST: &str =
    "use fx::gross;\n\n#[test]\nfn gross_exceeds_net() {\n    assert!(gross(100) > 100);\n}\n";

/// #13701 (a): with no readable change, the menu leads with the unscoped note
/// and the `--base` rerun, in the human menu and in the `--json` refusal
/// document's message; nothing is selected for the reader (fail closed).
#[test]
fn an_unscoped_menu_says_it_is_not_narrowed_and_names_the_base_rerun() -> Result<(), String> {
    let new = TWO_SPOTS_BASE.replace("x > 100", "x >= 100");
    let repo = Repo::package(TWO_SPOTS_BASE, &new, TWO_SPOTS_TEST, Form::NoResolvableBase)?;
    repo.assert_changed_paths(Form::NoResolvableBase, "src/lib.rs")?;

    let human = repo.run(&["repair", "--root", "."])?;
    assert_eq!(
        human.code,
        Some(3),
        "a menu is a decision:\n{}",
        human.stderr
    );
    // The menu still offers the unchanged line: that is the premise.
    assert!(
        human.stderr.contains("different repairs")
            && human.stderr.contains("src/lib.rs:2")
            && human.stderr.contains("src/lib.rs:5"),
        "control: unscoped, both spots are offered:\n{}",
        human.stderr
    );
    let note_at = human
        .stderr
        .find("could not tell which lines you changed")
        .ok_or_else(|| format!("the menu must carry the note:\n{}", human.stderr))?;
    let menu_at = human.stderr.find("different repairs").unwrap_or(0);
    assert!(
        note_at < menu_at,
        "the note leads the menu:\n{}",
        human.stderr
    );
    assert!(
        human.stderr.contains(NOT_NARROWED_MENU)
            && human
                .stderr
                .contains("no base to compare against could be found")
            && names_base_rerun(&human.stderr),
        "{}",
        human.stderr
    );
    assert_eq!(
        human
            .stderr
            .matches("could not tell which lines you changed")
            .count(),
        1,
        "the human menu says it once:\n{}",
        human.stderr
    );
    assert!(human.stdout.is_empty(), "{}", human.stdout);

    let json = repo.run(&["repair", "--root", ".", "--json"])?;
    assert_eq!(json.code, Some(3), "{}", json.stderr);
    let document = json_document(&json)?;
    assert_eq!(
        document["error"]["code"], "several_eligible_seams",
        "{document:#}"
    );
    let message = document["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.starts_with("no repair started; run one of the commands above; ")
            && message.contains(NOT_NARROWED_MENU)
            && names_base_rerun(message),
        "the JSON refusal message carries the note and the rerun:\n{document:#}"
    );
    assert_eq!(
        document["error"]["next"],
        serde_json::Value::Null,
        "the `<ref>` template is never offered as a runnable next step:\n{document:#}"
    );
    // The boundary echoes the refusal message on stderr; the note is not
    // printed a second time above the menu.
    assert_eq!(
        json.stderr
            .matches("could not tell which lines you changed")
            .count(),
        1,
        "the --json menu says it once on stderr:\n{}",
        json.stderr
    );
    assert!(
        !repo.0.join("target/ripr/repair-attempts").exists(),
        "a menu starts nothing"
    );
    Ok(())
}

/// #13701 (b): with no readable change, a bare start on the one repairable
/// spot (an unchanged line, `rust/private-helper`) says it is not narrowed and
/// names the `--base` rerun, on stderr for a human start and as the
/// `repair_start_scope` block of the `--json` start document.
#[test]
fn an_unscoped_single_start_says_it_is_not_narrowed_and_names_the_base_rerun() -> Result<(), String>
{
    let new = HELPER_BASE.replace("v / 10", "v / 5");
    let repo = Repo::package(HELPER_BASE, &new, HELPER_TEST, Form::OnDefaultBranch)?;
    repo.assert_changed_paths(Form::OnDefaultBranch, "src/lib.rs")?;

    let human = repo.run(&["repair", "--root", "."])?;
    // The fixture must start: the claim is about what a start says.
    assert!(
        human.stdout.contains("Repair prepared for seam") && human.stdout.contains("src/lib.rs:6"),
        "precondition, a bare repair starts on the unchanged caller line:\nstdout:\n{}\nstderr:\n{}",
        human.stdout,
        human.stderr
    );
    assert!(
        human
            .stderr
            .contains("could not tell which lines you changed")
            && human.stderr.contains("nothing differs from the base")
            && human.stderr.contains(NOT_NARROWED_START)
            && names_base_rerun(&human.stderr),
        "the human start must say it is not narrowed:\n{}",
        human.stderr
    );

    let json_repo = Repo::package(HELPER_BASE, &new, HELPER_TEST, Form::OnDefaultBranch)?;
    let json = json_repo.run(&["repair", "--root", ".", "--json"])?;
    assert_eq!(json.code, Some(0), "{}", json.stderr);
    let document = json_document(&json)?;
    assert!(
        document.pointer("/repair_attempt/attempt_id").is_some(),
        "precondition, the JSON start started an attempt:\n{document:#}"
    );
    let scope = &document["repair_start_scope"];
    assert_eq!(scope["narrowed_to_change"], false, "{document:#}");
    let note = scope["note"].as_str().unwrap_or_default();
    assert!(
        note.contains("could not tell which lines you changed")
            && note.contains(NOT_NARROWED_START),
        "{document:#}"
    );
    let base_command = scope["base_command"].as_str().unwrap_or_default();
    assert!(
        base_command.starts_with("ripr repair --root ") && base_command.ends_with(" --base <ref>"),
        "{document:#}"
    );
    assert_eq!(
        scope.as_object().map_or(0, serde_json::Map::len),
        3,
        "{document:#}"
    );
    // A repeated `--json` start resumes the attempt and prints its status
    // document, which carries the same block.
    let resumed = json_repo.run(&["repair", "--root", ".", "--json"])?;
    assert_eq!(resumed.code, Some(0), "{}", resumed.stderr);
    assert!(
        resumed
            .stderr
            .contains("is already awaiting the focused test edit"),
        "precondition, the second start resumes:\n{}",
        resumed.stderr
    );
    let resumed_doc = json_document(&resumed)?;
    assert!(
        resumed_doc.get("repair_attempt").is_none(),
        "precondition, the resume prints the status document:\n{resumed_doc:#}"
    );
    assert_eq!(
        resumed_doc.get("repair_start_scope"),
        document.get("repair_start_scope"),
        "the resumed start keeps the scope block:\n{resumed_doc:#}"
    );
    Ok(())
}

/// A feature branch off `main` whose only change adds a line to the test file
/// and a README: the diff is readable but touches no Rust production code.
fn test_and_readme_only(src: &str, test: &str) -> Result<Repo, String> {
    let repo = Repo::empty()?;
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    repo.write("src/lib.rs", src)?;
    repo.write("tests/t.rs", test)?;
    repo.commit_base(Form::FeatureBranch)?;
    repo.git(&["checkout", "-q", "-b", "feature"])?;
    repo.write("tests/t.rs", &format!("{test}// touched\n"))?;
    repo.write("README.md", "fx\n")?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "--no-gpg-sign", "-m", "tests and docs"])?;
    let names = repo.git_out(&["diff", "--name-only", "main...HEAD"])?;
    assert_eq!(
        names.trim(),
        "README.md\ntests/t.rs",
        "fixture must change only the test file and the README"
    );
    Ok(repo)
}

const NO_PRODUCTION_CODE: &str = "touches no Rust production code";

/// #13701 (d): a readable diff that changes only tests and docs scopes
/// nothing, so the whole-repository menu says so in its own words (not as an
/// unreadable change) and names the `--base` rerun, in human and JSON output.
#[test]
fn a_menu_for_a_change_with_no_production_code_says_it_is_not_narrowed() -> Result<(), String> {
    let repo = test_and_readme_only(TWO_SPOTS_BASE, TWO_SPOTS_TEST)?;
    let human = repo.run(&["repair", "--root", "."])?;
    assert_eq!(human.code, Some(3), "{}", human.stderr);
    assert!(
        human.stderr.contains("different repairs")
            && human.stderr.contains("src/lib.rs:2")
            && human.stderr.contains("src/lib.rs:5"),
        "precondition, nothing scopes the menu:\n{}",
        human.stderr
    );
    assert!(
        human.stderr.contains(NO_PRODUCTION_CODE)
            && human
                .stderr
                .contains("the repairs ripr offers here are not narrowed to it")
            && names_base_rerun(&human.stderr)
            && !human
                .stderr
                .contains("could not tell which lines you changed"),
        "{}",
        human.stderr
    );
    let json = repo.run(&["repair", "--root", ".", "--json"])?;
    assert_eq!(json.code, Some(3), "{}", json.stderr);
    let document = json_document(&json)?;
    let message = document["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains(NO_PRODUCTION_CODE) && names_base_rerun(message),
        "{document:#}"
    );
    Ok(())
}

/// #13701 (e): the same readable test-and-docs-only diff, with one repairable
/// spot: the start says it is not narrowed, on stderr and in the `--json`
/// document's `repair_start_scope` block.
#[test]
fn a_start_for_a_change_with_no_production_code_says_it_is_not_narrowed() -> Result<(), String> {
    let repo = test_and_readme_only(HELPER_BASE, HELPER_TEST)?;
    let human = repo.run(&["repair", "--root", "."])?;
    assert!(
        human.stdout.contains("Repair prepared for seam") && human.stdout.contains("src/lib.rs:6"),
        "precondition, a bare repair starts on the unchanged caller line:\nstdout:\n{}\nstderr:\n{}",
        human.stdout,
        human.stderr
    );
    assert!(
        human.stderr.contains(NO_PRODUCTION_CODE)
            && human
                .stderr
                .contains("the spot this repair starts on is not narrowed to it")
            && names_base_rerun(&human.stderr),
        "{}",
        human.stderr
    );
    let json_repo = test_and_readme_only(HELPER_BASE, HELPER_TEST)?;
    let json = json_repo.run(&["repair", "--root", ".", "--json"])?;
    assert_eq!(json.code, Some(0), "{}", json.stderr);
    let document = json_document(&json)?;
    assert_eq!(
        document["repair_start_scope"]["narrowed_to_change"], false,
        "{document:#}"
    );
    assert!(
        document["repair_start_scope"]["note"]
            .as_str()
            .unwrap_or_default()
            .contains(NO_PRODUCTION_CODE),
        "{document:#}"
    );
    Ok(())
}

/// #13701 (c): a change the diff reads and that touches a seam scopes the
/// start, so neither the human start nor the `--json` document carries the
/// note or the `--base` rerun. (A scoped menu is not covered here: with the
/// diff read, `ripr check` narrows these fixtures to its one recommended
/// start, so no scoped menu could be built from them.)
#[test]
fn a_start_scoped_by_the_diff_carries_no_unscoped_note() -> Result<(), String> {
    let new = TWO_SPOTS_BASE.replace("x > 100", "x >= 100");
    let repo = Repo::package(TWO_SPOTS_BASE, &new, TWO_SPOTS_TEST, Form::FeatureBranch)?;
    repo.assert_changed_paths(Form::FeatureBranch, "src/lib.rs")?;
    let human = repo.run(&["repair", "--root", "."])?;
    assert!(
        human.stdout.contains("Repair prepared for seam") && human.stdout.contains("src/lib.rs:5"),
        "precondition, the inferred base scopes a start to the changed spot:\nstdout:\n{}\nstderr:\n{}",
        human.stdout,
        human.stderr
    );
    assert!(
        !human
            .stderr
            .contains("could not tell which lines you changed")
            && !human.stderr.contains(NO_PRODUCTION_CODE)
            && !human.stderr.contains("--base <ref>"),
        "{}",
        human.stderr
    );

    let json_repo = Repo::package(TWO_SPOTS_BASE, &new, TWO_SPOTS_TEST, Form::FeatureBranch)?;
    let json = json_repo.run(&["repair", "--root", ".", "--json"])?;
    assert_eq!(json.code, Some(0), "{}", json.stderr);
    let document = json_document(&json)?;
    assert!(
        document.pointer("/repair_attempt/attempt_id").is_some(),
        "precondition, the JSON start started an attempt:\n{document:#}"
    );
    assert!(
        document.get("repair_start_scope").is_none(),
        "a scoped start carries no repair_start_scope block:\n{document:#}"
    );
    Ok(())
}
