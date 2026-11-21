//! One-command freeze readiness for the 0.11 swarm cut (#1609).
//!
//! `cargo xtask freeze-readiness --candidate <sha>` evaluates the eight
//! predicates that gate the freeze against one exact candidate commit and
//! writes a receipt. It replaces a hand audit of the same eight rows.
//!
//! The check is built so it cannot confirm itself:
//!
//! - every repository input is a Git object read at a named commit
//!   (`<sha>:<path>`), never the working tree, so an uncommitted packet edit or
//!   an earlier receipt on disk cannot change the result;
//! - the selected-claim packet is compared with facts it does not author: Git
//!   ancestry, the paths changed since the main it says it observed, and the
//!   landings its rows cite;
//! - a predicate that read no subject is `not_established`, never `pass`, and
//!   `ready` needs all eight at `pass`;
//! - the receipt is an output only. Nothing here reads a report directory.
//!
//! Hosted results (cargo-deny on the candidate, the hard-cut watch on the
//! control tip) cannot be derived from Git. They enter as check-run JSON the
//! operator captures with `gh api`, bound by `head_sha`; without them the
//! predicate stays `not_established`.
//!
//! The packet schema, vocabulary and stage law stay with `release_selection`;
//! advisory expiry stays with `dependency_expiry`. This module owns only the
//! comparison of those results with the candidate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::dependency_expiry::{check_dependency_suppression_expiry_text, utc_date_string};
use super::release_selection::{
    HARD_CUT_ELIGIBLE_STAGE, RELEASE_SELECTION_PATH, RowCutState, SelectionProjection,
    project_release_selection, row_cut_state,
};

const ADMISSION_PACKET_PATH: &str = "policy/release-hard-cut-admission.json";
const DENY_PATH: &str = "deny.toml";
const RELEASE: &str = "0.11.0";
const DENY_CHECK: &str = "cargo-deny";
const WATCH_CHECK: &str = "Hard-cut main watch";
const SECURITY_WORKFLOW_PATH: &str = ".github/workflows/security.yml";
const DEPENDENCY_REVIEW_CHECK: &str = "dependency-review";
/// The two lines of `security.yml` that make a skipped `dependency-review` job
/// prove what `cargo-deny` scanned. A manual dispatch may select another commit
/// with `base_sha`/`head_sha`; the check run still attaches to the dispatched
/// ref's head, so its `head_sha` alone does not name the scanned commit. The
/// first line moves the cargo-deny subject only when an input is set; the
/// second runs `dependency-review` on a dispatch exactly when an input is set.
/// So in one workflow run, `dependency-review` skipped means cargo-deny scanned
/// the commit the check run names.
pub(super) const SECURITY_SUBJECT_MARKERS: &[&str] = &[
    r#"if [[ -n "$INPUT_BASE_SHA" || -n "$INPUT_HEAD_SHA" ]]; then"#,
    "(github.event_name == 'workflow_dispatch' && (inputs.base_sha != '' || inputs.head_sha != ''))",
];
const JSON_NAME: &str = "freeze-readiness.json";
const MD_NAME: &str = "freeze-readiness.md";
const GIT_TIMEOUT: Duration = Duration::from_mins(2);
/// Findings listed per predicate before the rest are counted, so one broken
/// packet cannot bury the other seven rows.
const MAX_LISTED: usize = 12;

const NON_CLAIMS: &[&str] = &[
    "Readiness is the start condition for the freeze. It does not create the candidate ref, activate the freeze, qualify, tag, sign or publish anything.",
    "A packet row is the #2766 packet's recorded disposition, compared here with Git where Git can answer. A row marked final_candidate_execution_only still owes its proof on the exact candidate under #2769. A cited landing is checked for presence in the candidate's history, not for surviving a later revert.",
    "Hosted check runs are operator-captured observations bound by head_sha. This command does not read GitHub and cannot establish that the public head or the control tip is the current remote head.",
    "A receipt describes the named candidate only. Any later commit on main needs a new run.",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Pass,
    Limited,
    NotEstablished,
    Failed,
}

