//! #15000: the repo-scope formats (`ripr check --format repo-*`) withhold a
//! seam whose owner sits inside a `#[mutants::skip]` `impl`, `trait`, inline
//! module or `#![mutants::skip]` file, as `ripr check` (diff scope) withholds
//! its findings (RIPR-SPEC-0450) and as cargo-mutants skips it.
//!
//! Every fixture holds two identical owners: one inside the skipped scope and
//! a same-named (or same-shaped) sibling outside it. The attribute slot is
//! always exactly one line, and its baseline twin is a comment of the same
//! byte length, so lines, offsets and seam ids match across the two runs.
//! Each test drives the real `ripr` binary.

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

const REASON: &str = "owner_mutants_skip";
const SKIP: &str = "#[mutants::skip]";
/// Same byte length as [`SKIP`].
const NO_ATTR: &str = "// no attribute.";
const INNER_SKIP: &str = "#![mutants::skip]";
/// Same byte length as [`INNER_SKIP`].
const NO_INNER_ATTR: &str = "// no attribute..";

/// One fixture: the crate's source files, a test file reaching both owners
/// weakly, and where each owner's single changed line sits.
struct Fixture {
    files: Vec<(&'static str, String)>,
    tests: &'static str,
    /// `(file, line)` of the boundary inside the skipped scope.
    skipped: (&'static str, u64),
    /// `(file, line)` of the identical boundary outside it.
    sibling: (&'static str, u64),
    /// The JSON `withheld_scope` diff scope names for the skipped owner.
    scope: &'static str,
    /// The withheld owner key repo scope discloses.
    owner: &'static str,
}

fn impl_fixture(attr: &str) -> Fixture {
    Fixture {
        files: vec![(
            "src/lib.rs",
            format!(
                "pub struct Cart;
pub struct Bag;

{attr}
impl Cart {{
    pub fn limit(&self, x: u32) -> bool {{
        x > 3
    }}
}}

impl Bag {{
    pub fn limit(&self, x: u32) -> bool {{
        x > 3
    }}
}}
"
            ),
        )],
        tests: "#[test]\nfn far() {\n    assert!(fx::Cart.limit(10));\n    assert!(fx::Bag.limit(10));\n}\n",
        skipped: ("src/lib.rs", 7),
        sibling: ("src/lib.rs", 13),
        scope: "impl",
        owner: "src/lib.rs::impl Cart::limit",
    }
}

fn trait_fixture(attr: &str) -> Fixture {
    Fixture {
        files: vec![(
            "src/lib.rs",
            format!(
                "pub struct Cart;
pub struct Bag;

{attr}
pub trait Capped {{
    fn capped(&self, x: u32) -> bool {{
        x > 3
    }}
}}

pub trait Open {{
    fn capped(&self, x: u32) -> bool {{
        x > 3
    }}
}}

impl Capped for Cart {{}}
impl Open for Bag {{}}
"
            ),
        )],
        tests: "use fx::{Capped, Open};\n\n#[test]\nfn far() {\n    assert!(fx::Cart.capped(10));\n    assert!(fx::Bag.capped(10));\n}\n",
        skipped: ("src/lib.rs", 7),
        sibling: ("src/lib.rs", 13),
        scope: "trait",
        owner: "src/lib.rs::capped#L6",
    }
}

fn module_fixture(attr: &str) -> Fixture {
    Fixture {
        files: vec![(
            "src/lib.rs",
            format!(
                "{attr}
pub mod a {{
    pub mod inner {{
        pub fn bound(x: u32) -> bool {{
            x > 3
        }}
    }}
}}

pub mod b {{
    pub fn bound(x: u32) -> bool {{
        x > 3
    }}
}}
"
            ),
        )],
        tests: "#[test]\nfn far() {\n    assert!(fx::a::inner::bound(10));\n    assert!(fx::b::bound(10));\n}\n",
        skipped: ("src/lib.rs", 5),
        sibling: ("src/lib.rs", 12),
        scope: "module",
        owner: "src/lib.rs::a::inner::bound",
    }
}

/// A module file whose first line is the inner attribute; its sibling
/// module file is identical without it.
fn module_file(attr: &str) -> String {
    format!(
        "{attr}
pub fn bound(x: u32) -> bool {{
    x > 3
}}
"
    )
}

fn file_fixture(attr: &str) -> Fixture {
    Fixture {
        files: vec![
            (
                "src/lib.rs",
                "pub mod kept;\npub mod skipped;\n".to_string(),
            ),
            ("src/skipped.rs", module_file(attr)),
            ("src/kept.rs", module_file(NO_INNER_ATTR)),
        ],
        tests: "#[test]\nfn far() {\n    assert!(fx::skipped::bound(10));\n    assert!(fx::kept::bound(10));\n}\n",
        skipped: ("src/skipped.rs", 3),
        sibling: ("src/kept.rs", 3),
        scope: "file",
        owner: "src/skipped.rs::bound",
    }
}

/// `#[mutants::skip] mod skipped;` declared in `lib.rs`, with the module's
/// body in its own file: RIPR-SPEC-0450 does not follow the declaration into
/// that file, in either scope.
fn out_of_line_fixture(attr: &str) -> Fixture {
    Fixture {
        files: vec![
            (
                "src/lib.rs",
                format!("pub mod kept;\n{attr}\npub mod skipped;\n"),
            ),
            ("src/skipped.rs", module_file("//")),
            ("src/kept.rs", module_file("//")),
        ],
        tests: "#[test]\nfn far() {\n    assert!(fx::skipped::bound(10));\n    assert!(fx::kept::bound(10));\n}\n",
        skipped: ("src/skipped.rs", 3),
        sibling: ("src/kept.rs", 3),
        scope: "module",
        owner: "src/skipped.rs::bound",
    }
}

struct Repo {
    base: PathBuf,
    root: PathBuf,
}

impl Repo {
    fn new(fixture: &Fixture) -> Result<Self, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "ripr-repo-enclosing-skip-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("fx");
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )?;
        for (path, text) in &fixture.files {
            write(&root.join(path), text)?;
        }
        write(&root.join("tests/far.rs"), fixture.tests)?;
        // Both boundaries change `>=` to `>` in one diff.
        let mut diff = String::new();
        for (file, line) in [fixture.skipped, fixture.sibling] {
            let indent = boundary_indent(fixture, file, line)?;
            diff.push_str(&format!(
                "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -{line},1 +{line},1 @@\n-{indent}x >= 3\n+{indent}x > 3\n"
            ));
        }
        write(&base.join("change.diff"), &diff)?;
        Ok(Self { base, root })
    }

    fn ripr(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(args)
            .arg("--root")
            .arg(&self.root)
            .env("RIPR_CACHE_DIR", self.base.join("cache"))
            .env_remove("RIPR_QUIET")
            .output()
            .map_err(|e| format!("run ripr {args:?}: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr {args:?} failed: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    }

    fn repo(&self, format: &str) -> Result<String, String> {
        self.ripr(&["check", "--format", format])
    }

    fn repo_json(&self, format: &str) -> Result<Value, String> {
        serde_json::from_str(&self.repo(format)?)
            .map_err(|e| format!("{format} output is not JSON: {e}"))
    }

    /// The diff-scope `ripr check --format json` report over both changes.
    fn diff_report(&self) -> Result<Value, String> {
        let diff = self.base.join("change.diff").to_string_lossy().into_owned();
        let text = self.ripr(&[
            "check", "--mode", "draft", "--diff", &diff, "--format", "json",
        ])?;
        serde_json::from_str(&text).map_err(|e| format!("diff report is not JSON: {e}"))
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

/// The leading whitespace of the fixture's `x > 3` line at `file:line`,
/// asserting the fixture places the boundary there.
fn boundary_indent(fixture: &Fixture, file: &str, line: u64) -> Result<String, String> {
    let text = fixture
        .files
        .iter()
        .find(|(path, _)| *path == file)
        .map(|(_, text)| text)
        .ok_or_else(|| format!("fixture has no {file}"))?;
    let row = text
        .lines()
        .nth(line as usize - 1)
        .ok_or_else(|| format!("{file} has no line {line}"))?;
    let indent = row.trim_end_matches("x > 3");
    if indent.len() == row.len() || !indent.trim().is_empty() {
        return Err(format!("{file}:{line} must be the boundary, got {row:?}"));
    }
    Ok(indent.to_string())
}

fn seams(document: &Value) -> Result<&Vec<Value>, String> {
    document["seams"]
        .as_array()
        .ok_or_else(|| format!("document has no seams array: {document}"))
}

/// The repo seams on `file:line`, as `kind|expression` keys.
fn seams_at(document: &Value, (file, line): (&str, u64)) -> Result<Vec<String>, String> {
    let mut keys: Vec<String> = seams(document)?
        .iter()
        .filter(|seam| seam["file"] == file && seam["line"].as_u64() == Some(line))
        .map(|seam| format!("{}|{}", seam["kind"], seam["expression"]))
        .collect();
    keys.sort();
    Ok(keys)
}

/// The fixed seam set every boundary `x > 3` owner yields: the predicate
/// boundary and the returned value it computes.
fn boundary_seams() -> Vec<String> {
    vec![
        r#""predicate_boundary"|"x > 3""#.to_string(),
        r#""return_value"|"x > 3""#.to_string(),
    ]
}

/// One diff-scope finding's `(withheld_by, withheld_scope)`.
type Withholding = (Option<String>, Option<String>);

/// Diff-scope findings on `file:line`.
fn findings_at(report: &Value, (file, line): (&str, u64)) -> Result<Vec<Withholding>, String> {
    let findings = report["findings"]
        .as_array()
        .ok_or_else(|| format!("report has no findings array: {report}"))?;
    Ok(findings
        .iter()
        .filter(|finding| {
            finding["probe"]["file"]
                .as_str()
                .is_some_and(|path| path.trim_start_matches("./") == file)
                && finding["probe"]["line"].as_u64() == Some(line)
        })
        .map(|finding| {
            (
                finding["withheld_by"].as_str().map(str::to_string),
                finding["withheld_scope"].as_str().map(str::to_string),
            )
        })
        .collect())
}

/// The repo-scope check for one enclosing kind: the baseline reports both
/// owners' seams; with the attribute the skipped owner's seams are withheld
/// and disclosed in every repo format, and the sibling's are unchanged.
fn assert_repo_withholds(skipped: &Fixture, baseline: &Fixture) -> Result<(), String> {
    let base_repo = Repo::new(baseline)?;
    let base = base_repo.repo_json("repo-exposure-json")?;
    // Fixture construction: both owners yield the same, known seams.
    for at in [baseline.skipped, baseline.sibling] {
        if seams_at(&base, at)? != boundary_seams() {
            return Err(format!("baseline must report {at:?}: {base}"));
        }
    }
    if base.get("owner_opt_outs").is_some() {
        return Err(format!("baseline must withhold nothing: {base}"));
    }

    let repo = Repo::new(skipped)?;
    let withheld = boundary_seams().len();
    for format in [
        "repo-exposure-json",
        "repo-exposure-summary-json",
        "repo-seams-json",
    ] {
        let document = repo.repo_json(format)?;
        let disclosure = &document["owner_opt_outs"];
        if disclosure["withheld"] != json!(withheld)
            || disclosure["reason"] != REASON
            || disclosure["owners"] != json!([skipped.owner])
        {
            return Err(format!(
                "{format}: must disclose {withheld} seam of {} withheld: {disclosure}",
                skipped.owner
            ));
        }
        if format != "repo-exposure-summary-json" {
            if !seams_at(&document, skipped.skipped)?.is_empty() {
                return Err(format!(
                    "{format}: the skipped owner's seams must be withheld: {document}"
                ));
            }
            if seams_at(&document, skipped.sibling)? != boundary_seams() {
                return Err(format!(
                    "{format}: the sibling must keep its seam: {document}"
                ));
            }
        }
    }
    let line = format!("Withheld by owner opt-out ({REASON}): {withheld} seam");
    for format in ["repo-seams-md", "repo-exposure-md"] {
        let md = repo.repo(format)?;
        if !md.contains(&line) {
            return Err(format!("{format} must disclose `{line}`:\n{md}"));
        }
    }
    Ok(())
}

/// The agreement control: diff scope withholds the skipped owner's finding
/// with `scope` and reports the sibling's, and repo scope withholds exactly
/// the skipped owner's seam.
fn assert_scopes_agree(fixture: &Fixture, withholds: bool) -> Result<(), String> {
    let repo = Repo::new(fixture)?;
    let report = repo.diff_report()?;
    let skipped = findings_at(&report, fixture.skipped)?;
    let sibling = findings_at(&report, fixture.sibling)?;
    if skipped.is_empty() || sibling.is_empty() {
        return Err(format!("diff scope must find both changed lines: {report}"));
    }
    let want_skipped = if withholds {
        (Some(REASON.to_string()), Some(fixture.scope.to_string()))
    } else {
        (None, None)
    };
    if skipped.iter().any(|row| *row != want_skipped) {
        return Err(format!(
            "diff scope on {:?} must be {want_skipped:?}: {skipped:?}",
            fixture.skipped
        ));
    }
    if sibling.iter().any(|row| *row != (None, None)) {
        return Err(format!("diff scope must report the sibling: {sibling:?}"));
    }

    let exposure = repo.repo_json("repo-exposure-json")?;
    let repo_withheld = seams_at(&exposure, fixture.skipped)?.is_empty();
    if repo_withheld != withholds
        || seams_at(&exposure, fixture.sibling)? != boundary_seams()
        || exposure.get("owner_opt_outs").is_some() != withholds
    {
        return Err(format!(
            "repo scope must agree with diff scope (withheld: {withholds}) on {:?}: {exposure}",
            fixture.skipped
        ));
    }
    Ok(())
}

#[test]
fn repo_scope_withholds_a_seam_inside_a_skipped_impl() -> Result<(), String> {
    assert_repo_withholds(&impl_fixture(SKIP), &impl_fixture(NO_ATTR))
}

#[test]
fn repo_scope_withholds_a_seam_inside_a_skipped_trait() -> Result<(), String> {
    assert_repo_withholds(&trait_fixture(SKIP), &trait_fixture(NO_ATTR))
}

#[test]
fn repo_scope_withholds_a_seam_inside_a_skipped_inline_module() -> Result<(), String> {
    assert_repo_withholds(&module_fixture(SKIP), &module_fixture(NO_ATTR))
}

#[test]
fn repo_scope_withholds_a_seam_in_a_file_level_inner_skip() -> Result<(), String> {
    assert_repo_withholds(&file_fixture(INNER_SKIP), &file_fixture(NO_INNER_ATTR))
}

#[test]
fn diff_and_repo_scope_agree_on_every_enclosing_kind() -> Result<(), String> {
    for fixture in [
        impl_fixture(SKIP),
        trait_fixture(SKIP),
        module_fixture(SKIP),
        file_fixture(INNER_SKIP),
    ] {
        assert_scopes_agree(&fixture, true)?;
    }
    Ok(())
}

/// Control: without the attribute neither scope withholds anything.
#[test]
fn diff_and_repo_scope_agree_without_the_attribute() -> Result<(), String> {
    for fixture in [
        impl_fixture(NO_ATTR),
        trait_fixture(NO_ATTR),
        module_fixture(NO_ATTR),
        file_fixture(NO_INNER_ATTR),
    ] {
        assert_scopes_agree(&fixture, false)?;
    }
    Ok(())
}

/// RIPR-SPEC-0450 bound: `#[mutants::skip] mod name;` is not followed into
/// the module's own file, so both scopes keep reporting its owner alike.
#[test]
fn out_of_line_module_declaration_is_not_followed_by_either_scope() -> Result<(), String> {
    assert_scopes_agree(&out_of_line_fixture(SKIP), false)
}
