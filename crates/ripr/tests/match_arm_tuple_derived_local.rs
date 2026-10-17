//! Public-API witnesses for RIPR #1728 derived-local tuple-arm identity.
//!
//! Model the active UB Review `terminalize_proof` shape without rewriting the
//! consumer: two immutable booleans are derived inside a `filter_map` closure,
//! the `(false, false)` sibling returns `None`, and the selected relation flows
//! into the returned terminal projection. Unsupported mappings must remain
//! explicitly unverified.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_REPO: AtomicU64 = AtomicU64::new(0);

#[test]
fn real_terminal_crate_namespace_preserves_exact_result_flow() -> Result<(), String> {
    let start = TERMINAL_SOURCE
        .find("pub fn terminalize_proof(")
        .ok_or("retained owner declaration")?;
    let root = format!(
        "{}\nmod work_queue;\npub use work_queue::terminalize_proof;\n",
        &TERMINAL_SOURCE[..start]
    );
    let owner = format!("use crate::*;\n{}", &TERMINAL_SOURCE[start..]);
    for counterfeit in ["none", "format", "Some"] {
        let repo = TempRepo::create(TERMINAL_SOURCE, &terminal_test(true, false))?;
        let root = if counterfeit == "format" {
            format!(
                r#"macro_rules! format {{
                ($($tokens:tt)*) => {{ "receipt-1=head_passed join=request_identity_v2".to_string() }};
            }}
            {root}"#
            )
        } else if counterfeit == "Some" {
            format!(
                r#"fn Some<T>((receipt, _relation): (T, &'static str)) -> Option<(T, &'static str)> {{
                std::option::Option::Some((receipt, "request_identity_v2"))
            }}
            {root}"#
            )
        } else {
            root.clone()
        };
        std::fs::write(repo.workspace.join("src/lib.rs"), root)
            .map_err(|error| error.to_string())?;
        std::fs::write(repo.workspace.join("src/work_queue.rs"), &owner)
            .map_err(|error| error.to_string())?;
        let diff = candidate_diff(&owner)?.replace("src/lib.rs", "src/work_queue.rs");
        std::fs::write(repo.workspace.join("diff.patch"), diff)
            .map_err(|error| error.to_string())?;
        let output = repo.check()?;
        let finding = changed_request_only_arm(&output)?;
        if counterfeit != "none" {
            assert_unverified(finding, counterfeit);
        } else {
            assert_eq!(
                finding.class,
                ExposureClass::Exposed,
                "actual crate wildcard must retain the exact tuple projection: {finding:#?}"
            );
        }
    }
    Ok(())
}

#[test]
fn generated_inherited_constructor_cannot_certify_terminal_arm() -> Result<(), String> {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/match_arm_tuple_terminal_inherited_constructor/input");
    let root =
        std::fs::read_to_string(fixture.join("src/lib.rs")).map_err(|error| error.to_string())?;
    let (_, declarations) = root.split_once('\n').ok_or("constructor declaration")?;
    let root = format!(
        r#"macro_rules! generate_constructor {{
            () => {{
                fn Some<T>((receipt, _relation): (T, &'static str)) -> Option<(T, &'static str)> {{
                    std::option::Option::Some((receipt, "request_identity_v2"))
                }}
            }};
        }}
        generate_constructor!();
        {declarations}"#
    );
    let owner = std::fs::read_to_string(fixture.join("src/work_queue.rs"))
        .map_err(|error| error.to_string())?;
    let test = std::fs::read_to_string(fixture.join("tests/projection.rs"))
        .map_err(|error| error.to_string())?;
    let repo = TempRepo::create(TERMINAL_SOURCE, &test)?;
    std::fs::write(repo.workspace.join("src/lib.rs"), &root).map_err(|error| error.to_string())?;
    std::fs::write(repo.workspace.join("src/work_queue.rs"), &owner)
        .map_err(|error| error.to_string())?;
    std::fs::write(
        repo.workspace.join("diff.patch"),
        candidate_diff(&owner)?.replace("src/lib.rs", "src/work_queue.rs"),
    )
    .map_err(|error| error.to_string())?;
    let output = repo.check()?;
    assert_unverified(
        changed_request_only_arm(&output)?,
        "macro-generated inherited Some",
    );
    Ok(())
}

#[test]
fn real_terminal_imported_formatter_remains_unverified() -> Result<(), String> {
    for import in [
        "#[macro_use] extern crate counterfeit_format;",
        "use counterfeit_format::*;",
    ] {
        let source = format!("{import}\n{TERMINAL_SOURCE}");
        let repo = TempRepo::create(&source, &terminal_test(true, false))?;
        let dependency = repo.workspace.join("formatter");
        std::fs::create_dir_all(dependency.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            dependency.join("Cargo.toml"),
            "[package]\nname = \"counterfeit-format\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            dependency.join("src/lib.rs"),
            r#"#[macro_export] macro_rules! format {
            ($($tokens:tt)*) => { "receipt-1=head_passed join=request_identity_v2".to_string() };
        }"#,
        )
        .map_err(|error| error.to_string())?;
        let manifest = repo.workspace.join("Cargo.toml");
        let mut cargo = std::fs::read_to_string(&manifest).map_err(|error| error.to_string())?;
        cargo.push_str("\n[dependencies]\ncounterfeit-format = { path = \"formatter\" }\n");
        std::fs::write(manifest, cargo).map_err(|error| error.to_string())?;
        let output = repo.check()?;
        assert_unverified(changed_request_only_arm(&output)?, "imported formatter");
    }
    Ok(())
}