impl State {
    fn rank(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Limited => 1,
            Self::NotEstablished => 2,
            Self::Failed => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Limited => "limited",
            Self::NotEstablished => "not_established",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct Predicate {
    pub(super) number: u8,
    pub(super) id: &'static str,
    title: &'static str,
    owner_issues: Vec<u64>,
    pub(super) state: State,
    /// Why the predicate is not `pass`.
    pub(super) findings: Vec<String>,
    /// What was read to reach the state.
    pub(super) evidence: Vec<String>,
}

impl Predicate {
    fn new(number: u8, id: &'static str, title: &'static str, owner_issues: &[u64]) -> Self {
        Self {
            number,
            id,
            title,
            owner_issues: owner_issues.to_vec(),
            state: State::Pass,
            findings: Vec::new(),
            evidence: Vec::new(),
        }
    }

    fn lower(&mut self, state: State, finding: impl Into<String>) {
        if state.rank() > self.state.rank() {
            self.state = state;
        }
        self.findings.push(finding.into());
    }

    fn saw(&mut self, evidence: impl Into<String>) {
        self.evidence.push(evidence.into());
    }

    /// A predicate that consumed nothing has established nothing.
    fn finish(mut self) -> Self {
        if self.state == State::Pass && self.evidence.is_empty() {
            self.lower(State::NotEstablished, "no subject was read");
        }
        if self.findings.len() > MAX_LISTED {
            let hidden = self.findings.len() - MAX_LISTED;
            self.findings.truncate(MAX_LISTED);
            self.findings.push(format!("and {hidden} more"));
        }
        self
    }
}

#[derive(Debug, Serialize)]
struct CommitIdentity {
    sha: String,
    tree: String,
}

#[derive(Debug, Serialize)]
struct Input {
    kind: &'static str,
    subject: String,
    digest: String,
}

#[derive(Debug, Serialize)]
pub(super) struct Receipt {
    schema_version: &'static str,
    kind: &'static str,
    release: &'static str,
    pub(super) verdict: &'static str,
    candidate: CommitIdentity,
    public_head: Option<String>,
    control_tip: Option<CommitIdentity>,
    evaluated_on: String,
    pub(super) predicates: Vec<Predicate>,
    inputs: Vec<Input>,
    non_claims: &'static [&'static str],
}

pub(super) struct Request<'a> {
    pub(super) repo: &'a Path,
    pub(super) candidate: &'a str,
    pub(super) public_head: Option<&'a str>,
    pub(super) control_tip: Option<&'a str>,
    pub(super) hosted_checks: &'a [PathBuf],
    pub(super) today: &'a str,
}

struct CheckRun {
    id: u64,
    name: String,
    head_sha: String,
    status: String,
    conclusion: Option<String>,
    completed_at: Option<String>,
    suite: Option<u64>,
}

struct Git<'a> {
    repo: &'a Path,
}

impl Git<'_> {
    fn run(&self, args: &[&str]) -> Result<(Option<i32>, Vec<u8>, String), String> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let label = format!(
            "freeze-readiness git {}",
            args.first().copied().unwrap_or("")
        );
        let output = crate::run::capture_bytes_in_dir_with_timeout(
            Path::new("git"),
            &owned,
            self.repo,
            &[],
            &["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"],
            GIT_TIMEOUT,
            &label,
        )?;
        if output.timed_out {
            return Err(format!("{label} timed out"));
        }
        Ok((
            output.status.and_then(|status| status.code()),
            output.stdout,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }

    fn text(&self, args: &[&str]) -> Result<String, String> {
        match self.run(args)? {
            (Some(0), stdout, _) => String::from_utf8(stdout)
                .map(|text| text.trim().to_string())
                .map_err(|_error| format!("git {} produced non-UTF-8 output", args.join(" "))),
            (_, _, stderr) => Err(format!("git {} failed: {stderr}", args.join(" "))),
        }
    }

    /// Whether `sha` names a commit object present in this repository.
    fn has_commit(&self, sha: &str) -> Result<bool, String> {
        let spec = format!("{sha}^{{commit}}");
        Ok(self.run(&["cat-file", "-e", &spec])?.0 == Some(0))
    }

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, String> {
        match self.run(&["merge-base", "--is-ancestor", ancestor, descendant])? {
            (Some(0), _, _) => Ok(true),
            (Some(1), _, _) => Ok(false),
            (_, _, stderr) => Err(format!(
                "git merge-base --is-ancestor {ancestor} {descendant} failed: {stderr}"
            )),
        }
    }

    /// The blob at `<rev>:<path>` as `(object id, bytes)`, or `None` when the
    /// commit has no such file.
    fn blob(&self, rev: &str, path: &str) -> Result<Option<(String, Vec<u8>)>, String> {
        let spec = format!("{rev}:{path}");
        let (code, stdout, _) = self.run(&["rev-parse", "--verify", "--quiet", &spec])?;
        if code != Some(0) {
            return Ok(None);
        }
        let oid = String::from_utf8_lossy(&stdout).trim().to_string();
        match self.run(&["cat-file", "blob", &oid])? {
            (Some(0), bytes, _) => Ok(Some((oid, bytes))),
            (_, _, stderr) => Err(format!("git cat-file blob {oid} failed: {stderr}")),
        }
    }

    fn changed_paths(&self, from: &str, to: &str) -> Result<Vec<String>, String> {
        let text = self.text(&["diff", "--name-only", "--no-renames", "-z", from, to])?;
        Ok(text
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Pull requests whose landing is visible in the subjects reachable from
    /// `rev`: a squash subject ending `(#N)` or a `Merge pull request #N`.
    fn landed_prs(&self, rev: &str) -> Result<BTreeSet<u64>, String> {
        let text = self.text(&["log", "--format=%s", rev])?;
        Ok(text.lines().filter_map(pr_number_in_subject).collect())
    }
}

fn pr_number_in_subject(subject: &str) -> Option<u64> {
    let subject = subject.trim_end();
    if let Some(rest) = subject.strip_prefix("Merge pull request #") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        return digits.parse().ok();
    }
    let open = subject.rfind("(#")?;
    subject.get(open + 2..)?.strip_suffix(')')?.parse().ok()
}

fn is_full_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn require_full_sha(flag: &str, value: &str) -> Result<(), String> {
    if is_full_sha(value) {
        return Ok(());
    }
    Err(format!(
        "{flag} must be a full 40-character lowercase commit id, not `{value}`; a branch or short id can move"
    ))
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

fn load_check_runs(paths: &[PathBuf], inputs: &mut Vec<Input>) -> Result<Vec<CheckRun>, String> {
    let mut runs = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("--hosted-checks {}: {error}", path.display()))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("--hosted-checks {}: {error}", path.display()))?;
        let records = value
            .get("check_runs")
            .unwrap_or(&value)
            .as_array()
            .ok_or_else(|| {
                format!(
                    "--hosted-checks {}: expected the `check-runs` API response or an array of check runs",
                    path.display()
                )
            })?;
        for record in records {
            let field = |key: &str| record.get(key).and_then(Value::as_str).map(str::to_string);
            let (Some(id), Some(name), Some(head_sha), Some(status)) = (
                record.get("id").and_then(Value::as_u64),
                field("name"),
                field("head_sha"),
                field("status"),
            ) else {
                return Err(format!(
                    "--hosted-checks {}: a check run lacks id, name, head_sha or status",
                    path.display()
                ));
            };
            runs.push(CheckRun {
                id,
                name,
                head_sha,
                status,
                conclusion: field("conclusion"),
                completed_at: field("completed_at"),
                suite: record.pointer("/check_suite/id").and_then(Value::as_u64),
            });
        }
        inputs.push(Input {
            kind: "operator_file",
            subject: path.display().to_string(),
            digest: format!("sha256:{:x}", Sha256::digest(&bytes)),
        });
    }
    Ok(runs)
}

