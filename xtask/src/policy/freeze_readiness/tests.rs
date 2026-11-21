//! Discriminating proof for `freeze-readiness` (#1609).
//!
//! Every test evaluates a small real Git repository. One fixture satisfies all
//! eight predicates; each control then changes one fact and asserts that
//! exactly the predicate that owns it leaves `pass`. The self-confirmation
//! controls put a contradicting packet and a forged receipt in the working
//! tree and assert the committed objects still decide the result.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use super::{
    ADMISSION_PACKET_PATH, DENY_CHECK, DEPENDENCY_REVIEW_CHECK, Predicate, Receipt, Request,
    SECURITY_SUBJECT_MARKERS, SECURITY_WORKFLOW_PATH, State, WATCH_CHECK, evaluate,
    pr_number_in_subject, verdict,
};
use crate::policy::release_selection::RELEASE_SELECTION_PATH;

const TODAY: &str = "2026-11-21";
const SENTENCE: &str = "The release makes no claim about this beyond the stated boundary.";
const ABSENT: &str = "0123456789abcdef0123456789abcdef01234567";
/// Every owner issue a row predicate looks up, with its predicate number.
const ROW_OWNERS: &[(u8, u64)] = &[
    (3, 3920),
    (4, 3921),
    (5, 3807),
    (6, 3787),
    (6, 3811),
    (7, 3922),
    (7, 3924),
    (7, 4508),
    (8, 5391),
];
const DENY: &str = "[advisories]\nignore = [\n  \"RUSTSEC-2025-0075\",  # expires: 2026-12-31\n]\n";

struct Repo {
    root: PathBuf,
}

impl Repo {
    fn new(label: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-freeze-{label}-{nonce}"));
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let repo = Self { root };
        repo.git(&["init", "--quiet"])?;
        repo.git(&["config", "user.name", "test"])?;
        repo.git(&["config", "user.email", "test@example.invalid"])?;
        repo.git(&["config", "commit.gpgsign", "false"])?;
        repo.git(&["checkout", "--quiet", "-b", "trunk"])?;
        Ok(repo)
    }

    fn git(&self, values: &[&str]) -> Result<String, String> {
        let args: Vec<String> = values.iter().map(|value| (*value).to_string()).collect();
        let output = crate::run::capture_bytes_in_dir_with_timeout(
            Path::new("git"),
            &args,
            &self.root,
            &[],
            &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
            Duration::from_secs(30),
            "freeze-readiness fixture Git",
        )?;
        if output.timed_out || !output.status.is_some_and(|status| status.success()) {
            return Err(format!(
                "fixture Git {values:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// A directory beside the repository, outside its tree, removed with it.
    fn aux(&self) -> Result<PathBuf, String> {
        let mut name = self.root.clone().into_os_string();
        name.push("-aux");
        let path = PathBuf::from(name);
        fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, text).map_err(|error| error.to_string())
    }

    fn commit(&self, files: &[(&str, &str)], message: &str) -> Result<String, String> {
        for (path, text) in files {
            self.write(path, text)?;
        }
        self.git(&["add", "-A"])?;
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message])?;
        self.git(&["rev-parse", "HEAD"])
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        // Best effort: a failed removal must not turn a passing test red.
        let _removed = fs::remove_dir_all(&self.root);
        if let Ok(aux) = self.aux() {
            let _removed = fs::remove_dir_all(aux);
        }
    }
}

fn landed(issue: u64) -> Value {
    json!({
        "claim_id": format!("claim-{issue}"),
        "owner_issue": issue,
        "materiality": "release_required",
        "resolution": "landed_and_accepted",
        "implementation_refs": ["pr:100"],
        "proof_refs": ["https://github.com/EffortlessMetrics/ripr-swarm/issues/1"],
        "candidate_effect": "Present in the cut."
    })
}

/// A packet at `hard_cut_eligible` with one landed row per looked-up owner.
fn packet(observed: &str) -> Value {
    let issues: Vec<u64> = ROW_OWNERS.iter().map(|(_, issue)| *issue).collect();
    let mut census = issues.clone();
    census.push(20);
    json!({
        "schema_version": "1",
        "kind": "ripr_release_selection_packet",
        "release": "0.11.0",
        "authority_issue": 2766,
        "stage": "hard_cut_eligible",
        "observed_main": { "sha": observed },
        "claims": issues.iter().map(|issue| landed(*issue)).collect::<Vec<_>>(),
        "outside_selection": [{ "issue": 20, "reason": "post_release", "note": SENTENCE }],
        "open_prs": { "captured_at_main": observed, "rows": [] },
        "census": { "captured_on": "2026-11-21", "open_release_check_issues": census },
        "non_claims": ["Not a qualification."]
    })
}

fn admission(state: &str, sha: &str, tree: &str) -> String {
    json!({
        "schema_version": "1",
        "kind": "ripr_release_cut_admission",
        "release": "0.11.0",
        "state": state,
        "generation": "cut-1",
        "observed_main": { "sha": sha, "tree": tree }
    })
    .to_string()
}

fn check_run(id: u64, name: &str, sha: &str, conclusion: &str, day: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "head_sha": sha,
        "status": "completed",
        "conclusion": conclusion,
        "completed_at": format!("{day}T21:06:28Z"),
        "check_suite": { "id": 10 }
    })
}