#[test]
fn real_terminal_replaced_formatter_remains_unverified() -> Result<(), String> {
    // This formatter ignores the relation. The complete native request-only
    // oracle still passes, so body spelling alone cannot certify its arm.
    let source = format!(
        r#"macro_rules! format {{
            ($($tokens:tt)*) => {{ "receipt-1=head_passed join=request_identity_v2".to_string() }};
        }}
        {TERMINAL_SOURCE}"#
    );
    let repo = TempRepo::create(&source, &terminal_test(true, false))?;
    let output = repo.check()?;
    assert_unverified(changed_request_only_arm(&output)?, "replaced formatter");
    Ok(())
}

#[test]
fn real_terminal_projection_is_current_and_deterministic_in_ready_and_deep() -> Result<(), String> {
    let repo = TempRepo::create(TERMINAL_SOURCE, &terminal_test(true, false))?;
    let ready = repo.check()?;
    let repeated = repo.check()?;
    let deep = check_workspace(CheckInput {
        root: repo.workspace.clone(),
        base: None,
        diff_file: Some(repo.workspace.join("diff.patch")),
        mode: Mode::Deep,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let ready = changed_request_only_arm(&ready)?;
    for (label, output) in [("repeated ready", repeated), ("deep", deep)] {
        let current = changed_request_only_arm(&output)?;
        assert_eq!(
            current.class,
            ExposureClass::Exposed,
            "{label}: {current:#?}"
        );
        assert_eq!(
            current.probe.id, ready.probe.id,
            "{label}: stable finding identity"
        );
        assert_eq!(
            serde_json::to_value(current).map_err(|error| error.to_string())?,
            serde_json::to_value(ready).map_err(|error| error.to_string())?,
            "{label}: complete finding stays deterministic"
        );
    }
    Ok(())
}

#[test]
fn real_terminal_owner_flow_controls_remain_unverified() -> Result<(), String> {
    for (label, from, to) in [
        (
            "swapped owner parameters",
            "task_id: &str,\n    plan_status: &str",
            "plan_status: &str,\n    task_id: &str",
        ),
        (
            "wrong proof key",
            "proof_request_ids.get(task_id)",
            "proof_request_ids.get(plan_status)",
        ),
        (
            "shadowed request set",
            "let mut matching = receipts",
            "let request_set = BTreeSet::<&str>::new();\n    let mut matching = receipts",
        ),
        (
            "swapped tuple",
            "match (request_identity_matches, task_identity_matches)",
            "match (task_identity_matches, request_identity_matches)",
        ),
        (
            "mutated matching",
            "matching.sort_by",
            "matching.clear();\n    matching.sort_by",
        ),
        (
            "wrong receipt identity in reason",
            "receipt.id, receipt.result",
            "task_id, receipt.result",
        ),
        (
            "diagnostic literal",
            "join={relation}",
            "diagnostic={relation}",
        ),
        ("string whitespace", " join={relation}", "join={relation}"),
        (
            "swapped return fields",
            "Ok((status, reason, request_ids, receipt_ids))",
            "Ok((status, reason, receipt_ids, request_ids))",
        ),
        (
            "wrong task binding",
            "receipt.id == task_id",
            "receipt.id == plan_status",
        ),
    ] {
        assert!(
            TERMINAL_SOURCE.contains(from),
            "{label}: setup must change the specimen"
        );
        let source = TERMINAL_SOURCE.replace(from, to);
        let repo = TempRepo::create(&source, &terminal_test(true, false))?;
        let output = repo.check()?;
        assert_unverified(changed_request_only_arm(&output)?, label);
    }
    Ok(())
}

#[test]
fn real_terminal_oracle_controls_remain_unverified() -> Result<(), String> {
    let honest = terminal_test(true, false);
    for (label, from, to) in [
        (
            "conditional observation",
            "    assert_eq!(terminal.0",
            "    #[cfg(any())]\n    assert_eq!(terminal.0",
        ),
        (
            "uninvoked oracle",
            "    assert_eq!(terminal.0",
            "    let _never_called = || {\n    assert_eq!(terminal.0",
        ),
        (
            "direct return",
            "    assert_eq!(terminal.0",
            "    return;\n    assert_eq!(terminal.0",
        ),
        (
            "no-op reason",
            "assert_eq!(terminal.1, \"receipt-1=head_passed join=request_identity_v2\", \"request_identity_v2\")",
            "assert_eq!(terminal.1, terminal.1, \"request_identity_v2\")",
        ),
        ("mutable input", "let receipts =", "let mut receipts ="),
        ("mutated result", "let terminal =", "let mut terminal ="),
        (
            "wrong fed binding",
            "&proof_request_ids, &receipt_request_ids",
            "&receipt_request_ids, &proof_request_ids",
        ),
        (
            "diagnostic only",
            "terminal.1, \"receipt-1=head_passed join=request_identity_v2\"",
            "terminal.1, \"request_identity_v2\"",
        ),
        (
            "duplicate proof key",
            "vec![\"request-1\".to_string(), \"request-1\".to_string()])]",
            "vec![\"request-1\".to_string(), \"request-1\".to_string()]), (\"different-task\".to_string(), vec![\"unrelated-request\".to_string()])]",
        ),
        (
            "shadowed result",
            "    assert_eq!(terminal.0",
            "    let terminal = (String::new(), String::new(), Vec::<String>::new(), Vec::<String>::new());\n    assert_eq!(terminal.0",
        ),
    ] {
        assert!(
            honest.contains(from),
            "{label}: setup must change the oracle"
        );
        let mut test = honest.replace(from, to);
        if label == "uninvoked oracle" {
            let close = test.rfind('}').ok_or("test has closing brace")?;
            test.insert_str(close, "};\n");
        }
        let repo = TempRepo::create(TERMINAL_SOURCE, &test)?;
        let output = repo.check()?;
        assert_unverified(changed_request_only_arm(&output)?, label);
    }
    Ok(())
}

const TERMINAL_SOURCE: &str = r#"use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, String>;
#[derive(Debug, PartialEq, Eq)]
pub struct ProofReceipt { pub id: String, pub result: String }
pub fn terminalize_proof(
    task_id: &str,
    plan_status: &str,
    receipts: &[ProofReceipt],
    proof_request_ids: &BTreeMap<String, Vec<String>>,
    receipt_request_ids: &BTreeMap<String, Vec<String>>,
) -> Result<(String, String, Vec<String>, Vec<String>)> {
    let mut request_ids = proof_request_ids.get(task_id).cloned().unwrap_or_default();
    request_ids.sort();
    request_ids.dedup();
    let request_set = request_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut matching = receipts
        .iter()
        .filter_map(|receipt| {
            let request_identity_matches =
                receipt_request_ids
                    .get(&receipt.id)
                    .is_some_and(|receipt_requests| {
                        receipt_requests
                            .iter()
                            .any(|request_id| request_set.contains(request_id.as_str()))
                    });
            let task_identity_matches = receipt.id == task_id;
            let relation = match (request_identity_matches, task_identity_matches) {
                (true, true) => "request_and_task_identity",
                (true, false) => "request_identity_v2",
                (false, true) => "task_identity",
                (false, false) => return None,
            };
            Some((receipt, relation))
        })
        .collect::<Vec<_>>();
    matching.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));
    let receipt_ids = matching
        .iter()
        .map(|(receipt, _)| receipt.id.clone())
        .collect::<Vec<_>>();
    if matching.is_empty() {
        let (status, reason) = if plan_status == "planned" {
            (
                "not_executed".to_owned(),
                "planned proof task has no terminal receipt joined by request or task identity"
                    .to_owned(),
            )
        } else {
            (
                plan_status.to_owned(),
                "proof task retained its terminal planner disposition".to_owned(),
            )
        };
        return Ok((status, reason, request_ids, receipt_ids));
    }
    let mut results = matching
        .iter()
        .map(|(receipt, _)| terminal_proof_receipt_status(receipt))
        .collect::<Result<Vec<_>>>()?;
    results.sort();
    results.dedup();
    let status = if results.len() == 1 {
        results[0].clone()
    } else {
        "multiple_terminal_receipts".to_owned()
    };
    let reason = matching
        .iter()
        .map(|(receipt, relation)| format!("{}={} join={relation}", receipt.id, receipt.result))
        .collect::<Vec<_>>()
        .join("; ");
    Ok((status, reason, request_ids, receipt_ids))
}