/// Judge the newest supplied run of `name` on exactly `sha`.
fn hosted(predicate: &mut Predicate, runs: &[CheckRun], name: &str, sha: &str) {
    let pool: Vec<&CheckRun> = runs
        .iter()
        .filter(|run| run.name == name && run.head_sha == sha)
        .collect();
    judge_newest_run(predicate, &pool, name, sha, None);
}

/// Judge the newest run in `pool` (runs of `name` on an accepted commit) and
/// return it. `sha` names the commit to capture when the pool is empty.
/// `fresh_on` is set for a result that can change without a commit: the run
/// must have completed on that UTC day, or it describes an older advisory
/// database.
fn judge_newest_run<'a>(
    predicate: &mut Predicate,
    pool: &[&'a CheckRun],
    name: &str,
    sha: &str,
    fresh_on: Option<&str>,
) -> Option<&'a CheckRun> {
    let Some(run) = pool.iter().copied().max_by_key(|run| run.id) else {
        predicate.lower(
            State::NotEstablished,
            format!(
                "no `{name}` check run for {} was supplied; capture `gh api \"repos/<owner>/<repo>/commits/{sha}/check-runs?filter=all&per_page=100\"` (every page) and pass it with --hosted-checks",
                short(sha)
            ),
        );
        return None;
    };
    let sha = run.head_sha.as_str();
    let conclusion = run.conclusion.as_deref().unwrap_or("none");
    predicate.saw(format!(
        "check run {} `{name}` on {}: {} / {conclusion}",
        run.id,
        short(sha),
        run.status
    ));
    if run.status != "completed" {
        predicate.lower(
            State::NotEstablished,
            format!("`{name}` on {} has not completed", short(sha)),
        );
    } else if conclusion != "success" {
        predicate.lower(
            State::Failed,
            format!("`{name}` on {} concluded {conclusion}", short(sha)),
        );
    } else if let Some(day) = fresh_on
        && !run
            .completed_at
            .as_deref()
            .is_some_and(|completed| completed.starts_with(day))
    {
        predicate.lower(
            State::NotEstablished,
            format!(
                "`{name}` on {} completed at {}, not on {day}; advisories change without a commit, so dispatch it again for this commit",
                short(sha),
                run.completed_at.as_deref().unwrap_or("an unrecorded time")
            ),
        );
    }
    Some(run)
}