/// A cargo-deny run and the skipped `dependency-review` of the same workflow
/// run, which is what a schedule or an input-free dispatch produces.
fn deny_runs(sha: &str, conclusion: &str, day: &str) -> Vec<Value> {
    vec![
        check_run(1, DENY_CHECK, sha, conclusion, day),
        check_run(4, DEPENDENCY_REVIEW_CHECK, sha, "skipped", day),
    ]
}

/// A workflow carrying exactly the lines the subject proof relies on.
fn security_workflow() -> String {
    SECURITY_SUBJECT_MARKERS.join("\n")
}

/// The fixture's moving parts, so a control can rebuild from any step.
struct Cut {
    repo: Repo,
    public: String,
    observed: String,
    candidate: String,
    tip: String,
    checks: PathBuf,
}

impl Cut {
    /// public head, product landing `(#100)`, packet commit (the candidate),
    /// then the packet-only activation commit and both hosted observations.
    fn build(label: &str, edit: impl FnOnce(&mut Value)) -> Result<Self, String> {
        let repo = Repo::new(label)?;
        let public = repo.commit(&[("README.md", "public\n")], "public head")?;
        let observed = repo.commit(
            &[
                ("deny.toml", DENY),
                ("src/lib.rs", "pub fn f() {}\n"),
                (SECURITY_WORKFLOW_PATH, &security_workflow()),
            ],
            "feat: product (#100)",
        )?;
        let mut selection = packet(&observed);
        edit(&mut selection);
        let candidate = repo.commit(
            &[(RELEASE_SELECTION_PATH, &selection.to_string())],
            "control: selection packet",
        )?;
        let mut cut = Self {
            repo,
            public,
            observed,
            candidate,
            tip: String::new(),
            checks: PathBuf::new(),
        };
        let tree = cut.tree(&cut.candidate)?;
        cut.tip = cut.repo.commit(
            &[(
                ADMISSION_PACKET_PATH,
                &admission("active", &cut.candidate, &tree),
            )],
            "control: activate the freeze",
        )?;
        cut.checks = cut.hosted_with_deny(
            deny_runs(&cut.candidate, "success", TODAY),
            &[check_run(2, WATCH_CHECK, &cut.tip, "success", TODAY)],
        )?;
        Ok(cut)
    }

    fn ready(label: &str) -> Result<Self, String> {
        Self::build(label, |_| {})
    }