fn terminal_proof_receipt_status(receipt: &ProofReceipt) -> Result<String> {
    if receipt.result.trim().is_empty() || !matches!(receipt.result.as_str(),
        "head_passed" | "head_failed" | "discriminating" | "non_discriminating"
        | "base_patch_failed" | "timed_out" | "skipped_budget" | "skipped_profile") {
        return Err("unsupported terminal result".to_string());
    }
    Ok(receipt.result.clone())
}
"#;

/// Faithful #1306 owner body from terminal.rs blob
/// 1ae5baef607bdf2d9c53fa6749956f6db38fd9c0. Only visibility and the changed
/// request-only literal differ; the Result alias/receipt validator keep this
/// native standalone specimen independent of UB's package dependencies.
fn terminal_test(request_match: bool, task_match: bool) -> String {
    let task = if task_match {
        "receipt-1"
    } else {
        "different-task"
    };
    let request = if request_match {
        "request-1"
    } else {
        "unrelated-request"
    };
    let (status, reason, receipt_ids) = match (request_match, task_match) {
        (true, false) => (
            "head_passed",
            "receipt-1=head_passed join=request_identity_v2",
            r#"vec!["receipt-1".to_string()]"#,
        ),
        (false, true) => (
            "head_passed",
            "receipt-1=head_passed join=task_identity",
            r#"vec!["receipt-1".to_string()]"#,
        ),
        (true, true) => (
            "head_passed",
            "receipt-1=head_passed join=request_and_task_identity",
            r#"vec!["receipt-1".to_string()]"#,
        ),
        (false, false) => (
            "not_executed",
            "planned proof task has no terminal receipt joined by request or task identity",
            "Vec::<String>::new()",
        ),
    };
    format!(
        r#"use match_arm_tuple_derived_local::{{ProofReceipt, terminalize_proof}};
use std::collections::BTreeMap;
#[test]
fn terminal_projection() {{
    let receipts = vec![ProofReceipt {{ id: "receipt-1".to_string(), result: "head_passed".to_string() }}];
    let proof_request_ids = BTreeMap::from([("{task}".to_string(), vec!["request-1".to_string(), "request-1".to_string()])]);
    let receipt_request_ids = BTreeMap::from([("receipt-1".to_string(), vec!["{request}".to_string()])]);
    let terminal = terminalize_proof("{task}", "planned", &receipts, &proof_request_ids, &receipt_request_ids).unwrap();
    assert_eq!(terminal.0, "{status}");
    assert_eq!(terminal.1, "{reason}", "request_identity_v2");
    assert_eq!(terminal.2, vec!["request-1".to_string()]);
    assert_eq!(terminal.3, {receipt_ids});
}}
"#
    )
}

