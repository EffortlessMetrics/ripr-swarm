//! Acceptance fixtures for #13411: an integration-test helper that builds its
//! own receiver (`let re = Regex::new(..); re.is_match_with(..)`) and calls the
//! unique inherent method the change touched.
//!
//! Today such a helper forms no method hop (`method_hop` admits only a typed
//! parameter receiver), so the test that calls the helper carries no
//! `helper_owner_call` relation: it is linked by name only
//! (`weak_token_substring`, no call path). The positive case pins that current
//! limit under a name that says so, and the series flips it to
//! `helper_owner_call` once the local-receiver hop lands. The negative cases
//! must never carry `helper_owner_call` and never read `exposed`, before and
//! after.
//!
//! Each case writes a small crate with `src/lib.rs` and `tests/suite.rs`,
//! changes the owner's body line, and reads the finding through
//! `check_workspace`.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-local-receiver-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path.join("tests")).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "[package]\nname = \"local_receiver\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n";

const NEW_BODY: &str = "        !hay.is_empty() && hay.contains(self.pat.as_str())";
const OLD_BODY: &str = "        hay.contains(self.pat.as_str())";

/// The library: `Regex::is_match_with` is the unique inherent method whose body
/// line changes; one unrelated in-file unit test is the only file-level link.
fn library(extra: &str) -> String {
    format!(
        "pub struct Regex {{ pub pat: String }}\npub struct Cache;\nimpl Regex {{\n    pub fn new(p: &str) -> Regex {{ Regex {{ pat: p.to_string() }} }}\n    pub fn is_match_with(&self, _c: &mut Cache, hay: &str) -> bool {{\n{NEW_BODY}\n    }}\n}}\n{extra}\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn unrelated_unit() {{\n        let v = vec![1, 2, 3];\n        assert!(!v.is_empty());\n    }}\n}}\n"
    )
}

/// One-hunk unified diff replacing `OLD_BODY` with `NEW_BODY` on the line that
/// carries the owner's body.
fn diff() -> String {
    let source = library("");
    let line = source
        .lines()
        .position(|line| line == NEW_BODY)
        .map_or(0, |index| index + 1);
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{OLD_BODY}\n+{NEW_BODY}\n"
    )
}

fn finding(library_source: &str, suite: &str) -> Result<Value, String> {
    let scratch = Scratch::create()?;
    let write = |relative: &str, text: &str| {
        std::fs::write(scratch.0.join(relative), text).map_err(|error| error.to_string())
    };
    write("Cargo.toml", MANIFEST)?;
    write("src/lib.rs", library_source)?;
    write("tests/suite.rs", suite)?;
    write("change.diff", &diff())?;
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
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    findings
        .iter()
        .find(|finding| {
            finding["probe"]["family"] == "return_value"
                && finding["probe"]["expression"]
                    .as_str()
                    .is_some_and(|expression| expression.contains("hay.is_empty()"))
        })
        .cloned()
        .ok_or_else(|| format!("no return_value finding for the changed body: {json}"))
}

fn relation_of<'a>(finding: &'a Value, test: &str) -> Option<&'a str> {
    finding["related_tests"]
        .as_array()?
        .iter()
        .find(|related| related["name"] == test)
        .and_then(|related| related["relation_reason"].as_str())
}

/// The calling test may still be linked weakly (`weak_token_substring`: "linked
/// by name only", no call path); what it must not carry is the helper-hop
/// relation, which is what credits a call path.
fn assert_not_hop_credited(finding: &Value, test: &str) {
    assert_ne!(
        relation_of(finding, test),
        Some("helper_owner_call"),
        "{finding}"
    );
}

fn assert_never_exposed(finding: &Value) {
    assert_ne!(finding["classification"], "exposed", "{finding}");
}

/// The shared helper shape: builds the receiver locally, one call site.
const LOCAL_RECEIVER_SUITE: &str = "use local_receiver::{Cache, Regex};\nfn run_case(pat: &str, hay: &str, want: bool) {\n    let re = Regex::new(pat);\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_helper() {\n    run_case(\"ab\", \"xaby\", true);\n}\n";

/// Control: the same call written in the test body is a direct owner call, so
/// the harness above is the only variable.
#[test]
fn control_direct_call_in_the_test_body_is_related() -> Result<(), String> {
    let suite = "use local_receiver::{Cache, Regex};\n#[test]\nfn direct() {\n    let re = Regex::new(\"ab\");\n    let mut cache = Cache;\n    assert_eq!(true, re.is_match_with(&mut cache, \"xaby\"));\n}\n";
    let f = finding(&library(""), suite)?;
    assert_eq!(relation_of(&f, "direct"), Some("direct_owner_call"), "{f}");
    Ok(())
}