    fn tree(&self, sha: &str) -> Result<String, String> {
        self.repo.git(&["rev-parse", &format!("{sha}^{{tree}}")])
    }

    /// Write a check-runs response beside the repository, outside its tree.
    fn hosted(&self, runs: &[Value]) -> Result<PathBuf, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let aux = self.repo.aux()?;
        let path = aux.join(format!("checks-{nonce}.json"));
        fs::write(&path, json!({ "check_runs": runs }).to_string())
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    fn hosted_with_deny(&self, mut runs: Vec<Value>, more: &[Value]) -> Result<PathBuf, String> {
        runs.extend_from_slice(more);
        self.hosted(&runs)
    }

    fn request(&self) -> Request<'_> {
        Request {
            repo: &self.repo.root,
            candidate: &self.candidate,
            public_head: Some(&self.public),
            control_tip: Some(&self.tip),
            hosted_checks: std::slice::from_ref(&self.checks),
            today: TODAY,
        }
    }

    fn evaluate(&self) -> Result<Receipt, String> {
        evaluate(&self.request())
    }
}

fn states(receipt: &Receipt) -> Vec<(u8, State)> {
    receipt
        .predicates
        .iter()
        .map(|predicate| (predicate.number, predicate.state))
        .collect()
}

fn state_of(receipt: &Receipt, number: u8) -> Option<State> {
    receipt
        .predicates
        .iter()
        .find(|predicate| predicate.number == number)
        .map(|predicate| predicate.state)
}

/// Assert the named predicates hold the given states and all others pass.
fn assert_states(receipt: &Receipt, expected: &[(u8, State)]) {
    let wanted: Vec<(u8, State)> = (1..=8)
        .map(|number| {
            let state = expected
                .iter()
                .find(|(owner, _)| *owner == number)
                .map_or(State::Pass, |(_, state)| *state);
            (number, state)
        })
        .collect();
    assert_eq!(states(receipt), wanted, "{:#?}", receipt.predicates);
    let ready = expected.is_empty();
    assert_eq!(receipt.verdict, if ready { "ready" } else { "not_ready" });
}

fn remove_row(selection: &mut Value, issue: u64) {
    if let Some(claims) = selection["claims"].as_array_mut() {
        claims.retain(|claim| claim["owner_issue"] != json!(issue));
    }
    if let Some(census) = selection["census"]["open_release_check_issues"].as_array_mut() {
        census.retain(|entry| entry != &json!(issue));
    }
}

fn set_row(selection: &mut Value, issue: u64, key: &str, value: Value) {
    if let Some(claims) = selection["claims"].as_array_mut() {
        for claim in claims
            .iter_mut()
            .filter(|claim| claim["owner_issue"] == json!(issue))
        {
            claim[key] = value.clone();
        }
    }
}

#[test]
fn every_predicate_passes_on_a_complete_cut() -> Result<(), String> {
    let cut = Cut::ready("ready")?;
    let receipt = cut.evaluate()?;
    assert_states(&receipt, &[]);
    for predicate in &receipt.predicates {
        assert!(
            !predicate.evidence.is_empty() && predicate.findings.is_empty(),
            "{predicate:#?}"
        );
    }
    Ok(())
}

#[test]
fn ready_needs_all_eight_predicates() -> Result<(), String> {
    let cut = Cut::ready("denominator")?;
    let mut receipt = cut.evaluate()?;
    assert_eq!(verdict(&receipt.predicates), "ready");
    receipt.predicates.pop();
    assert_eq!(verdict(&receipt.predicates), "not_ready");
    Ok(())
}

#[test]
fn a_predicate_that_read_nothing_is_not_established() {
    let predicate = Predicate::new(3, "schema_convergence", "t", &[3920]).finish();
    assert_eq!(predicate.state, State::NotEstablished);
}