#[test]
fn real_terminal_request_derivation_and_reason_projection() -> Result<(), String> {
    let repo = TempRepo::create(TERMINAL_SOURCE, &terminal_test(true, false))?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "real consumer must observe the current request-only arm: {finding:#?}"
    );
    Ok(())
}

#[test]
fn real_terminal_sibling_projections_remain_unverified() -> Result<(), String> {
    for (request, task) in [(false, true), (true, true), (false, false)] {
        let repo = TempRepo::create(TERMINAL_SOURCE, &terminal_test(request, task))?;
        let output = repo.check()?;
        assert_unverified(changed_request_only_arm(&output)?, "real terminal sibling");
    }
    Ok(())
}

const CANDIDATE_SOURCE: &str = r#"use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
}

pub fn terminalize_proof<'a>(
    receipts: &'a [Receipt],
    receipt_request_ids: &BTreeMap<String, Vec<String>>,
    request_set: &BTreeSet<String>,
    task_id: &str,
) -> Vec<(&'a Receipt, &'static str)> {
    receipts
        .iter()
        .filter_map(|receipt| {
            let request_identity_matches = receipt_request_ids
                .get(&receipt.id)
                .is_some_and(|receipt_requests| {
                    receipt_requests
                        .iter()
                        .any(|request_id| request_set.contains(request_id.as_str()))
                });
            let task_identity_matches = receipt.id == task_id;
            let relation = match (request_identity_matches, task_identity_matches) {
                (true, true) => "request_and_task_identity",
                (true, false) => "request_identity_v2",
                (false, true) => "task_identity",
                (false, false) => return None,
            };
            Some((receipt, relation))
        })
        .collect()
}
"#;

/// Build the candidate diff from the actual candidate source so the hunk
/// coordinates and context lines always describe the real on-disk file.
/// `check` trusts hunk coordinates: a stale hand-written hunk silently
/// re-coordinates the changed arm onto whatever parser shape covers the
/// claimed line (here the whole method chain is one tail-expression
/// return-value shape), which manufactures a false negative instead of the
/// intended candidate-current match-arm witness.
fn candidate_diff(source: &str) -> Result<String, String> {
    let lines: Vec<&str> = source.lines().collect();
    let position = lines
        .iter()
        .position(|line| line.trim() == "(true, false) => \"request_identity_v2\",")
        .ok_or("candidate source keeps the changed request-only arm")?;
    if position < 3 || position + 3 >= lines.len() {
        return Err("three context lines stay inside the candidate source".to_string());
    }
    let context_start = position + 1 - 3;
    let mut diff = String::from(
        "diff --git a/src/lib.rs b/src/lib.rs\n\
         index 1111111..2222222 100644\n\
         --- a/src/lib.rs\n\
         +++ b/src/lib.rs\n",
    );
    diff.push_str(&format!(
        "@@ -{context_start},7 +{context_start},7 @@ pub fn terminalize_proof<'a>(\n"
    ));
    for line in &lines[position - 3..position] {
        diff.push_str(&format!(" {line}\n"));
    }
    diff.push_str(&format!(
        "-{}\n",
        lines[position].replace("\"request_identity_v2\"", "\"request_identity_v1\"")
    ));
    diff.push_str(&format!("+{}\n", lines[position]));
    for line in &lines[position + 1..position + 4] {
        diff.push_str(&format!(" {line}\n"));
    }
    Ok(diff)
}

const REQUEST_ONLY_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn request_only_projection_observes_join() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let mut receipt_request_ids = BTreeMap::new();
    receipt_request_ids.insert(
        "receipt-1".to_string(),
        vec!["request-1".to_string()],
    );
    let request_set = BTreeSet::from(["request-1".to_string()]);

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "different-task",
    );

    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].0.id, "receipt-1");
    assert_eq!(terminal[0].1, "request_identity_v2");
}
"#;

const TASK_ONLY_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn task_only_projection_observes_sibling() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let receipt_request_ids = BTreeMap::new();
    let request_set = BTreeSet::new();

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "receipt-1",
    );

    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].0.id, "receipt-1");
    assert_eq!(terminal[0].1, "task_identity", "request_identity_v2");
}
"#;