/// The hosted cargo-deny result for the candidate, with proof of which commit
/// it scanned (see [`SECURITY_SUBJECT_MARKERS`]).
fn deny_observation(
    predicate: &mut Predicate,
    git: &Git,
    request: &Request,
    runs: &[CheckRun],
    inputs: &mut Vec<Input>,
) -> Result<(), String> {
    // After activation main's head is the control tip, so a dispatch can no
    // longer name the candidate. The tip is an equal subject only when nothing
    // but the admission packet separates it from the candidate.
    let mut subjects = vec![request.candidate];
    if let Some(tip) = request.control_tip
        && tip != request.candidate
        && git.has_commit(tip)?
        && git.is_ancestor(request.candidate, tip)?
        && git
            .changed_paths(request.candidate, tip)?
            .iter()
            .all(|path| path == ADMISSION_PACKET_PATH)
    {
        subjects.push(tip);
    }
    let sibling_of = |run: &CheckRun| {
        runs.iter()
            .filter(|other| {
                other.name == DEPENDENCY_REVIEW_CHECK
                    && other.head_sha == run.head_sha
                    && run.suite.is_some()
                    && other.suite == run.suite
            })
            .max_by_key(|other| other.id)
            .map(|other| other.conclusion.as_deref().unwrap_or("none"))
    };
    let on_subject: Vec<&CheckRun> = runs
        .iter()
        .filter(|run| run.name == DENY_CHECK && subjects.contains(&run.head_sha.as_str()))
        .collect();
    let Some(newest) = on_subject.iter().copied().max_by_key(|run| run.id) else {
        judge_newest_run(predicate, &[], DENY_CHECK, request.candidate, None);
        return Ok(());
    };
    // A run that cannot show which commit it scanned says nothing about the
    // candidate in either direction: its failure is not the candidate's
    // failure, and it must not displace a run that can.
    let bound: Vec<&CheckRun> = on_subject
        .iter()
        .copied()
        .filter(|run| sibling_of(run) == Some("skipped"))
        .collect();
    let run = bound
        .iter()
        .copied()
        .max_by_key(|run| run.id)
        .unwrap_or(newest);
    let workflow = git.blob(&run.head_sha, SECURITY_WORKFLOW_PATH)?;
    let shaped = workflow.as_ref().is_some_and(|(_, bytes)| {
        let text = String::from_utf8_lossy(bytes);
        SECURITY_SUBJECT_MARKERS
            .iter()
            .all(|marker| text.contains(marker))
    });
    if let Some((oid, _)) = &workflow {
        inputs.push(blob_input(&run.head_sha, SECURITY_WORKFLOW_PATH, oid));
    }
    let why_unbound = if !shaped {
        Some(
            "the workflow at that commit no longer has the subject-selection lines this check relies on",
        )
    } else {
        match sibling_of(run) {
            Some("skipped") => None,
            Some(_) => Some(
                "`dependency-review` ran in the same workflow run, which happens for a manually selected base/head or a pull request event",
            ),
            None => Some(
                "the supplied check runs lack the `dependency-review` result of the same workflow run",
            ),
        }
    };
    if let Some(why) = why_unbound {
        predicate.saw(format!(
            "check run {} `{DENY_CHECK}` attached to {}: {} / {}; its result is not used",
            run.id,
            short(&run.head_sha),
            run.status,
            run.conclusion.as_deref().unwrap_or("none")
        ));
        predicate.lower(
            State::NotEstablished,
            format!(
                "check run {} `{DENY_CHECK}` does not show that it scanned {}: {why}. Dispatch {SECURITY_WORKFLOW_PATH} on that commit with no base_sha or head_sha",
                run.id,
                short(&run.head_sha)
            ),
        );
        return Ok(());
    }
    judge_newest_run(
        predicate,
        &bound,
        DENY_CHECK,
        request.candidate,
        Some(request.today),
    );
    predicate.saw(format!(
        "`{DEPENDENCY_REVIEW_CHECK}` was skipped in the same workflow run, so no manual subject was selected and cargo-deny scanned {}",
        short(&run.head_sha)
    ));
    Ok(())
}

/// Require every named owner issue to have a terminal row in the packet read
/// at the candidate.
fn rows_terminal(
    predicate: &mut Predicate,
    selection: Option<&SelectionProjection>,
    issues: &[u64],
) {
    let Some(selection) = selection else {
        predicate.lower(
            State::NotEstablished,
            format!("{RELEASE_SELECTION_PATH} is absent at the candidate"),
        );
        return;
    };
    for issue in issues {
        let mut matched = false;
        for row in selection
            .rows
            .iter()
            .filter(|row| row.owner_issue == *issue)
        {
            matched = true;
            let label = format!("row `{}` (#{issue}): {}", row.claim_id, row.resolution);
            match row_cut_state(&row.resolution) {
                RowCutState::Terminal => predicate.saw(label),
                RowCutState::ExecutionOwed => predicate.saw(format!(
                    "{label}; proof is owed on the exact candidate under #2769"
                )),
                RowCutState::Pending => predicate.lower(State::Limited, label),
                RowCutState::Hold => predicate.lower(State::Failed, label),
                RowCutState::Unknown => predicate.lower(State::NotEstablished, label),
            }
        }
        if !matched {
            predicate.lower(
                State::NotEstablished,
                format!("no packet row is owned by #{issue}"),
            );
        }
    }
}