#[test]
fn public_head_must_be_an_imported_ancestor() -> Result<(), String> {
    let cut = Cut::ready("public")?;
    let absent = Request {
        public_head: Some(ABSENT),
        ..cut.request()
    };
    assert_states(&evaluate(&absent)?, &[(1, State::Failed)]);

    let unsupplied = Request {
        public_head: None,
        ..cut.request()
    };
    assert_states(&evaluate(&unsupplied)?, &[(1, State::NotEstablished)]);

    // Present in the object store but on a line the candidate never merged.
    cut.repo
        .git(&["checkout", "--quiet", "-b", "side", &cut.public])?;
    let side = cut.repo.commit(&[("side.txt", "s\n")], "public moved on")?;
    let diverged = Request {
        public_head: Some(&side),
        ..cut.request()
    };
    assert_states(&evaluate(&diverged)?, &[(1, State::Failed)]);
    Ok(())
}

#[test]
fn packet_stage_must_be_hard_cut_eligible() -> Result<(), String> {
    let cut = Cut::build("stage", |selection| {
        selection["stage"] = json!("scope_closed");
    })?;
    assert_states(&cut.evaluate()?, &[(2, State::Failed)]);
    Ok(())
}

#[test]
fn packet_must_describe_the_candidate_tree() -> Result<(), String> {
    // A product commit lands between the main the packet observed and the
    // packet commit, so the packet's roster describes an older tree.
    let repo = Repo::new("stale")?;
    let public = repo.commit(&[("README.md", "public\n")], "public head")?;
    let observed = repo.commit(&[("deny.toml", DENY)], "feat: product (#100)")?;
    repo.commit(
        &[("src/late.rs", "pub fn late() {}\n")],
        "feat: late (#101)",
    )?;
    let candidate = repo.commit(
        &[(RELEASE_SELECTION_PATH, &packet(&observed).to_string())],
        "control: selection packet",
    )?;
    let receipt = evaluate(&Request {
        repo: &repo.root,
        candidate: &candidate,
        public_head: Some(&public),
        control_tip: None,
        hosted_checks: &[],
        today: TODAY,
    })?;
    assert_eq!(state_of(&receipt, 2), Some(State::Failed));
    assert!(
        receipt
            .predicates
            .iter()
            .flat_map(|predicate| &predicate.findings)
            .any(|finding| finding.contains("src/late.rs")),
        "{receipt:#?}"
    );
    Ok(())
}

#[test]
fn landed_rows_must_cite_a_landing_in_candidate_history() -> Result<(), String> {
    let unlanded = Cut::build("unlanded", |selection| {
        set_row(selection, 3920, "implementation_refs", json!(["pr:999"]));
    })?;
    assert_states(&unlanded.evaluate()?, &[(2, State::Failed)]);

    let absent_commit = Cut::build("absent-commit", |selection| {
        set_row(
            selection,
            3920,
            "implementation_refs",
            json!([format!("commit:{ABSENT}")]),
        );
    })?;
    assert_states(&absent_commit.evaluate()?, &[(2, State::Failed)]);

    // The same row passes when it cites a commit the candidate contains.
    let repo = Repo::new("commit-ref-own")?;
    let public = repo.commit(&[("README.md", "public\n")], "public head")?;
    let observed = repo.commit(&[("deny.toml", DENY)], "feat: product")?;
    let mut selection = packet(&observed);
    for (_, issue) in ROW_OWNERS {
        set_row(
            &mut selection,
            *issue,
            "implementation_refs",
            json!([format!("commit:{observed}")]),
        );
    }
    let candidate = repo.commit(
        &[(RELEASE_SELECTION_PATH, &selection.to_string())],
        "control: selection packet",
    )?;
    let receipt = evaluate(&Request {
        repo: &repo.root,
        candidate: &candidate,
        public_head: Some(&public),
        control_tip: None,
        hosted_checks: &[],
        today: TODAY,
    })?;
    assert_eq!(state_of(&receipt, 2), Some(State::Pass), "{receipt:#?}");
    Ok(())
}