const BOTH_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn both_identity_projection_observes_sibling() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let mut receipt_request_ids = BTreeMap::new();
    receipt_request_ids.insert(
        "receipt-1".to_string(),
        vec!["request-1".to_string()],
    );
    let request_set = BTreeSet::from(["request-1".to_string()]);

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "receipt-1",
    );

    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].0.id, "receipt-1");
    assert_eq!(
        terminal[0].1,
        "request_and_task_identity",
        "request_identity_v2"
    );
}
"#;

const NEITHER_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn neither_identity_is_excluded() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let receipt_request_ids = BTreeMap::new();
    let request_set = BTreeSet::new();

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "different-task",
    );

    assert!(terminal.is_empty(), "request_identity_v2");
}
"#;

const UNINVOKED_ORACLE_TEST: &str = r#"use match_arm_tuple_derived_local::{Receipt, terminalize_proof};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn uninvoked_projection_assertions_are_not_observation() {
    let receipts = vec![Receipt { id: "receipt-1".to_string() }];
    let mut receipt_request_ids = BTreeMap::new();
    receipt_request_ids.insert(
        "receipt-1".to_string(),
        vec!["request-1".to_string()],
    );
    let request_set = BTreeSet::from(["request-1".to_string()]);

    let terminal = terminalize_proof(
        &receipts,
        &receipt_request_ids,
        &request_set,
        "different-task",
    );

    let _never_called = || {
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].0.id, "receipt-1");
        assert_eq!(terminal[0].1, "request_identity_v2");
    };
}
"#;

struct TempRepo {
    /// Owned temporary directory; removed on drop.
    root: PathBuf,
    /// Analyzed workspace: `root` itself, or one level below it for
    /// relative-root witnesses (see `create_nested`).
    workspace: PathBuf,
}

impl TempRepo {
    fn create(source: &str, test_source: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("clock before Unix epoch: {error}"))?
            .as_nanos();
        Self::create_with_stamp(source, test_source, stamp)
    }

    /// Place the workspace one directory below the temporary root. The
    /// temporary root and the test process's current directory sit at the same
    /// depth below the checkout, so a relative spelling of the root itself
    /// would resolve identically from either base and could not tell a
    /// cwd-relative probe path from a root-relative one.
    fn create_nested(source: &str, test_source: &str) -> Result<Self, String> {
        let mut repo = Self::create(source, test_source)?;
        let workspace = repo.root.join("nested");
        std::fs::create_dir(&workspace)
            .map_err(|error| format!("create nested workspace failed: {error}"))?;
        for entry in ["Cargo.toml", "src", "tests", "diff.patch"] {
            std::fs::rename(repo.root.join(entry), workspace.join(entry))
                .map_err(|error| format!("move {entry} into nested workspace failed: {error}"))?;
        }
        repo.workspace = workspace;
        Ok(repo)
    }

    fn create_with_stamp(source: &str, test_source: &str, stamp: u128) -> Result<Self, String> {
        let repo = loop {
            // Clock resolution is not a uniqueness guarantee between test threads.
            let sequence = NEXT_TEMP_REPO.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "ripr-match-arm-derived-local-{}-{stamp}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&root) {
                Ok(()) => {
                    break Self {
                        workspace: root.clone(),
                        root,
                    };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("create fixture root failed: {error}")),
            }
        };
        // Only the successful creator owns cleanup, including partial setup failures.
        std::fs::create_dir(repo.root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir(repo.root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            repo.root.join("Cargo.toml"),
            "[package]\nname = \"match-arm-tuple-derived-local\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(repo.root.join("src/lib.rs"), source)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(repo.root.join("tests/terminal.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(repo.root.join("diff.patch"), candidate_diff(source)?)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(repo)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        self.check_at(self.workspace.clone())
    }

    /// Check through a relative root with a directory prefix, which is how the
    /// fixture runner and `ripr check --root sub/dir` spell it. Probe
    /// locations then carry that prefix, unlike an absolute or `.` root.
    fn check_relative(&self) -> Result<CheckOutput, String> {
        let root = relative_from_current_dir(&self.workspace)?;
        if root.components().count() < 2 {
            return Err(format!(
                "relative root {} must carry a directory prefix",
                root.display()
            ));
        }
        self.check_at(root)
    }

    fn check_at(&self, root: PathBuf) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root,
            base: None,
            diff_file: Some(self.workspace.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn relative_from_current_dir(target: &std::path::Path) -> Result<PathBuf, String> {
    use std::path::Component;
    let current = std::env::current_dir()
        .map_err(|error| format!("current directory unavailable: {error}"))?;
    let current = current.components().collect::<Vec<_>>();
    let target_components = target.components().collect::<Vec<_>>();
    let shared = current
        .iter()
        .zip(&target_components)
        .take_while(|(left, right)| left == right)
        .count();
    if shared == 0
        || current[..shared]
            .iter()
            .all(|component| matches!(component, Component::Prefix(_)))
    {
        return Err(format!(
            "{} shares no root with the current directory",
            target.display()
        ));
    }
    let mut relative = PathBuf::new();
    for _ in shared..current.len() {
        relative.push("..");
    }
    for component in &target_components[shared..] {
        relative.push(component.as_os_str());
    }
    if relative.is_absolute() || relative.as_os_str().is_empty() {
        return Err(format!("no relative spelling for {}", target.display()));
    }
    Ok(relative)
}

fn normalized(text: &str) -> String {
    text.trim()
        .trim_end_matches(',')
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn changed_request_only_arm(output: &CheckOutput) -> Result<&ripr::Finding, String> {
    let expected_full = normalized("(true, false) => \"request_identity_v2\",");
    let expected_boundary = normalized("(true, false) =>");
    let expected_before = normalized("(true, false) => \"request_identity_v1\",");
    let is_current = |text: &str| {
        let text = normalized(text);
        text == expected_full || text == expected_boundary
    };
    let matches = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ProbeFamily::MatchArm
                && is_current(&finding.probe.expression)
                && finding.probe.after.as_deref().is_some_and(&is_current)
                && finding
                    .probe
                    .before
                    .as_deref()
                    .is_none_or(|before| normalized(before) == expected_before)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [finding] => Ok(*finding),
        _ => {
            let observed = output
                .findings
                .iter()
                .filter(|finding| finding.probe.family == ProbeFamily::MatchArm)
                .map(|finding| {
                    format!(
                        "id={:?}; file={}; line={}; before={:?}; after={:?}; expression={:?}; class={:?}",
                        finding.probe.id,
                        finding.probe.location.file.display(),
                        finding.probe.location.line,
                        finding.probe.before,
                        finding.probe.after,
                        finding.probe.expression,
                        finding.class
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            Err(format!(
                "expected exactly one candidate-current request-only arm, found {}; all match subjects: {observed}",
                matches.len()
            ))
        }
    }
}

fn assert_unverified(finding: &ripr::Finding, context: &str) {
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{context}: {:?}",
        finding.ripr.reveal.discriminate
    );
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("observation_unverified"),
        "{context}: {:?}",
        finding.ripr.reveal.discriminate
    );
}

#[test]
fn request_only_derived_locals_observe_the_changed_relation() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, REQUEST_ONLY_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the exact request-only receipt route must certify the current arm; probe={:#?}; stages={:#?}; related={:#?}",
        finding.probe,
        finding.ripr,
        finding.related_tests
    );
    // Related oracles are per-assertion; the owner tie is the relation reason,
    // and the discriminating literals pin the projection's element identity.
    let mut related = finding
        .related_tests
        .iter()
        .filter(|test| test.name == "request_only_projection_observes_join");
    assert!(
        related.clone().any(|test| {
            test.relation_reason.is_some()
                && test
                    .oracle
                    .as_deref()
                    .is_some_and(|oracle| oracle.contains("request_identity_v2"))
        }),
        "the changed relation literal must be observed; related={:#?}",
        finding.related_tests
    );
    assert!(
        related.any(|test| test
            .oracle
            .as_deref()
            .is_some_and(|oracle| oracle.contains("receipt-1"))),
        "the projection element identity must be observed; related={:#?}",
        finding.related_tests
    );
    Ok(())
}

#[test]
fn task_only_sibling_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, TASK_ONLY_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "task-only sibling");
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "task_only_projection_observes_sibling"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("task_identity"))
    }));
    Ok(())
}