fn public_ingress(git: &Git, request: &Request) -> Result<Predicate, String> {
    let mut predicate = Predicate::new(
        1,
        "public_ingress",
        "The public ripr head is an ancestor of the candidate",
        &[],
    );
    let Some(public_head) = request.public_head else {
        predicate.lower(
            State::NotEstablished,
            "no --public-head was supplied; pass the current EffortlessMetrics/ripr main commit",
        );
        return Ok(predicate.finish());
    };
    if !git.has_commit(public_head)? {
        predicate.lower(
            State::Failed,
            format!(
                "public head {} is not a commit in this repository, so it was never imported",
                short(public_head)
            ),
        );
    } else if git.is_ancestor(public_head, request.candidate)? {
        predicate.saw(format!(
            "git merge-base --is-ancestor {} {}",
            short(public_head),
            short(request.candidate)
        ));
    } else {
        predicate.lower(
            State::Failed,
            format!(
                "public head {} is present but is not an ancestor of the candidate",
                short(public_head)
            ),
        );
    }
    Ok(predicate.finish())
}

fn selection_terminal(
    git: &Git,
    request: &Request,
    selection: Option<&SelectionProjection>,
) -> Result<Predicate, String> {
    let mut predicate = Predicate::new(
        2,
        "selection_terminal",
        "The selected-claim packet at the candidate is terminal and describes the candidate",
        &[2766],
    );
    let Some(selection) = selection else {
        predicate.lower(
            State::Failed,
            format!("{RELEASE_SELECTION_PATH} is absent at the candidate"),
        );
        return Ok(predicate.finish());
    };
    predicate.saw(format!(
        "{RELEASE_SELECTION_PATH} at the candidate: stage {}, {} rows",
        selection.stage,
        selection.rows.len()
    ));
    for violation in &selection.violations {
        predicate.lower(State::Failed, violation.clone());
    }
    if selection.stage != HARD_CUT_ELIGIBLE_STAGE {
        predicate.lower(
            State::Failed,
            format!(
                "stage is `{}`; the cut consumes only `{HARD_CUT_ELIGIBLE_STAGE}`",
                selection.stage
            ),
        );
    }

    // The packet cannot name the commit that contains it. It must name an
    // ancestor from which nothing but the packet itself has changed, or its
    // rows and open-PR roster describe a different tree.
    match selection.observed_main.as_deref() {
        None => predicate.lower(State::Failed, "observed_main.sha is null"),
        Some(observed) if !is_full_sha(observed) || !git.has_commit(observed)? => predicate.lower(
            State::Failed,
            format!(
                "observed_main {} is not a commit in this repository",
                short(observed)
            ),
        ),
        Some(observed) if !git.is_ancestor(observed, request.candidate)? => predicate.lower(
            State::Failed,
            format!(
                "observed_main {} is not an ancestor of the candidate",
                short(observed)
            ),
        ),
        Some(observed) => {
            let moved: Vec<String> = git
                .changed_paths(observed, request.candidate)?
                .into_iter()
                .filter(|path| path != RELEASE_SELECTION_PATH)
                .collect();
            if moved.is_empty() {
                predicate.saw(format!(
                    "only {RELEASE_SELECTION_PATH} differs between observed_main {} and the candidate",
                    short(observed)
                ));
            } else {
                predicate.lower(
                    State::Failed,
                    format!(
                        "the packet observed main at {}, and {} other path(s) changed before the candidate (first: {}); regenerate the packet at the candidate's parent",
                        short(observed),
                        moved.len(),
                        moved.first().map_or("", String::as_str)
                    ),
                );
            }
        }
    }

    // A row that says its work landed must cite something Git can find in the
    // candidate's own history.
    let landed_prs = git.landed_prs(request.candidate)?;
    let mut cited = 0usize;
    for row in selection
        .rows
        .iter()
        .filter(|row| row.resolution == "landed_and_accepted")
    {
        for reference in &row.implementation_refs {
            cited += 1;
            let landed = if let Some(number) = reference.strip_prefix("pr:") {
                number
                    .parse::<u64>()
                    .is_ok_and(|number| landed_prs.contains(&number))
            } else if let Some(sha) = reference.strip_prefix("commit:") {
                is_full_sha(sha)
                    && git.has_commit(sha)?
                    && git.is_ancestor(sha, request.candidate)?
            } else {
                false
            };
            if !landed {
                predicate.lower(
                    State::Failed,
                    format!(
                        "row `{}` is landed_and_accepted but `{reference}` is not in the candidate's history; cite the landing as `commit:<sha>` if its subject carries no PR number",
                        row.claim_id
                    ),
                );
            }
        }
    }
    predicate.saw(format!(
        "{cited} landing reference(s) on landed_and_accepted rows checked against candidate history"
    ));
    Ok(predicate.finish())
}