#[test]
fn each_row_owner_decides_exactly_its_predicate() -> Result<(), String> {
    for (number, issue) in ROW_OWNERS {
        let missing = Cut::build(&format!("missing-{issue}"), |selection| {
            remove_row(selection, *issue);
        })?;
        assert_states(&missing.evaluate()?, &[(*number, State::NotEstablished)]);
    }
    Ok(())
}

#[test]
fn pending_and_held_rows_are_not_terminal() -> Result<(), String> {
    // The packet's own stage law also refuses these at hard_cut_eligible, so
    // predicate 2 fails with the row's predicate.
    let pending = Cut::build("pending", |selection| {
        set_row(selection, 3920, "resolution", json!("pending"));
        set_row(
            selection,
            3920,
            "next",
            json!("Owner decides before the cut."),
        );
    })?;
    assert_states(
        &pending.evaluate()?,
        &[(2, State::Failed), (3, State::Limited)],
    );

    let held = Cut::build("held", |selection| {
        set_row(selection, 3807, "resolution", json!("failed_release_hold"));
        set_row(
            selection,
            3807,
            "next",
            json!("Repair the failing audit row."),
        );
    })?;
    let receipt = held.evaluate()?;
    assert_eq!(state_of(&receipt, 5), Some(State::Failed), "{receipt:#?}");
    assert_eq!(receipt.verdict, "not_ready");
    Ok(())
}

#[test]
fn advisory_ignores_must_be_unexpired_on_the_evaluation_day() -> Result<(), String> {
    let cut = Cut::ready("expiry")?;
    let fresh = cut.hosted_with_deny(
        deny_runs(&cut.candidate, "success", "2027-01-01"),
        &[check_run(2, WATCH_CHECK, &cut.tip, "success", "2027-01-01")],
    )?;
    let later = Request {
        today: "2027-01-01",
        hosted_checks: std::slice::from_ref(&fresh),
        ..cut.request()
    };
    assert_states(&evaluate(&later)?, &[(6, State::Failed)]);
    Ok(())
}

#[test]
fn cargo_deny_must_have_passed_on_the_candidate_today() -> Result<(), String> {
    let cut = Cut::ready("deny")?;
    let watch = check_run(2, WATCH_CHECK, &cut.tip, "success", TODAY);
    let controls = [
        (deny_runs(&cut.candidate, "failure", TODAY), State::Failed),
        // A green run on another commit says nothing about the candidate.
        (
            deny_runs(&cut.observed, "success", TODAY),
            State::NotEstablished,
        ),
        // A green run from an earlier day predates today's advisories.
        (
            deny_runs(&cut.candidate, "success", "2026-11-17"),
            State::NotEstablished,
        ),
    ];
    for (deny, state) in controls {
        let checks = cut.hosted_with_deny(deny, std::slice::from_ref(&watch))?;
        let request = Request {
            hosted_checks: std::slice::from_ref(&checks),
            ..cut.request()
        };
        assert_states(&evaluate(&request)?, &[(6, state)]);
    }

    // A later failing rerun overrides an earlier success on the same commit.
    let rerun = cut.hosted_with_deny(
        deny_runs(&cut.candidate, "success", TODAY),
        &[
            check_run(9, DENY_CHECK, &cut.candidate, "failure", TODAY),
            check_run(3, "unrelated", &cut.candidate, "success", TODAY),
            watch,
        ],
    )?;
    let request = Request {
        hosted_checks: std::slice::from_ref(&rerun),
        ..cut.request()
    };
    assert_states(&evaluate(&request)?, &[(6, State::Failed)]);

    let unobserved = Request {
        hosted_checks: &[],
        ..cut.request()
    };
    assert_states(
        &evaluate(&unobserved)?,
        &[(6, State::NotEstablished), (8, State::NotEstablished)],
    );
    Ok(())
}

