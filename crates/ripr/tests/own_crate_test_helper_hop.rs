//! A test that reaches changed code through its own file's helper is related
//! when the helper spells the call through the package's own library name
//! (`fn bulk(q: u32) -> bool { hop_reach::tiers::is_bulk(q) }`, #8387). The
//! same helper with a `use` and a bare call was already related; the
//! qualified spelling read as a name-token neighbour, so a test the diff adds
//! through such a helper never led the related-test list.
//!
//! Each case writes a small package, changes the owner's boundary line, and
//! reads the predicate finding's related tests through `check_workspace`.
//! Every refusal differs from the linking positive by one stated thing, so it
//! cannot pass for an unrelated reason.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        // The workspace redirects the temp directory into `target/`, which a
        // fresh worktree may not hold yet.
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let path = root.join(format!(
            "ripr-own-crate-helper-hop-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // Exclusive creation: a colliding directory is never reused or deleted.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let target = self.0.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(target, content).map_err(|error| error.to_string())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname = \"hop_reach\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const LIB: &str = "pub mod tiers;\n\npub fn order_discount(qty: u32) -> u32 {\n    if tiers::is_bulk(qty) { 5 } else { 0 }\n}\n";

const TIERS: &str = "pub fn is_bulk(qty: u32) -> bool {\n    10 <= qty\n}\n";

/// The diff changing the owner's `qty >= 10` body line in `src/tiers.rs`.
const DIFF: &str = "diff --git a/src/tiers.rs b/src/tiers.rs\n--- a/src/tiers.rs\n+++ b/src/tiers.rs\n@@ -1,3 +1,3 @@\n pub fn is_bulk(qty: u32) -> bool {\n-    qty >= 10\n+    10 <= qty\n }\n";

const TEST: &str =
    "#[test]\nfn bulk_boundary() {\n    assert!(bulk(10));\n    assert!(!bulk(9));\n}\n";

/// `tests/api.rs`: `prelude` lines, a helper `bulk` with `body`, and the test
/// that calls only the helper.
fn helper_file(prelude: &str, body: &str) -> String {
    format!("{prelude}fn bulk(qty: u32) -> bool {{\n    {body}\n}}\n\n{TEST}")
}

const QUALIFIED: &str = "hop_reach::tiers::is_bulk(qty)";

struct Outcome {
    /// The relation reason of `bulk_boundary` on the predicate finding, or
    /// `None` when it is not a related test of it.
    relation: Option<String>,
    classification: String,
    /// The first related test of the finding, as rendered.
    leading: Option<String>,
}

fn outcome_of(manifest: &str, files: &[(&str, &str)]) -> Result<Outcome, String> {
    let scratch = Scratch::create()?;
    scratch.write("Cargo.toml", manifest)?;
    for (path, content) in files {
        scratch.write(path, content)?;
    }
    scratch.write("change.diff", DIFF)?;
    let report = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let finding = json["findings"]
        .as_array()
        .ok_or("missing findings")?
        .iter()
        .find(|finding| finding["probe"]["family"] == "predicate")
        .cloned()
        .ok_or_else(|| format!("no predicate finding: {json}"))?;
    // The intended subject: the changed line of the owner, not a neighbour.
    let file = finding["probe"]["file"].as_str().unwrap_or_default();
    let expression = finding["probe"]["expression"].as_str().unwrap_or_default();
    if !file.ends_with("src/tiers.rs") || expression != "10 <= qty" {
        return Err(format!(
            "predicate finding is `{expression}` in `{file}`, not the owner's boundary"
        ));
    }
    let related = finding["related_tests"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(Outcome {
        relation: related
            .iter()
            .find(|test| test["name"] == "bulk_boundary")
            .and_then(|test| test["relation_reason"].as_str().map(str::to_string)),
        classification: finding["classification"]
            .as_str()
            .ok_or("missing classification")?
            .to_string(),
        leading: related
            .first()
            .and_then(|test| test["name"].as_str().map(str::to_string)),
    })
}

fn outcome(tests: &str) -> Result<Outcome, String> {
    outcome_of(
        MANIFEST,
        &[
            ("src/lib.rs", LIB),
            ("src/tiers.rs", TIERS),
            ("tests/api.rs", tests),
        ],
    )
}

const LINKED: Option<&str> = Some("helper_owner_call");

/// Not a call relation: absent, or a name-token neighbour.
fn unlinked(relation: &Option<String>) -> bool {
    matches!(relation.as_deref(), None | Some("weak_token_substring"))
}

#[test]
fn imported_bare_helper_call_is_the_linked_baseline() -> Result<(), String> {
    let tests = helper_file("use hop_reach::tiers::is_bulk;\n\n", "is_bulk(qty)");
    assert_eq!(outcome(&tests)?.relation.as_deref(), LINKED);
    Ok(())
}

/// Fails without the `own_crate_test_helper_hop` route: the test is then not
/// a related test of the finding at all.
#[test]
fn helper_call_spelled_through_the_own_library_links() -> Result<(), String> {
    let found = outcome(&helper_file("", QUALIFIED))?;
    assert_eq!(found.relation.as_deref(), LINKED);
    assert_eq!(found.leading.as_deref(), Some("bulk_boundary"));
    Ok(())
}

/// The hop is reach only: it forwards no result, so the finding cannot become
/// `exposed` through it however exact the test's assertions are.
#[test]
fn the_own_library_hop_never_makes_the_finding_exposed() -> Result<(), String> {
    let tests = "fn bulk(qty: u32) -> bool {\n    hop_reach::tiers::is_bulk(qty)\n}\n\n#[test]\nfn bulk_boundary() {\n    assert_eq!(bulk(10), true);\n    assert_eq!(bulk(9), false);\n}\n";
    let found = outcome(tests)?;
    assert_eq!(found.relation.as_deref(), LINKED);
    assert_ne!(found.classification, "exposed");
    Ok(())
}

/// The regex `t()` shape: the helper enters a public wrapper through the
/// library name, and the wrapper reaches the changed owner by a
/// module-qualified call.
#[test]
fn helper_entering_a_public_wrapper_links() -> Result<(), String> {
    let tests = "fn discount(qty: u32) -> u32 {\n    hop_reach::order_discount(qty)\n}\n\n#[test]\nfn bulk_boundary() {\n    assert_eq!(discount(10), 5);\n    assert_eq!(discount(9), 0);\n}\n";
    let found = outcome(tests)?;
    assert_eq!(found.relation.as_deref(), LINKED);
    assert_ne!(found.classification, "exposed");
    Ok(())
}

/// A test the helper relates leads a name-token neighbour that only shares a
/// word with the owner.
#[test]
fn the_helper_related_test_leads_a_name_token_neighbour() -> Result<(), String> {
    let found = outcome_of(
        MANIFEST,
        &[
            ("src/lib.rs", LIB),
            ("src/tiers.rs", TIERS),
            ("tests/api.rs", &helper_file("", QUALIFIED)),
            (
                "tests/bulk_names.rs",
                "#[test]\nfn is_bulk_word_only() {\n    assert_eq!(\"bulk\".len(), 4);\n}\n",
            ),
        ],
    )?;
    assert_eq!(found.relation.as_deref(), LINKED);
    assert_eq!(found.leading.as_deref(), Some("bulk_boundary"));
    Ok(())
}

/// An ordinary import from the library does not rebind its name. The import
/// is unused here: a second test entering `order_discount` would give the
/// owner two reached callers, which is the separate multi-caller stop.
#[test]
fn an_unrelated_import_from_the_library_still_links() -> Result<(), String> {
    let tests = helper_file("use hop_reach::order_discount;\n\n", QUALIFIED);
    assert_eq!(outcome(&tests)?.relation.as_deref(), LINKED);
    Ok(())
}

#[test]
fn another_crate_name_does_not_link() -> Result<(), String> {
    let found = outcome(&helper_file("", "other_crate::tiers::is_bulk(qty)"))?;
    assert!(unlinked(&found.relation), "{:?}", found.relation);
    Ok(())
}

#[test]
fn a_path_that_is_not_the_owners_module_path_does_not_link() -> Result<(), String> {
    for body in [
        "hop_reach::is_bulk(qty)",
        "hop_reach::tiers::extra::is_bulk(qty)",
        "::hop_reach::tiers::is_bulk(qty)",
        "outer::hop_reach::tiers::is_bulk(qty)",
    ] {
        let found = outcome(&helper_file("", body))?;
        assert!(unlinked(&found.relation), "{body}: {:?}", found.relation);
    }
    Ok(())
}

/// Each prelude line, added to the otherwise linking file, lets the crate
/// name mean something other than the library.
#[test]
fn a_rebindable_crate_name_does_not_link() -> Result<(), String> {
    for prelude in [
        "mod hop_reach {\n    pub mod tiers {\n        pub fn is_bulk(_: u32) -> bool {\n            true\n        }\n    }\n}\n\n",
        "use other_dep as hop_reach;\n\n",
        "use other_dep::hop_reach;\n\n",
        "use other_dep::*;\n\n",
        "extern crate other_dep as hop_reach;\n\n",
    ] {
        let found = outcome(&helper_file(prelude, QUALIFIED))?;
        assert!(unlinked(&found.relation), "{prelude}: {:?}", found.relation);
    }
    Ok(())
}

#[test]
fn a_helper_that_mentions_the_owner_twice_does_not_link() -> Result<(), String> {
    let body = "hop_reach::tiers::is_bulk(qty) && hop_reach::tiers::is_bulk(qty + 1)";
    let found = outcome(&helper_file("", body))?;
    assert!(unlinked(&found.relation), "{:?}", found.relation);
    Ok(())
}

/// An unexpanded macro in the helper may return early or rebind a name.
#[test]
fn a_helper_that_invokes_an_opaque_macro_does_not_link() -> Result<(), String> {
    let prelude = "macro_rules! guard {\n    ($value:expr) => {\n        if $value > 100 {\n            return false;\n        }\n    };\n}\n\n";
    let tests = format!(
        "{prelude}fn bulk(qty: u32) -> bool {{\n    guard!(qty);\n    {QUALIFIED}\n}}\n\n{TEST}"
    );
    let found = outcome(&tests)?;
    assert!(unlinked(&found.relation), "{:?}", found.relation);
    Ok(())
}

/// With `[lib] name` set, the package name is not a spelling of the library.
#[test]
fn the_package_name_is_not_the_library_name_when_lib_name_differs() -> Result<(), String> {
    let manifest = format!("{MANIFEST}[lib]\nname = \"hop_lib\"\n");
    let files = |tests: &'static str| {
        [
            ("src/lib.rs", LIB),
            ("src/tiers.rs", TIERS),
            ("tests/api.rs", tests),
        ]
    };
    let by_package = "fn bulk(qty: u32) -> bool {\n    hop_reach::tiers::is_bulk(qty)\n}\n\n#[test]\nfn bulk_boundary() {\n    assert!(bulk(10));\n    assert!(!bulk(9));\n}\n";
    let by_lib = "fn bulk(qty: u32) -> bool {\n    hop_lib::tiers::is_bulk(qty)\n}\n\n#[test]\nfn bulk_boundary() {\n    assert!(bulk(10));\n    assert!(!bulk(9));\n}\n";
    let refused = outcome_of(&manifest, &files(by_package))?;
    assert!(unlinked(&refused.relation), "{:?}", refused.relation);
    assert_eq!(
        outcome_of(&manifest, &files(by_lib))?.relation.as_deref(),
        LINKED
    );
    Ok(())
}