#[test]
fn both_identity_sibling_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, BOTH_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "both-identity sibling");
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "both_identity_projection_observes_sibling"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("request_and_task_identity"))
    }));
    Ok(())
}

#[test]
fn neither_identity_exclusion_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, NEITHER_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "return-None sibling");
    assert!(finding.related_tests.iter().any(|test| {
        test.name == "neither_identity_is_excluded"
            && test
                .oracle
                .as_deref()
                .is_some_and(|oracle| oracle.contains("is_empty"))
    }));
    Ok(())
}

#[test]
fn unrelated_task_parameter_cannot_certify_the_request_arm() -> Result<(), String> {
    let signature = "    task_id: &str,\n) -> Vec<(&'a Receipt, &'static str)> {";
    let widened_signature =
        "    task_id: &str,\n    unrelated_id: &str,\n) -> Vec<(&'a Receipt, &'static str)> {";
    let initializer = "let task_identity_matches = receipt.id == task_id;";
    assert_eq!(CANDIDATE_SOURCE.matches(signature).count(), 1);
    assert_eq!(CANDIDATE_SOURCE.matches(initializer).count(), 1);
    let source = CANDIDATE_SOURCE
        .replace(signature, widened_signature)
        .replace(
            initializer,
            "let task_identity_matches = receipt.id == unrelated_id;",
        );
    let call = "        \"different-task\",\n    );";
    assert_eq!(REQUEST_ONLY_TEST.matches(call).count(), 1);
    let test_source = REQUEST_ONLY_TEST.replace(
        call,
        "        \"receipt-1\",\n        \"different-task\",\n    );",
    );
    let repo = TempRepo::create(&source, &test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "unrelated task parameter");
    Ok(())
}