/// The failure this guards against happened on main on 2026-11-21: a manual
/// dispatch selected another commit with `head_sha`, and its green `cargo-deny`
/// check run attached to main's head.
#[test]
fn a_green_cargo_deny_run_must_prove_which_commit_it_scanned() -> Result<(), String> {
    let cut = Cut::ready("deny-subject")?;
    let watch = check_run(2, WATCH_CHECK, &cut.tip, "success", TODAY);
    let deny = check_run(1, DENY_CHECK, &cut.candidate, "success", TODAY);
    let mut other_run = check_run(4, DEPENDENCY_REVIEW_CHECK, &cut.candidate, "skipped", TODAY);
    other_run["check_suite"]["id"] = json!(11);
    let unbound = [
        // dependency-review ran: a base/head pair was selected by hand.
        vec![
            deny.clone(),
            check_run(4, DEPENDENCY_REVIEW_CHECK, &cut.candidate, "success", TODAY),
        ],
        // Its sibling result was not captured.
        vec![deny.clone()],
        // A failed scan of a manually selected commit is not the candidate's
        // failure: there is still no result for the candidate.
        vec![
            check_run(1, DENY_CHECK, &cut.candidate, "failure", TODAY),
            check_run(4, DEPENDENCY_REVIEW_CHECK, &cut.candidate, "success", TODAY),
        ],
        // A skipped dependency-review from a different workflow run.
        vec![deny.clone(), other_run],
    ];
    for runs in unbound {
        let checks = cut.hosted_with_deny(runs, std::slice::from_ref(&watch))?;
        let request = Request {
            hosted_checks: std::slice::from_ref(&checks),
            ..cut.request()
        };
        assert_states(&evaluate(&request)?, &[(6, State::NotEstablished)]);
    }

    // A later manual run on another commit neither rescues an honest failure
    // nor displaces an honest success.
    let mut manual = vec![
        check_run(20, DENY_CHECK, &cut.candidate, "success", TODAY),
        check_run(
            21,
            DEPENDENCY_REVIEW_CHECK,
            &cut.candidate,
            "success",
            TODAY,
        ),
    ];
    for run in &mut manual {
        run["check_suite"]["id"] = json!(12);
    }
    for (honest, expected) in [("failure", Some(State::Failed)), ("success", None)] {
        let mut runs = deny_runs(&cut.candidate, honest, TODAY);
        runs.extend(manual.clone());
        let checks = cut.hosted_with_deny(runs, std::slice::from_ref(&watch))?;
        let request = Request {
            hosted_checks: std::slice::from_ref(&checks),
            ..cut.request()
        };
        let expected: Vec<(u8, State)> = expected.map(|state| (6, state)).into_iter().collect();
        assert_states(&evaluate(&request)?, &expected);
    }

    // The proof depends on the workflow the candidate itself carries.
    let reshaped = Repo::new("deny-workflow")?;
    let public = reshaped.commit(&[("README.md", "public\n")], "public head")?;
    let observed = reshaped.commit(
        &[("deny.toml", DENY), (SECURITY_WORKFLOW_PATH, "jobs: {}\n")],
        "feat: product (#100)",
    )?;
    let candidate = reshaped.commit(
        &[(RELEASE_SELECTION_PATH, &packet(&observed).to_string())],
        "control: selection packet",
    )?;
    let checks = Cut {
        repo: reshaped,
        public,
        observed,
        candidate,
        tip: String::new(),
        checks: PathBuf::new(),
    };
    let file = checks.hosted(&deny_runs(&checks.candidate, "success", TODAY))?;
    let receipt = evaluate(&Request {
        control_tip: None,
        hosted_checks: std::slice::from_ref(&file),
        ..checks.request()
    })?;
    assert_states(
        &receipt,
        &[(6, State::NotEstablished), (8, State::NotEstablished)],
    );
    Ok(())
}