fn dependency_packet(
    git: &Git,
    request: &Request,
    selection: Option<&SelectionProjection>,
    runs: &[CheckRun],
    inputs: &mut Vec<Input>,
) -> Result<Predicate, String> {
    let mut predicate = Predicate::new(
        6,
        "dependency_packet",
        "The dependency and advisory packet holds on the candidate",
        &[15068, 3787, 3811],
    );
    rows_terminal(&mut predicate, selection, &[3787, 3811]);
    match git.blob(request.candidate, DENY_PATH)? {
        None => predicate.lower(
            State::Failed,
            format!("{DENY_PATH} is absent at the candidate"),
        ),
        Some((oid, bytes)) => {
            inputs.push(blob_input(request.candidate, DENY_PATH, &oid));
            let text = String::from_utf8_lossy(&bytes);
            match check_dependency_suppression_expiry_text(&text, request.today) {
                Ok(()) => predicate.saw(format!(
                    "{DENY_PATH} at the candidate: every dated advisory ignore expires after {}",
                    request.today
                )),
                Err(error) => predicate.lower(State::Failed, error),
            }
        }
    }
    deny_observation(&mut predicate, git, request, runs, inputs)?;
    Ok(predicate.finish())
}

fn freeze_activation(
    git: &Git,
    request: &Request,
    candidate_tree: &str,
    selection: Option<&SelectionProjection>,
    runs: &[CheckRun],
    inputs: &mut Vec<Input>,
) -> Result<Predicate, String> {
    let mut predicate = Predicate::new(
        8,
        "freeze_activation",
        "The hard-cut freeze is active on exactly the candidate",
        &[5391],
    );
    rows_terminal(&mut predicate, selection, &[5391]);
    let Some(tip) = request.control_tip else {
        predicate.lower(
            State::NotEstablished,
            "no --control-tip was supplied; pass the main commit that carries the activation packet",
        );
        return Ok(predicate.finish());
    };
    if !git.has_commit(tip)? {
        predicate.lower(
            State::Failed,
            format!(
                "control tip {} is not a commit in this repository",
                short(tip)
            ),
        );
        return Ok(predicate.finish());
    }
    if tip == request.candidate || !git.is_ancestor(request.candidate, tip)? {
        predicate.lower(
            State::Failed,
            "the control tip must be a descendant of the candidate; the activation packet lands after the cut",
        );
        return Ok(predicate.finish());
    }
    let moved: Vec<String> = git
        .changed_paths(request.candidate, tip)?
        .into_iter()
        .filter(|path| path != ADMISSION_PACKET_PATH)
        .collect();
    if moved.is_empty() {
        predicate.saw(format!(
            "only {ADMISSION_PACKET_PATH} differs between the candidate and control tip {}",
            short(tip)
        ));
    } else {
        predicate.lower(
            State::Failed,
            format!(
                "{} path(s) other than the admission packet changed after the candidate (first: {}); main moved past the cut",
                moved.len(),
                moved.first().map_or("", String::as_str)
            ),
        );
    }
    match git.blob(tip, ADMISSION_PACKET_PATH)? {
        None => predicate.lower(
            State::Failed,
            format!("{ADMISSION_PACKET_PATH} is absent at the control tip"),
        ),
        Some((oid, bytes)) => {
            inputs.push(blob_input(tip, ADMISSION_PACKET_PATH, &oid));
            let packet: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let field = |pointer: &str| packet.pointer(pointer).and_then(Value::as_str);
            let state = field("/state").unwrap_or("unreadable");
            predicate.saw(format!(
                "{ADMISSION_PACKET_PATH} at the control tip: state {state}, generation {}",
                field("/generation").unwrap_or("unreadable")
            ));
            if field("/release") != Some(RELEASE) {
                predicate.lower(
                    State::Failed,
                    format!("the admission packet is not for release {RELEASE}"),
                );
            }
            // `candidate_pinned` is the state after #1609, when ordinary merges
            // have resumed and the watch no longer judges main. Only `active`
            // is a freeze.
            if state != "active" {
                predicate.lower(
                    State::Failed,
                    format!("the admission packet state is `{state}`, not `active`"),
                );
            }
            if field("/observed_main/sha") != Some(request.candidate)
                || field("/observed_main/tree") != Some(candidate_tree)
            {
                predicate.lower(
                    State::Failed,
                    "the admission packet's observed_main is not the candidate's exact commit and tree",
                );
            }
        }
    }
    hosted(&mut predicate, runs, WATCH_CHECK, tip);
    Ok(predicate.finish())
}