#[test]
fn unrelated_request_set_cannot_certify_the_request_arm() -> Result<(), String> {
    let signature = "    request_set: &BTreeSet<String>,\n    task_id: &str,";
    let widened_signature = "    request_set: &BTreeSet<String>,\n    unrelated_request_set: &BTreeSet<String>,\n    task_id: &str,";
    let membership = "request_set.contains(request_id.as_str())";
    assert_eq!(CANDIDATE_SOURCE.matches(signature).count(), 1);
    assert_eq!(CANDIDATE_SOURCE.matches(membership).count(), 1);
    let source = CANDIDATE_SOURCE
        .replace(signature, widened_signature)
        .replace(
            membership,
            "unrelated_request_set.contains(request_id.as_str())",
        );
    let set = "    let request_set = BTreeSet::from([\"request-1\".to_string()]);";
    assert_eq!(REQUEST_ONLY_TEST.matches(set).count(), 1);
    let test_source = REQUEST_ONLY_TEST
        .replace(
            set,
            "    let request_set = BTreeSet::new();\n    let unrelated_request_set = BTreeSet::from([\"request-1\".to_string()]);",
        )
        .replace(
            "        &request_set,\n        \"different-task\",",
            "        &request_set,\n        &unrelated_request_set,\n        \"different-task\",",
        );
    let repo = TempRepo::create(&source, &test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "unrelated request set");
    Ok(())
}

#[test]
fn wrong_receipt_identity_cannot_certify_the_request_arm() -> Result<(), String> {
    // The oracle still claims the original receipt identity while the test
    // feeds a different receipt, so the admission must not borrow it as
    // proof that the projection observes the actually fed receipt.
    let fed_receipt = "    let receipts = vec![Receipt { id: \"receipt-1\".to_string() }];";
    assert_eq!(REQUEST_ONLY_TEST.matches(fed_receipt).count(), 1);
    let identity_assertion = "assert_eq!(terminal[0].0.id, \"receipt-1\");";
    assert_eq!(REQUEST_ONLY_TEST.matches(identity_assertion).count(), 1);
    let test_source = REQUEST_ONLY_TEST.replace(
        fed_receipt,
        "    let receipts = vec![Receipt { id: \"input-receipt\".to_string() }];",
    );
    let repo = TempRepo::create(CANDIDATE_SOURCE, &test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "wrong receipt identity");
    Ok(())
}

#[test]
fn unrelated_receipts_cannot_certify_the_request_arm() -> Result<(), String> {
    let parameter = "    receipts: &'a [Receipt],";
    let receiver = "\n    receipts\n";
    assert_eq!(CANDIDATE_SOURCE.matches(parameter).count(), 1);
    assert_eq!(CANDIDATE_SOURCE.matches(receiver).count(), 1);
    let source = CANDIDATE_SOURCE
        .replace(
            parameter,
            "    receipts: &'a [Receipt], unrelated_receipts: &'a [Receipt],",
        )
        .replace(receiver, "\n    unrelated_receipts\n");
    let input = "    let receipts = vec![Receipt { id: \"receipt-1\".to_string() }];";
    let argument = "        &receipts,\n";
    assert_eq!(REQUEST_ONLY_TEST.matches(input).count(), 1);
    assert_eq!(REQUEST_ONLY_TEST.matches(argument).count(), 1);
    // Both collections carry the asserted receipt identity, so the fed
    // identity and its assertion agree and the unrelated iterator receiver
    // (`canonical_receipt_iteration`) is the only rejection reason.
    let test_source = REQUEST_ONLY_TEST
        .replace(
            input,
            "    let receipts = vec![Receipt { id: \"receipt-1\".to_string() }];\n    let unrelated_receipts = vec![Receipt { id: \"receipt-1\".to_string() }];",
        )
        .replace(argument, "        &receipts,\n        &unrelated_receipts,\n");
    let repo = TempRepo::create(&source, &test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "unrelated receipt collection");
    Ok(())
}

#[test]
fn shadowed_terminal_results_cannot_certify_the_request_arm() -> Result<(), String> {
    let assertion = "    assert_eq!(terminal.len(), 1);";
    assert_eq!(REQUEST_ONLY_TEST.matches(assertion).count(), 1);
    for shadow in [
        "    let terminal = vec![(&receipts[0], \"request_identity_v2\")];",
        "    let (terminal,) = (vec![(&receipts[0], \"request_identity_v2\")],);",
    ] {
        let test_source = REQUEST_ONLY_TEST.replace(assertion, &format!("{shadow}\n{assertion}"));
        let repo = TempRepo::create(CANDIDATE_SOURCE, &test_source)?;
        let output = repo.check()?;
        let finding = changed_request_only_arm(&output)?;
        assert_unverified(finding, "shadowed terminal result");
    }
    Ok(())
}

#[test]
fn uninvoked_projection_assertions_cannot_certify_the_request_arm() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, UNINVOKED_ORACLE_TEST)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, "uninvoked projection assertions");
    Ok(())
}