#[test]
fn cargo_deny_may_run_on_a_packet_only_control_tip() -> Result<(), String> {
    // After activation main's head is the control tip, which carries the same
    // lockfile and deny.toml as the candidate.
    let cut = Cut::ready("deny-tip")?;
    let watch = check_run(2, WATCH_CHECK, &cut.tip, "success", TODAY);
    let on_tip = cut.hosted_with_deny(
        deny_runs(&cut.tip, "success", TODAY),
        std::slice::from_ref(&watch),
    )?;
    let request = Request {
        hosted_checks: std::slice::from_ref(&on_tip),
        ..cut.request()
    };
    assert_states(&evaluate(&request)?, &[]);

    // A tip that also changed product code is not an equal subject.
    cut.repo
        .git(&["checkout", "--quiet", "--detach", &cut.candidate])?;
    let tree = cut.tree(&cut.candidate)?;
    let moved = cut.repo.commit(
        &[
            (
                ADMISSION_PACKET_PATH,
                admission("active", &cut.candidate, &tree).as_str(),
            ),
            ("Cargo.lock", "changed\n"),
        ],
        "control and product",
    )?;
    let checks = cut.hosted_with_deny(
        deny_runs(&moved, "success", TODAY),
        &[check_run(2, WATCH_CHECK, &moved, "success", TODAY)],
    )?;
    let request = Request {
        control_tip: Some(&moved),
        hosted_checks: std::slice::from_ref(&checks),
        ..cut.request()
    };
    assert_states(
        &evaluate(&request)?,
        &[(6, State::NotEstablished), (8, State::Failed)],
    );
    Ok(())
}

/// The subject proof is only as good as the workflow it describes. If
/// `security.yml` changes shape, this fails here instead of at the freeze.
#[test]
fn the_repository_security_workflow_has_the_shape_the_proof_relies_on() {
    let workflow = include_str!("../../../../.github/workflows/security.yml");
    for marker in SECURITY_SUBJECT_MARKERS {
        assert!(workflow.contains(marker), "missing: {marker}");
    }
}

#[test]
fn freeze_must_be_active_on_exactly_the_candidate() -> Result<(), String> {
    let cut = Cut::ready("activation")?;
    let unsupplied = Request {
        control_tip: None,
        ..cut.request()
    };
    assert_states(&evaluate(&unsupplied)?, &[(8, State::NotEstablished)]);

    // An ancestor of the candidate cannot carry its activation.
    let backwards = Request {
        control_tip: Some(&cut.observed),
        ..cut.request()
    };
    assert_states(&evaluate(&backwards)?, &[(8, State::Failed)]);

    let failed_watch = cut.hosted_with_deny(
        deny_runs(&cut.candidate, "success", TODAY),
        &[check_run(2, WATCH_CHECK, &cut.tip, "failure", TODAY)],
    )?;
    let request = Request {
        hosted_checks: std::slice::from_ref(&failed_watch),
        ..cut.request()
    };
    assert_states(&evaluate(&request)?, &[(8, State::Failed)]);

    let tree = cut.tree(&cut.candidate)?;
    let other_tree = cut.tree(&cut.observed)?;
    let tips = [
        (
            "inactive",
            admission("inactive", &cut.candidate, &tree),
            None,
        ),
        // After #1609 the packet is pinned, merges resume and the watch stops
        // judging main, so a pinned tip is not a freeze.
        (
            "released",
            admission("candidate_pinned", &cut.candidate, &tree),
            None,
        ),
        (
            "another cut",
            admission("active", &cut.observed, &other_tree),
            None,
        ),
        (
            "wrong tree",
            admission("active", &cut.candidate, &other_tree),
            None,
        ),
        (
            "main moved",
            admission("active", &cut.candidate, &tree),
            Some(("src/lib.rs", "pub fn g() {}\n")),
        ),
    ];
    for (label, body, product) in tips {
        cut.repo
            .git(&["checkout", "--quiet", "--detach", &cut.candidate])?;
        let mut files = vec![(ADMISSION_PACKET_PATH, body.as_str())];
        files.extend(product);
        let tip = cut.repo.commit(&files, label)?;
        let checks = cut.hosted_with_deny(
            deny_runs(&cut.candidate, "success", TODAY),
            &[check_run(2, WATCH_CHECK, &tip, "success", TODAY)],
        )?;
        let request = Request {
            control_tip: Some(&tip),
            hosted_checks: std::slice::from_ref(&checks),
            ..cut.request()
        };
        let receipt = evaluate(&request)?;
        assert_eq!(
            state_of(&receipt, 8),
            Some(State::Failed),
            "{label}: {receipt:#?}"
        );
        assert_eq!(receipt.verdict, "not_ready", "{label}");
    }
    Ok(())
}