fn blob_input(rev: &str, path: &str, oid: &str) -> Input {
    Input {
        kind: "git_blob",
        subject: format!("{rev}:{path}"),
        digest: format!("git:{oid}"),
    }
}

fn row_predicate(
    number: u8,
    id: &'static str,
    title: &'static str,
    issues: &[u64],
    selection: Option<&SelectionProjection>,
) -> Predicate {
    let mut predicate = Predicate::new(number, id, title, issues);
    rows_terminal(&mut predicate, selection, issues);
    predicate.finish()
}

pub(super) fn verdict(predicates: &[Predicate]) -> &'static str {
    let numbers: Vec<u8> = predicates
        .iter()
        .map(|predicate| predicate.number)
        .collect();
    if numbers == [1, 2, 3, 4, 5, 6, 7, 8]
        && predicates
            .iter()
            .all(|predicate| predicate.state == State::Pass)
    {
        "ready"
    } else {
        "not_ready"
    }
}

pub(super) fn evaluate(request: &Request) -> Result<Receipt, String> {
    require_full_sha("--candidate", request.candidate)?;
    if let Some(public_head) = request.public_head {
        require_full_sha("--public-head", public_head)?;
    }
    if let Some(tip) = request.control_tip {
        require_full_sha("--control-tip", tip)?;
    }
    let git = Git { repo: request.repo };
    if git.text(&["rev-parse", "--is-shallow-repository"])? == "true" {
        return Err(format!(
            "freeze-readiness needs full history to answer ancestry, but {} is a shallow repository; run `git fetch --unshallow` and retry",
            request.repo.display()
        ));
    }
    if !git.has_commit(request.candidate)? {
        return Err(format!(
            "--candidate {} is not a commit in {}; fetch it first",
            request.candidate,
            request.repo.display()
        ));
    }
    let tree_of = |sha: &str| git.text(&["rev-parse", &format!("{sha}^{{tree}}")]);
    let candidate_tree = tree_of(request.candidate)?;

    let mut inputs = Vec::new();
    let runs = load_check_runs(request.hosted_checks, &mut inputs)?;
    let selection = match git.blob(request.candidate, RELEASE_SELECTION_PATH)? {
        None => None,
        Some((oid, bytes)) => {
            inputs.push(blob_input(request.candidate, RELEASE_SELECTION_PATH, &oid));
            Some(project_release_selection(
                RELEASE_SELECTION_PATH,
                &String::from_utf8_lossy(&bytes),
            ))
        }
    };
    let selection = selection.as_ref();

    let predicates = vec![
        public_ingress(&git, request)?,
        selection_terminal(&git, request, selection)?,
        row_predicate(
            3,
            "schema_convergence",
            "Schema and producer convergence has a terminal row",
            &[3920],
            selection,
        ),
        row_predicate(
            4,
            "release_docs",
            "The documentation and changelog packet has a terminal row",
            &[3921],
            selection,
        ),
        row_predicate(
            5,
            "final_audit",
            "The final adversarial audit has a terminal row",
            &[3807],
            selection,
        ),
        dependency_packet(&git, request, selection, &runs, &mut inputs)?,
        row_predicate(
            7,
            "pre_cut_execution",
            "Windows, candidate lifecycle and Stage A journey have terminal rows",
            &[3922, 3924, 4508],
            selection,
        ),
        freeze_activation(
            &git,
            request,
            &candidate_tree,
            selection,
            &runs,
            &mut inputs,
        )?,
    ];
    let control_tip = match request.control_tip {
        Some(tip) if git.has_commit(tip)? => Some(CommitIdentity {
            sha: tip.to_string(),
            tree: tree_of(tip)?,
        }),
        _ => None,
    };
    Ok(Receipt {
        schema_version: "1",
        kind: "ripr_freeze_readiness_receipt",
        release: RELEASE,
        verdict: verdict(&predicates),
        candidate: CommitIdentity {
            sha: request.candidate.to_string(),
            tree: candidate_tree,
        },
        public_head: request.public_head.map(str::to_string),
        control_tip,
        evaluated_on: request.today.to_string(),
        predicates,
        inputs,
        non_claims: NON_CLAIMS,
    })
}