fn check_unsupported_mapping(
    scrutinee: &str,
    test_source: &str,
    context: &str,
) -> Result<(), String> {
    let original = "match (request_identity_matches, task_identity_matches)";
    assert_eq!(CANDIDATE_SOURCE.matches(original).count(), 1);
    let replacement = format!("match {scrutinee}");
    let source = CANDIDATE_SOURCE.replace(original, &replacement);
    let repo = TempRepo::create(&source, test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;

    assert_unverified(finding, context);
    Ok(())
}

#[test]
fn reordered_local_tuple_does_not_borrow_owner_call_inputs() -> Result<(), String> {
    check_unsupported_mapping(
        "(task_identity_matches, request_identity_matches)",
        TASK_ONLY_TEST,
        "reordered local tuple",
    )
}

#[test]
fn transformed_local_tuple_does_not_borrow_derived_values() -> Result<(), String> {
    check_unsupported_mapping(
        "(!request_identity_matches, task_identity_matches)",
        NEITHER_TEST,
        "transformed local tuple",
    )
}

#[test]
fn same_timestamp_fixtures_keep_independent_contents_and_cleanup() -> Result<(), String> {
    let first = TempRepo::create_with_stamp(CANDIDATE_SOURCE, REQUEST_ONLY_TEST, 0)?;
    let second = TempRepo::create_with_stamp(CANDIDATE_SOURCE, TASK_ONLY_TEST, 0)?;
    assert_ne!(first.root, second.root);
    assert_eq!(
        std::fs::read_to_string(first.root.join("tests/terminal.rs"))
            .map_err(|error| format!("read first fixture test failed: {error}"))?,
        REQUEST_ONLY_TEST
    );
    let first_root = first.root.clone();
    drop(first);
    assert!(
        !first_root.exists(),
        "the first owner must clean its own root"
    );
    let diff = candidate_diff(CANDIDATE_SOURCE)?;
    for (relative, expected) in [
        ("src/lib.rs", CANDIDATE_SOURCE),
        ("tests/terminal.rs", TASK_ONLY_TEST),
        ("diff.patch", diff.as_str()),
    ] {
        assert_eq!(
            std::fs::read_to_string(second.root.join(relative))
                .map_err(|error| format!("read surviving fixture {relative} failed: {error}"))?,
            expected,
            "dropping the first owner must not remove or overwrite the second fixture"
        );
    }
    Ok(())
}

#[test]
fn fixture_paths_remain_inside_the_ephemeral_root() -> Result<(), String> {
    let repo = TempRepo::create(CANDIDATE_SOURCE, REQUEST_ONLY_TEST)?;
    for relative in [
        "Cargo.toml",
        "src/lib.rs",
        "tests/terminal.rs",
        "diff.patch",
    ] {
        let path = repo.root.join(relative);
        if !path.starts_with(&repo.root) || !path.is_file() {
            return Err(format!(
                "fixture path escaped or is missing: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

#[test]
fn projection_return_boundary_unreachable_assertions_cannot_certify_the_arm() -> Result<(), String>
{
    for assertion in [
        "    assert_eq!(terminal.len(), 1);",
        "    assert_eq!(terminal[0].0.id, \"receipt-1\");",
        "    assert_eq!(terminal[0].1, \"request_identity_v2\");",
    ] {
        assert_eq!(REQUEST_ONLY_TEST.matches(assertion).count(), 1);
        let test_source =
            REQUEST_ONLY_TEST.replace(assertion, &format!("    return;\n{assertion}"));
        let repo = TempRepo::create(CANDIDATE_SOURCE, &test_source)?;
        let output = repo.check()?;
        let finding = changed_request_only_arm(&output)?;
        assert_unverified(finding, assertion);
    }
    Ok(())
}

#[test]
fn projection_return_boundary_after_complete_observation_preserves_exposure() -> Result<(), String>
{
    let assertion = "    assert_eq!(terminal[0].1, \"request_identity_v2\");";
    assert_eq!(REQUEST_ONLY_TEST.matches(assertion).count(), 1);
    let test_source = REQUEST_ONLY_TEST.replace(assertion, &format!("{assertion}\n    return;"));
    let repo = TempRepo::create(CANDIDATE_SOURCE, &test_source)?;
    let output = repo.check()?;
    let finding = changed_request_only_arm(&output)?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "all required projection assertions precede the return: {:#?}",
        finding.ripr
    );
    Ok(())
}

#[test]
fn relative_prefixed_root_keeps_the_derived_arm_admission() -> Result<(), String> {
    let repo = TempRepo::create_nested(CANDIDATE_SOURCE, REQUEST_ONLY_TEST)?;
    let output = repo.check_relative()?;
    let finding = changed_request_only_arm(&output)?;
    assert!(
        finding.probe.location.file.is_relative(),
        "probe location must carry the relative root spelling: {:?}",
        finding.probe.location.file
    );
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a relative --root with a directory prefix must admit the same arm as an absolute root: {:#?}",
        finding.ripr
    );
    Ok(())
}

#[test]
fn relative_prefixed_root_keeps_the_return_boundary() -> Result<(), String> {
    let assertion = "    assert_eq!(terminal[0].1, \"request_identity_v2\");";
    assert_eq!(REQUEST_ONLY_TEST.matches(assertion).count(), 1);
    let test_source = REQUEST_ONLY_TEST.replace(assertion, &format!("    return;\n{assertion}"));
    let repo = TempRepo::create_nested(CANDIDATE_SOURCE, &test_source)?;
    let output = repo.check_relative()?;
    let finding = changed_request_only_arm(&output)?;
    assert_unverified(finding, "relative root, return before relation assertion");
    Ok(())
}