#[test]
fn working_tree_and_earlier_receipts_cannot_change_the_result() -> Result<(), String> {
    // Committed state is not ready; the working tree and a forged receipt say
    // it is.
    let blocked = Cut::build("forged-ready", |selection| {
        selection["stage"] = json!("scope_closed");
    })?;
    let before = states(&blocked.evaluate()?);
    blocked.repo.write(
        RELEASE_SELECTION_PATH,
        &packet(&blocked.observed).to_string(),
    )?;
    blocked.repo.write(
        "target/ripr/reports/freeze-readiness.json",
        "{\"verdict\":\"ready\"}\n",
    )?;
    let receipt = blocked.evaluate()?;
    assert_eq!(states(&receipt), before);
    assert_states(&receipt, &[(2, State::Failed)]);

    // Committed state is ready; the working tree is damaged.
    let ready = Cut::ready("dirty-tree")?;
    ready.repo.write(RELEASE_SELECTION_PATH, "not json")?;
    ready.repo.write("deny.toml", "")?;
    ready.repo.write(ADMISSION_PACKET_PATH, "{}")?;
    assert_states(&ready.evaluate()?, &[]);
    Ok(())
}

#[test]
fn moving_or_incomplete_subjects_are_refused() -> Result<(), String> {
    let cut = Cut::ready("subjects")?;
    let abbreviated = cut.candidate.get(..12).unwrap_or("abc");
    for candidate in ["HEAD", "trunk", abbreviated] {
        let request = Request {
            candidate,
            ..cut.request()
        };
        let error = evaluate(&request).err().unwrap_or_default();
        assert!(error.contains("full 40-character"), "{candidate}: {error}");
    }
    let absent = Request {
        candidate: ABSENT,
        ..cut.request()
    };
    let error = evaluate(&absent).err().unwrap_or_default();
    assert!(error.contains("is not a commit"), "{error}");

    // A shallow clone cannot answer ancestry, so it must not answer at all.
    let shallow = cut.repo.aux()?.join("shallow");
    // `--no-local` makes a path clone honor `--depth` on every platform.
    cut.repo.git(&[
        "clone",
        "--quiet",
        "--no-local",
        "--depth",
        "1",
        &cut.repo.root.display().to_string(),
        &shallow.display().to_string(),
    ])?;
    let request = Request {
        repo: &shallow,
        ..cut.request()
    };
    let error = evaluate(&request).err().unwrap_or_default();
    assert!(error.contains("shallow"), "{error}");
    Ok(())
}

#[test]
fn landings_are_read_from_squash_and_merge_subjects() {
    for (subject, number) in [
        ("fix(x): thing (#15264) (#15876)", Some(15876)),
        ("Merge pull request #1439 from a/b", Some(1439)),
        ("fix(cli): cache help (#3786)", Some(3786)),
        ("fix: mentions (#12) mid-subject", None),
        ("no number", None),
    ] {
        assert_eq!(pr_number_in_subject(subject), number, "{subject}");
    }
}
