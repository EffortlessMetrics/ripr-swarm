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
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(text.replace(&self.0.to_string_lossy().into_owned(), "<root>"))
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
        unscoped.contains("7 visible seams") && unscoped.contains("repair route not ready"),
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
        repair.contains("7 visible seams")
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
        explicit.contains("already grip it (exposed)") || explicit.contains("already grip"),
        "the gripped change is named:\n{explicit}"
    );
    assert!(offered_starts(&explicit).is_empty(), "{explicit}");
    // A base that names nothing is not silently ignored: the diff is
    // unreadable, so the refusal says it could not scope.
    let bogus = repo.ripr(&["repair", "--root", ".", "--base", "no-such-ref"])?;
    assert!(
        bogus.contains("could not tell which lines you changed"),
        "{bogus}"
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