fn render_markdown(receipt: &Receipt) -> String {
    let mut out = format!(
        "# Freeze readiness\n\nVerdict: `{}`\n\nCandidate: `{}` (tree `{}`)\n",
        receipt.verdict, receipt.candidate.sha, receipt.candidate.tree
    );
    if let Some(public_head) = &receipt.public_head {
        out.push_str(&format!("Public head: `{public_head}`\n"));
    }
    if let Some(tip) = &receipt.control_tip {
        out.push_str(&format!("Control tip: `{}`\n", tip.sha));
    }
    out.push_str(&format!("Evaluated on: {}\n\n", receipt.evaluated_on));
    out.push_str("| # | Predicate | State |\n|---|---|---|\n");
    for predicate in &receipt.predicates {
        out.push_str(&format!(
            "| {} | {} | `{}` |\n",
            predicate.number,
            predicate.title,
            predicate.state.label()
        ));
    }
    for predicate in &receipt.predicates {
        out.push_str(&format!(
            "\n## {}. {} — `{}`\n\n",
            predicate.number,
            predicate.id,
            predicate.state.label()
        ));
        for finding in &predicate.findings {
            out.push_str(&format!("- Finding: {finding}\n"));
        }
        for evidence in &predicate.evidence {
            out.push_str(&format!("- Read: {evidence}\n"));
        }
    }
    out.push_str("\n## Inputs\n\n");
    for input in &receipt.inputs {
        out.push_str(&format!(
            "- {} `{}` `{}`\n",
            input.kind, input.subject, input.digest
        ));
    }
    out.push_str("\n## Not claimed\n\n");
    for non_claim in receipt.non_claims {
        out.push_str(&format!("- {non_claim}\n"));
    }
    out
}

struct Options {
    repo: PathBuf,
    candidate: String,
    public_head: Option<String>,
    control_tip: Option<String>,
    hosted_checks: Vec<PathBuf>,
}

fn usage() -> String {
    "usage: cargo xtask freeze-readiness --candidate <full-sha> [--public-head <full-sha>] [--control-tip <full-sha>] [--hosted-checks <check-runs.json>]... [--repo <dir>]".to_string()
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        repo: PathBuf::from("."),
        candidate: String::new(),
        public_head: None,
        control_tip: None,
        hosted_checks: Vec::new(),
    };
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let mut value = || {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("missing value for {flag}; {}", usage()))
        };
        match flag.as_str() {
            "--candidate" => options.candidate = value()?,
            "--public-head" => options.public_head = Some(value()?),
            "--control-tip" => options.control_tip = Some(value()?),
            "--hosted-checks" => options.hosted_checks.push(PathBuf::from(value()?)),
            "--repo" => options.repo = PathBuf::from(value()?),
            other => {
                return Err(format!(
                    "unknown freeze-readiness argument `{other}`; {}",
                    usage()
                ));
            }
        }
    }
    if options.candidate.is_empty() {
        return Err(usage());
    }
    Ok(options)
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let today = utc_date_string();
    let request = Request {
        repo: &options.repo,
        candidate: &options.candidate,
        public_head: options.public_head.as_deref(),
        control_tip: options.control_tip.as_deref(),
        hosted_checks: &options.hosted_checks,
        today: &today,
    };
    let receipt = match evaluate(&request) {
        Ok(receipt) => receipt,
        Err(error) => {
            // Replace both reports so an earlier receipt is never left standing
            // as the result of a run that could not evaluate.
            let body = serde_json::json!({
                "schema_version": "1",
                "kind": "ripr_freeze_readiness_receipt",
                "release": RELEASE,
                "verdict": "not_evaluated",
                "candidate": options.candidate,
                "error": error,
            });
            crate::write_report(JSON_NAME, &format!("{body:#}\n"))?;
            crate::write_report(
                MD_NAME,
                &format!("# Freeze readiness\n\nVerdict: `not_evaluated`\n\n{error}\n"),
            )?;
            return Err(error);
        }
    };
    let json = serde_json::to_string_pretty(&receipt)
        .map_err(|error| format!("failed to serialize the freeze-readiness receipt: {error}"))?;
    crate::write_report(JSON_NAME, &format!("{json}\n"))?;
    crate::write_report(MD_NAME, &render_markdown(&receipt))?;
    if receipt.verdict == "ready" {
        return Ok(());
    }
    let open: Vec<String> = receipt
        .predicates
        .iter()
        .filter(|predicate| predicate.state != State::Pass)
        .map(|predicate| {
            format!(
                "  {}. {} — {}: {}",
                predicate.number,
                predicate.id,
                predicate.state.label(),
                predicate.findings.first().map_or("", String::as_str)
            )
        })
        .collect();
    Err(format!(
        "freeze-readiness: not ready for {}\n{}\nreceipt: target/ripr/reports/{JSON_NAME}",
        options.candidate,
        open.join("\n")
    ))
}

#[cfg(test)]
mod tests;