/// Pinned current limit (#13411): a typed-parameter helper written in an
/// integration-test file (`tests/suite.rs`) is not `helper_owner_call` either.
/// `extend_chain` keeps an evidence-role hop caller only for `#[cfg(test)]`
/// modules, so the locally built receiver is not the only missing piece: the
/// series must decide the integration-test helper question deliberately for
/// both receiver kinds. (The #9329 typed-parameter hop is not exercised by
/// this method-call shape; its own fixtures live with that work.)
#[test]
fn typed_parameter_helper_in_an_integration_test_file_is_not_yet_hop_credited() -> Result<(), String>
{
    let suite = "use local_receiver::{Cache, Regex};\nfn run_case(re: &Regex, hay: &str, want: bool) {\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_param_helper() {\n    run_case(&Regex::new(\"ab\"), \"xaby\", true);\n}\n";
    let f = finding(&library(""), suite)?;
    assert_not_hop_credited(&f, "via_param_helper");
    assert_never_exposed(&f);
    Ok(())
}

/// Pinned current limit (#13411): the helper builds its own receiver, so no
/// hop forms and the calling test is not related. The local-receiver hop PR
/// flips this to `helper_owner_call`; it must never become `exposed` by reach alone.
#[test]
fn helper_building_its_own_receiver_is_not_yet_hop_credited() -> Result<(), String> {
    let f = finding(&library(""), LOCAL_RECEIVER_SUITE)?;
    assert_not_hop_credited(&f, "via_helper");
    assert_never_exposed(&f);
    Ok(())
}

/// A receiver rebound to a different value before the call is not the type the
/// constructor named.
#[test]
fn rebound_local_receiver_stays_unrelated() -> Result<(), String> {
    let suite = "use local_receiver::{Cache, Regex};\nfn make_other() -> Other { Other }\nstruct Other;\nimpl Other { fn is_match_with(&self, _c: &mut Cache, _h: &str) -> bool { true } }\nfn run_case(pat: &str, hay: &str, want: bool) {\n    let re = Regex::new(pat);\n    let re = make_other();\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_rebound() {\n    run_case(\"ab\", \"xaby\", true);\n}\n";
    let f = finding(&library(""), suite)?;
    assert_not_hop_credited(&f, "via_rebound");
    assert_never_exposed(&f);
    Ok(())
}

/// A constructor of another type whose argument merely mentions `Regex` does
/// not type the receiver as `Regex`.
#[test]
fn constructor_of_another_type_mentioning_the_owner_type_stays_unrelated() -> Result<(), String> {
    let suite = "use local_receiver::{Cache, Regex};\nstruct Wrapper(Regex);\nimpl Wrapper {\n    fn wrap(inner: Regex) -> Wrapper { Wrapper(inner) }\n    fn is_match_with(&self, c: &mut Cache, hay: &str) -> bool { self.0.is_match_with(c, hay) }\n}\nfn run_case(pat: &str, hay: &str, want: bool) {\n    let re = Wrapper::wrap(Regex::new(pat));\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_wrapper() {\n    run_case(\"ab\", \"xaby\", true);\n}\n";
    let f = finding(&library(""), suite)?;
    assert_not_hop_credited(&f, "via_wrapper");
    assert_never_exposed(&f);
    Ok(())
}

/// More than one call of the method in the helper is not one binding site.
#[test]
fn helper_with_two_call_sites_stays_unrelated() -> Result<(), String> {
    let suite = "use local_receiver::{Cache, Regex};\nfn run_case(pat: &str, hay: &str, want: bool) {\n    let re = Regex::new(pat);\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_two_sites() {\n    run_case(\"ab\", \"xaby\", true);\n}\n";
    let f = finding(&library(""), suite)?;
    assert_not_hop_credited(&f, "via_two_sites");
    assert_never_exposed(&f);
    Ok(())
}

/// An imported same-named type may not be the impl's type.
#[test]
fn imported_type_name_in_the_helper_file_stays_unrelated() -> Result<(), String> {
    let suite = "use local_receiver::Cache;\nuse other_crate::Regex;\nfn run_case(pat: &str, hay: &str, want: bool) {\n    let re = Regex::new(pat);\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_imported() {\n    run_case(\"ab\", \"xaby\", true);\n}\n";
    let f = finding(&library(""), suite)?;
    assert_not_hop_credited(&f, "via_imported");
    assert_never_exposed(&f);
    Ok(())
}

/// A `&mut self` method is refused: it may change a field before the owner.
#[test]
fn mut_self_owner_stays_unrelated() -> Result<(), String> {
    let source = library("").replace("&self, _c", "&mut self, _c");
    let suite = "use local_receiver::{Cache, Regex};\nfn run_case(pat: &str, hay: &str, want: bool) {\n    let mut re = Regex::new(pat);\n    let mut cache = Cache;\n    assert_eq!(want, re.is_match_with(&mut cache, hay));\n}\n#[test]\nfn via_mut_self() {\n    run_case(\"ab\", \"xaby\", true);\n}\n";
    let f = finding(&source, suite)?;
    assert_not_hop_credited(&f, "via_mut_self");
    assert_never_exposed(&f);
    Ok(())
}
