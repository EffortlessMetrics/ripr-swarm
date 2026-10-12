//! Independently authored real Rust capture -> ledger -> framed LSP proof.
//! Metadata assertions are isolated from the behavioral consumer controls.

use super::{
    ANALYSIS_TIMEOUT, CompatFixtureRoot, LspSession, compat_file_uri, execute_compat_command,
    expect_result, ripr_binary_probe, shutdown_exit_and_wait, unique_compat_fixture_root,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[path = "../common/fixture_git.rs"]
mod fixture_git;

const GAP: &str = "gap:54c07ec8eeadf203";
const SEAM: &str = "67f9714ba37b3967";
const SOURCE: &str = "pub fn discounted_total(amount: i32, threshold: i32) -> i32 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const PRICING: &str = "use ordinary_shop::discounted_total;\n#[test]\nfn below_threshold_has_no_discount() {\n    assert_eq!(discounted_total(50, 100), 50);\n}\n#[test]\nfn above_threshold_discounts() {\n    assert_eq!(discounted_total(1_000, 100), 990);\n}\n";
const CORPUS_CONTROL: &str = "#[test]\nfn unrelated_control() {\n    assert_eq!(2 + 2, 4);\n}\n";
const CORPUS_CONTROL_EDITED: &str =
    "#[test]\nfn unrelated_control() {\n    assert_eq!(3 + 3, 6);\n}\n";
const EQUALITY_TEST: &str = "use ordinary_shop::discounted_total;\n#[test]\nfn boundary_discounts() {\n    assert_eq!(discounted_total(100, 100), 90);\n}\n";

type ProofResult<T = ()> = Result<T, String>;

struct CorpusFixture {
    owned: CompatFixtureRoot,
}

impl CorpusFixture {
    fn root(&self) -> &Path {
        &self.owned.path
    }

    fn new(label: &str) -> ProofResult<Self> {
        let owned = unique_compat_fixture_root(label)?;
        let root = &owned.path;
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"ordinary_shop\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), SOURCE.replace(">=", ">"))
            .map_err(|error| error.to_string())?;
        std::fs::write(root.join("tests/pricing.rs"), PRICING)
            .map_err(|error| error.to_string())?;
        std::fs::write(root.join("tests/corpus_control.rs"), CORPUS_CONTROL)
            .map_err(|error| error.to_string())?;
        fixture_git::fixture_git_ok(root, &["init"])?;
        fixture_git::fixture_git_ok(root, &["config", "user.name", "RIPR proof"])?;
        fixture_git::fixture_git_ok(root, &["config", "user.email", "ripr@example.invalid"])?;
        std::fs::write(root.join(".git/info/exclude"), "/target/\n")
            .map_err(|error| error.to_string())?;
        fixture_git::fixture_git_ok(root, &["add", "Cargo.toml", "src", "tests"])?;
        fixture_git::fixture_git_ok(root, &["commit", "-m", "base pricing predicate"])?;
        std::fs::write(root.join("src/lib.rs"), SOURCE).map_err(|error| error.to_string())?;
        fixture_git::fixture_git_ok(root, &["add", "src/lib.rs"])?;
        fixture_git::fixture_git_ok(root, &["commit", "-m", "include equality boundary"])?;
        Ok(Self { owned })
    }
}

fn require(condition: bool, message: impl FnOnce() -> String) -> ProofResult {
    if condition { Ok(()) } else { Err(message()) }
}

fn read_json(path: &Path) -> ProofResult<Value> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> ProofResult {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn cli_json(root: &Path, args: &[&str]) -> ProofResult<Value> {
    let output = ripr_binary_probe(Some(root), args)?;
    require(output.status.success(), || {
        format!("real CLI {args:?} failed: {output:?}")
    })?;
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("real CLI {args:?} returned invalid JSON: {error}; {output:?}"))
}

fn capture(root: &Path) -> ProofResult<Value> {
    // Deliberately use the default root, as a developer does in the repository.
    let value = cli_json(root, &["check", "--format", "repo-exposure-json"])?;
    let seams = value["seams"]
        .as_array()
        .ok_or("real capture has no seams")?;
    require(seams.len() == 1 && seams[0]["seam_id"] == SEAM, || {
        format!("capture must retain the exact nonempty pricing subject: {value}")
    })?;
    require(
        seams[0]["file"] == "src/lib.rs"
            && seams[0]["line"] == 2
            && seams[0]["owner"] == "src/lib.rs::discounted_total",
        || format!("capture changed the selected source subject: {value}"),
    )?;
    Ok(value)
}

fn derive(root: &Path, captured: &Value, label: &str) -> ProofResult<Value> {
    let inputs = root.join("target/corpus-proof");
    std::fs::create_dir_all(&inputs).map_err(|error| error.to_string())?;
    let input = inputs.join(format!("{label}.repo-exposure.json"));
    let output = inputs.join(format!("{label}.gap-ledger.json"));
    write_json(&input, captured)?;
    let input_arg = input.to_str().ok_or("capture path is not UTF-8")?;
    let output_arg = output.to_str().ok_or("ledger path is not UTF-8")?;
    let produced = ripr_binary_probe(
        Some(root),
        &[
            "reports",
            "gap-ledger",
            "--repo-exposure",
            input_arg,
            "--out",
            output_arg,
        ],
    )?;
    require(produced.status.success(), || {
        format!("real ledger producer failed: {produced:?}")
    })?;
    let ledger = read_json(&output)?;
    let records = ledger["records"]
        .as_array()
        .ok_or("real ledger has no records")?;
    require(records.len() == 1 && records[0]["seam_id"] == SEAM, || {
        format!("ledger must retain one real typed pricing subject: {ledger}")
    })?;
    Ok(ledger)
}

fn install(root: &Path, ledger: &Value) -> ProofResult {
    let reports = root.join("target/ripr/reports");
    std::fs::create_dir_all(&reports).map_err(|error| error.to_string())?;
    write_json(&reports.join("gap-decision-ledger.json"), ledger)
}

fn head(root: &Path) -> ProofResult<String> {
    fixture_git::fixture_git_output(root, &["rev-parse", "HEAD"])
        .map(|head| head.trim().to_string())
}

fn assert_original_named_bytes(root: &Path, ledger: &Value) -> ProofResult {
    require(
        std::fs::read(root.join("src/lib.rs")).map_err(|error| error.to_string())?
            == SOURCE.as_bytes(),
        || "pricing production bytes changed during corpus stimulus".into(),
    )?;
    require(
        std::fs::read(root.join("tests/pricing.rs")).map_err(|error| error.to_string())?
            == PRICING.as_bytes(),
        || "old named pricing-test bytes changed during corpus stimulus".into(),
    )?;
    let files = ledger["source_subject"]["files"]
        .as_array()
        .ok_or("real original ledger lacks named-file source_subject")?;
    require(files.len() == 2, || {
        format!("fixture must isolate two old named subjects: {files:?}")
    })?;
    for (path, bytes) in [
        ("src/lib.rs", SOURCE.as_bytes()),
        ("tests/pricing.rs", PRICING.as_bytes()),
    ] {
        let expected = format!("sha256:{:x}", Sha256::digest(bytes));
        require(
            files
                .iter()
                .any(|file| file["path"] == path && file["digest"] == expected),
            || {
                format!(
                    "real original ledger did not stamp expected named bytes for {path}: {files:?}"
                )
            },
        )?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn consumer_root(root: &Path) -> ProofResult<PathBuf> {
    Ok(root.to_path_buf())
}

#[cfg(windows)]
fn consumer_root(root: &Path) -> ProofResult<PathBuf> {
    use std::path::{Component, Prefix};
    let mut components = root.components();
    let drive = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            _ => return Err("proof root requires an ordinary local drive".into()),
        },
        _ => return Err("proof root requires a native absolute drive path".into()),
    };
    require(
        matches!(components.next(), Some(Component::RootDir)),
        || "proof root requires a drive-absolute path".into(),
    )?;
    let mut ordinary = PathBuf::from(format!("{}:\\", char::from(drive)));
    ordinary.extend(components.map(|component| component.as_os_str()));
    require(
        std::fs::canonicalize(root).map_err(|error| error.to_string())?
            == std::fs::canonicalize(&ordinary).map_err(|error| error.to_string())?,
        || "ordinary consumer spelling must resolve to the original local fixture".into(),
    )?;
    Ok(ordinary)
}

fn framed_status(root: &Path) -> ProofResult<Value> {
    let selected = consumer_root(root)?;
    let uri = compat_file_uri(&selected)?;
    let mut session = LspSession::spawn()?;
    let initialized = session.request(
        "initialize",
        json!({
            "processId": null,
            "rootUri": uri,
            "initializationOptions": {
                "baseRef": "HEAD", "checkMode": "instant", "diagnosticProfile": "full"
            },
            "capabilities": {}
        }),
    )?;
    expect_result(&initialized, "initialize")?;
    session.notify("initialized", Some(json!({})))?;
    let refresh = session.request_with_timeout(
        "workspace/executeCommand",
        json!({"command": "ripr.refresh", "arguments": []}),
        ANALYSIS_TIMEOUT,
    )?;
    require(expect_result(&refresh, "ripr.refresh")?.is_null(), || {
        format!("explicit refresh response must be null: {refresh}")
    })?;
    let status = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
    shutdown_exit_and_wait(&mut session)?;
    require(
        status["kind"] == "workspace_status"
            && status["analysis_status"]["state"] == "succeeded"
            && status["analysis_status"]["reason"] == "explicit_refresh",
        || format!("the real protocol must reach an executed committed snapshot: {status}"),
    )?;
    let components = status["analysis_status"]["components"]
        .as_array()
        .ok_or("protocol status omitted component outcomes")?;
    require(
        components.iter().any(|component| {
            component["component"] == "seam_inventory" && component["state"] == "complete"
        }),
        || format!("proof requires complete real seam inventory, not deferred work: {status}"),
    )?;
    Ok(status)
}

fn assert_current_action(status: &Value) -> ProofResult {
    require(
        status["run_status"] == "full"
            && status["diagnostics"]["gap_artifacts"] == 1
            && status["diagnostics"]["gap_artifact_rejections"] == 0
            && status["diagnostics"]["actionable_gap_artifacts"] == 1
            && status["diagnostics"]["seam_diagnostics"] == 1
            && status["top_actionable_packet"]["canonical_gap_id"] == GAP
            && status["top_actionable_packet"]["repair_kind"] == "actionable",
        || format!("unchanged corpus must retain the exact nonempty useful action: {status}"),
    )?;
    let selected = status["diagnostic_budget"]["selected"]
        .as_array()
        .ok_or("current action omitted its actual selected diagnostic evidence")?;
    require(
        selected.iter().any(|item| item["canonical_id"] == GAP),
        || format!("nonempty positive must select the genuine ledger gap: {status}"),
    )
}

fn assert_current_no_action(status: &Value) -> ProofResult {
    require(
        status["run_status"] == "full"
            && status["diagnostics"]["gap_artifacts"] == 1
            && status["diagnostics"]["gap_artifact_rejections"] == 0
            && status["diagnostics"]["actionable_gap_artifacts"] == 0
            && status["diagnostics"]["seam_diagnostics"] == 0
            && status["top_actionable_packet"].is_null(),
        || {
            format!(
                "fresh already-observed record must survive admission without an obsolete action: {status}"
            )
        },
    )
}

fn assert_capture_is_actionable(captured: &Value, ledger: &Value) -> ProofResult {
    require(
        captured["seams"][0]["grip_class"] == "weakly_gripped"
            && captured["seams"][0]["evidence_record"]["canonical_gap_id"] == GAP
            && ledger["records"][0]["canonical_gap_id"] == GAP
            && ledger["records"][0]["gap_state"] == "actionable"
            && ledger["summary"]["records_total"] == 1
            && ledger["summary"]["repairable_total"] == 1,
        || {
            format!(
                "real initial producers must contain the precise actionable pricing gap: {captured}; {ledger}"
            )
        },
    )
}

#[test]
fn passive_corpus_accepts_empty_commit_and_identical_input_branch() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-same-input-revisions")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let ledger = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &ledger)?;
    assert_original_named_bytes(root, &ledger)?;
    install(root, &ledger)?;
    assert_current_action(&framed_status(root)?)?;
    let original_head = head(root)?;

    fixture_git::fixture_git_ok(root, &["commit", "--allow-empty", "-m", "empty revision"])?;
    let empty_head = head(root)?;
    require(empty_head != original_head, || {
        "empty commit did not change actual HEAD".into()
    })?;
    assert_original_named_bytes(root, &ledger)?;
    require(
        std::fs::read(root.join("tests/corpus_control.rs")).map_err(|error| error.to_string())?
            == CORPUS_CONTROL.as_bytes(),
        || "empty commit changed corpus control bytes".into(),
    )?;
    assert_current_action(&framed_status(root)?)?;

    fixture_git::fixture_git_ok(root, &["checkout", "-b", "same-inputs", &original_head])?;
    fixture_git::fixture_git_ok(
        root,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "same-input branch revision",
        ],
    )?;
    let branch_head = head(root)?;
    require(
        branch_head != original_head && branch_head != empty_head,
        || "same-input branch must have its own observed revision".into(),
    )?;
    assert_original_named_bytes(root, &ledger)?;
    require(
        std::fs::read(root.join("tests/corpus_control.rs")).map_err(|error| error.to_string())?
            == CORPUS_CONTROL.as_bytes(),
        || "branch switch changed corpus control bytes".into(),
    )?;
    assert_current_action(&framed_status(root)?)
}

#[test]
fn passive_corpus_accepts_same_head_dirty_non_corpus_input() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-dirty-non-input")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let ledger = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &ledger)?;
    install(root, &ledger)?;
    assert_current_action(&framed_status(root)?)?;
    let original_head = head(root)?;
    std::fs::write(
        root.join("notes.txt"),
        "ordinary developer notes outside Rust analysis\n",
    )
    .map_err(|error| error.to_string())?;
    let dirty =
        fixture_git::fixture_git_output(root, &["status", "--porcelain", "--untracked-files=all"])?;
    require(dirty.lines().any(|line| line == "?? notes.txt"), || {
        format!(
            "positive control must genuinely dirty Git without changing the Rust corpus: {dirty}"
        )
    })?;
    require(head(root)? == original_head, || {
        "non-corpus positive changed actual HEAD".into()
    })?;
    assert_original_named_bytes(root, &ledger)?;
    assert_current_action(&framed_status(root)?)
}

fn selected_rerun(root: &Path, ledger: &Value, label: &str) -> ProofResult<Value> {
    let path = root
        .join("target/corpus-proof")
        .join(format!("{label}.selector-ledger.json"));
    write_json(&path, ledger)?;
    let ledger_arg = path.to_str().ok_or("selector ledger path is not UTF-8")?;
    let selected = cli_json(
        root,
        &["rerun", "--gap", GAP, "--gap-ledger", ledger_arg, "--json"],
    )?;
    require(
        selected["state"] == "current_state_only"
            && selected["selector"]["canonical_gap_id"] == GAP
            && selected["selector"]["matched_record_count"] == 1
            && selected["selector"]["recomputed_scope_count"] == 1,
        || {
            format!(
                "absolute-root historical selector must recompute its real selected scope: {selected}"
            )
        },
    )?;
    let seams = selected["seams"]
        .as_array()
        .ok_or("active selector omitted seams")?;
    require(seams.len() == 1 && seams[0]["seam_id"] == SEAM, || {
        format!("active selector must retain the exact nonempty typed seam: {selected}")
    })?;
    Ok(selected)
}

#[derive(Clone, Copy)]
enum CorpusMutation {
    AddEqualityTest,
    EditUnclaimedTest,
    DeleteUnclaimedTest,
}

impl CorpusMutation {
    fn label(self) -> &'static str {
        match self {
            Self::AddEqualityTest => "added-test",
            Self::EditUnclaimedTest => "changed-test",
            Self::DeleteUnclaimedTest => "deleted-test",
        }
    }

    fn apply(self, root: &Path) -> ProofResult {
        match self {
            Self::AddEqualityTest => {
                std::fs::write(root.join("tests/new_boundary.rs"), EQUALITY_TEST)
            }
            Self::EditUnclaimedTest => {
                std::fs::write(root.join("tests/corpus_control.rs"), CORPUS_CONTROL_EDITED)
            }
            Self::DeleteUnclaimedTest => std::fs::remove_file(root.join("tests/corpus_control.rs")),
        }
        .map_err(|error| error.to_string())
    }
}

fn assert_corpus_refused(status: &Value, kind: &str, reason: Option<&str>) -> ProofResult {
    require(
        status["diagnostics"]["gap_artifacts"] == 0
            && status["diagnostics"]["gap_artifact_rejections"] == 1
            && status["diagnostics"]["actionable_gap_artifacts"] == 0
            && status["diagnostics"]["actionable_diagnostics"] == 0
            && status["top_actionable_packet"].is_null(),
        || format!("corpus refusal must withhold obsolete artifact actions: {status}"),
    )?;
    if kind == "stale_corpus" {
        require(
            status["run_status"] == "stale"
                && status["top_limitation"]["category"]
                    .as_str()
                    .is_some_and(|category| category != "no_active_limitation_in_current_scope"),
            || format!("corpus mismatch must disclose stale currentness: {status}"),
        )?;
    } else {
        require(
            status["run_status"] == "cache_limited"
                && status["top_limitation"]["category"] == kind
                && status["top_limitation"]["repair_route"] == "regenerate_gap_artifacts",
            || format!("unverifiable corpus must name {kind} and regeneration: {status}"),
        )?;
    }
    let components = status["analysis_status"]["components"]
        .as_array()
        .ok_or("corpus refusal omitted component outcomes")?;
    for name in ["cache", "gap_ledger"] {
        let component = components
            .iter()
            .find(|component| component["component"] == name)
            .ok_or_else(|| format!("corpus refusal omitted {name} outcome: {status}"))?;
        require(
            component["state"] == "limited" && component["kind"] == kind,
            || format!("{name} must disclose the named corpus limitation: {component}"),
        )?;
        require(
            component["recovery"].as_str().is_some_and(|recovery| {
                recovery.contains("regenerate") && recovery.contains("gap-ledger")
            }),
            || format!("{name} must provide executable ledger regeneration guidance: {component}"),
        )?;
        if let Some(reason) = reason {
            require(
                component["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(reason)),
                || {
                    format!(
                        "{name} must preserve exact corpus refusal reason {reason}: {component}"
                    )
                },
            )?;
        }
    }
    for collection in ["selected", "omitted"] {
        let items = status["diagnostic_budget"][collection]
            .as_array()
            .ok_or_else(|| {
                format!("corpus refusal omitted actual {collection} diagnostic evidence")
            })?;
        require(
            !items.iter().any(|item| item["canonical_id"] == GAP),
            || {
                format!(
                    "obsolete ledger gap must not remain {collection} behind zero action counters: {status}"
                )
            },
        )?;
    }
    Ok(())
}

fn corpus_mutation_contract(mutation: CorpusMutation, stale_kind: &str) -> ProofResult {
    let fixture = CorpusFixture::new(mutation.label())?;
    let root = fixture.root();
    let captured = capture(root)?;
    let original = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &original)?;
    assert_original_named_bytes(root, &original)?;
    install(root, &original)?;
    assert_current_action(&framed_status(root)?)?;
    let initial_head = head(root)?;
    let original_bytes = std::fs::read(root.join("target/ripr/reports/gap-decision-ledger.json"))
        .map_err(|error| error.to_string())?;

    mutation.apply(root)?;
    require(head(root)? == initial_head, || {
        "corpus stimulus must leave actual HEAD unchanged".into()
    })?;
    assert_original_named_bytes(root, &original)?;
    require(
        std::fs::read(root.join("target/ripr/reports/gap-decision-ledger.json"))
            .map_err(|error| error.to_string())?
            == original_bytes,
        || "corpus stimulus must leave the installed old ledger byte-for-byte unchanged".into(),
    )?;

    // This active selector is intentionally allowed to recompute current
    // tests from an absolute-root historical artifact. No HEAD gate or
    // passive freshness condition may erase the still-useful selector.
    let current = selected_rerun(root, &original, "historical")?;
    let expected_class = match mutation {
        CorpusMutation::AddEqualityTest => "strongly_gripped",
        CorpusMutation::EditUnclaimedTest | CorpusMutation::DeleteUnclaimedTest => "weakly_gripped",
    };
    require(
        current["seams"][0]["static_class"] == expected_class,
        || format!("active selector must recompute the actual corpus classification: {current}"),
    )?;
    let refusal = framed_status(root)?;
    assert_corpus_refused(&refusal, stale_kind, None)?;

    // Recovery recaptures analysis first, then derives a ledger. It never
    // edits old classifications, drops the subject, or mints a validator
    // stamp merely to make the passive projection accept old records.
    let fresh_capture = capture(root)?;
    let fresh = derive(root, &fresh_capture, "fresh")?;
    require(
        fresh_capture["seams"][0]["grip_class"] == expected_class,
        || format!("fresh producer must classify the retained exact seam: {fresh_capture}"),
    )?;
    install(root, &fresh)?;
    let recovered = framed_status(root)?;
    match mutation {
        CorpusMutation::AddEqualityTest => {
            require(
                fresh["summary"]["records_total"] == 1
                    && fresh["summary"]["repairable_total"] == 0
                    && fresh["summary"]["no_action_total"] == 1
                    && fresh["records"][0]["gap_state"] == "already_observed",
                || {
                    format!(
                        "real equality-test regeneration must retain a nonempty no-action record: {fresh}"
                    )
                },
            )?;
            assert_current_no_action(&recovered)
        }
        CorpusMutation::EditUnclaimedTest | CorpusMutation::DeleteUnclaimedTest => {
            assert_capture_is_actionable(&fresh_capture, &fresh)?;
            assert_current_action(&recovered)
        }
    }
}

#[test]
fn passive_corpus_rejects_added_test_at_same_head_and_recovers() -> ProofResult {
    corpus_mutation_contract(CorpusMutation::AddEqualityTest, "stale_corpus")
}

#[test]
fn passive_corpus_rejects_changed_unclaimed_test_at_same_head_and_recovers() -> ProofResult {
    corpus_mutation_contract(CorpusMutation::EditUnclaimedTest, "stale_corpus")
}

#[test]
fn passive_corpus_rejects_deleted_unclaimed_test_at_same_head_and_recovers() -> ProofResult {
    corpus_mutation_contract(CorpusMutation::DeleteUnclaimedTest, "stale_corpus")
}

fn captured_corpus(captured: &Value) -> ProofResult<&Value> {
    let corpus = captured["source_subject"]
        .get("corpus")
        .ok_or("analysis producer omitted source_subject.corpus")?;
    require(
        corpus["scope"] == "rust_workspace"
            && corpus["cache_schema_version"] == "1.62"
            && corpus["files_content_hash"].as_str().is_some_and(|hash| {
                hash.len() == 16
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }),
        || format!("analysis must stamp the typed Rust corpus authority: {corpus}"),
    )?;
    Ok(corpus)
}

fn cache_corpus_authority(root: &Path, corpus: &Value) -> ProofResult {
    // Inspect the actual cache receipt produced by check in this fresh fixture.
    // Do not implement another aggregate hash, use a HEAD token, or call the
    // production decision helper to compute the expected public stamp.
    let mut pending = vec![root.join("target/ripr/cache/repo-seam-facts")];
    let mut receipts = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .map_err(|error| format!("read actual corpus cache {}: {error}", directory.display()))?
        {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                receipts.push(read_json(&path)?);
            }
        }
    }
    require(
        receipts.iter().any(|receipt| {
            receipt["schema_version"] == corpus["cache_schema_version"]
                && receipt["files_content_hash"] == corpus["files_content_hash"]
                && receipt["classified_seams"]
                    .as_array()
                    .is_some_and(|seams| seams.len() == 1)
        }),
        || {
            format!(
                "capture corpus must use the real typed classified-inventory cache authority: {corpus}; {receipts:?}"
            )
        },
    )
}

#[test]
fn analysis_and_ledger_share_the_real_rust_corpus_content_authority() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-producer-authority")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let corpus = captured_corpus(&captured)?;
    cache_corpus_authority(root, corpus)?;
    let warm = capture(root)?;
    require(captured_corpus(&warm)? == corpus, || {
        format!(
            "warm real capture must retain the existing unchanged corpus authority: {captured}; {warm}"
        )
    })?;
    let ledger = derive(root, &captured, "captured")?;
    assert_capture_is_actionable(&captured, &ledger)?;
    assert_original_named_bytes(root, &ledger)?;
    require(ledger["source_subject"]["corpus"] == *corpus, || {
        format!(
            "ledger derivation must copy the existing capture corpus envelope exactly: {captured}; {ledger}"
        )
    })
}

#[derive(Clone, Copy)]
enum InvalidCorpus {
    Missing,
    Malformed,
    MissingHash,
    UppercaseHash,
    UnsupportedScope,
    UnsupportedSchema,
}

impl InvalidCorpus {
    fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Malformed => "malformed",
            Self::MissingHash => "incomplete",
            Self::UppercaseHash => "malformed-hash",
            Self::UnsupportedScope => "unsupported-scope",
            Self::UnsupportedSchema => "unsupported-schema",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Missing => "source_subject_corpus_missing",
            Self::Malformed | Self::MissingHash | Self::UppercaseHash => {
                "source_subject_corpus_malformed"
            }
            Self::UnsupportedScope => "source_subject_corpus_unsupported_scope",
            Self::UnsupportedSchema => "source_subject_corpus_unsupported_schema",
        }
    }

    fn capture(self, captured: &Value) -> ProofResult<Value> {
        let mut legacy = captured.clone();
        let subject = legacy
            .get_mut("source_subject")
            .and_then(Value::as_object_mut)
            .ok_or("real capture omitted its named-file stamp")?;
        match self {
            Self::Missing => {
                subject.remove("corpus");
            }
            Self::Malformed => {
                subject.insert("corpus".into(), json!("not a corpus envelope"));
            }
            variant => {
                let corpus = subject
                    .get_mut("corpus")
                    .and_then(Value::as_object_mut)
                    .ok_or(
                        "producer must supply real corpus metadata before corruption controls",
                    )?;
                match variant {
                    Self::MissingHash => {
                        corpus.remove("files_content_hash");
                    }
                    Self::UppercaseHash => {
                        corpus.insert("files_content_hash".into(), json!("ABCDEF0123456789"));
                    }
                    Self::UnsupportedScope => {
                        corpus.insert("scope".into(), json!("unrecognized_workspace"));
                    }
                    Self::UnsupportedSchema => {
                        corpus.insert(
                            "cache_schema_version".into(),
                            json!("unsupported-cache-schema"),
                        );
                    }
                    Self::Missing | Self::Malformed => {
                        return Err("unreachable corpus corruption variant".into());
                    }
                }
            }
        }
        Ok(legacy)
    }
}

#[test]
fn passive_legacy_corpus_requires_regeneration_without_artifact_actions() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-legacy-provenance")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let original = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &original)?;
    install(root, &original)?;
    assert_current_action(&framed_status(root)?)?;
    let original_head = head(root)?;
    for variant in [
        InvalidCorpus::Missing,
        InvalidCorpus::Malformed,
        InvalidCorpus::MissingHash,
        InvalidCorpus::UppercaseHash,
        InvalidCorpus::UnsupportedScope,
        InvalidCorpus::UnsupportedSchema,
    ] {
        // Change provenance alone in a real capture. The report writer must
        // retain that unavailable provenance, never bless it with current
        // workspace bytes during report derivation.
        let legacy_capture = variant.capture(&captured)?;
        let legacy = derive(root, &legacy_capture, variant.label())?;
        require(legacy["records"] == original["records"], || {
            format!("negative metadata control must preserve all real gap records: {legacy}")
        })?;
        require(
            legacy["source_subject"]["corpus"] == legacy_capture["source_subject"]["corpus"],
            || {
                format!(
                    "writer must preserve invalid/legacy capture corpus provenance rather than mint current metadata: {legacy_capture}; {legacy}"
                )
            },
        )?;
        assert_original_named_bytes(root, &legacy)?;
        require(head(root)? == original_head, || {
            "legacy metadata control changed actual HEAD".into()
        })?;
        install(root, &legacy)?;
        assert_corpus_refused(
            &framed_status(root)?,
            "unverifiable_subject",
            Some(variant.reason()),
        )?;
        let selected = selected_rerun(root, &legacy, variant.label())?;
        require(
            selected["seams"][0]["static_class"] == "weakly_gripped",
            || {
                format!(
                    "absolute-root active selector must remain useful with unavailable passive corpus provenance: {selected}"
                )
            },
        )?;
    }
    let fresh_capture = capture(root)?;
    let fresh = derive(root, &fresh_capture, "regenerated")?;
    install(root, &fresh)?;
    assert_current_action(&framed_status(root)?)
}

#[test]
fn delayed_ledger_derivation_copies_the_analysis_corpus_before_workspace_edit() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-delayed-derivation")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let captured_subject = captured_corpus(&captured)?.clone();
    cache_corpus_authority(root, &captured_subject)?;
    let initial_head = head(root)?;
    CorpusMutation::EditUnclaimedTest.apply(root)?;
    require(head(root)? == initial_head, || {
        "delayed derivation stimulus changed actual HEAD".into()
    })?;

    let delayed = derive(root, &captured, "delayed")?;
    assert_capture_is_actionable(&captured, &delayed)?;
    assert_original_named_bytes(root, &delayed)?;
    require(
        delayed["source_subject"]["corpus"] == captured_subject,
        || {
            format!(
                "delayed writer must copy captured corpus bytes, not mint current provenance: {captured}; {delayed}"
            )
        },
    )?;
    let fresh_capture = capture(root)?;
    let fresh_subject = captured_corpus(&fresh_capture)?;
    cache_corpus_authority(root, fresh_subject)?;
    require(
        fresh_subject["files_content_hash"] != captured_subject["files_content_hash"],
        || {
            format!(
                "editing a real unclaimed test must change the existing content authority at the same HEAD: {captured_subject}; {fresh_subject}"
            )
        },
    )?;
    install(root, &delayed)?;
    assert_corpus_refused(&framed_status(root)?, "stale_corpus", None)?;
    let fresh = derive(root, &fresh_capture, "fresh")?;
    install(root, &fresh)?;
    assert_current_action(&framed_status(root)?)
}

#[test]
fn active_absolute_legacy_gap_selector_remains_useful_after_corpus_edits() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-active-legacy-selector")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let original = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &original)?;
    let mut legacy = original.clone();
    legacy
        .as_object_mut()
        .ok_or("ledger is not an object")?
        .remove("source_subject");
    let original_head = head(root)?;
    let before = selected_rerun(root, &legacy, "legacy-before")?;
    require(
        before["seams"][0]["static_class"] == "weakly_gripped",
        || {
            format!(
                "unstamped absolute-root legacy selector must initially retain its weak seam: {before}"
            )
        },
    )?;
    CorpusMutation::AddEqualityTest.apply(root)?;
    assert_original_named_bytes(root, &original)?;
    require(head(root)? == original_head, || {
        "active legacy edit control changed actual HEAD".into()
    })?;
    let after = selected_rerun(root, &legacy, "legacy-after")?;
    require(
        after["seams"][0]["static_class"] == "strongly_gripped"
            && after["seams"][0]["repair_route_readiness"]["state"] == "already_gripped",
        || {
            format!(
                "unstamped absolute-root historical selector must recompute the closed equality gap: {after}"
            )
        },
    )
}

#[test]
fn passive_corpus_cannot_bypass_rust_currentness_with_missing_or_unknown_language() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-unproven-language-scope")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let original = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &original)?;
    install(root, &original)?;
    assert_current_action(&framed_status(root)?)?;
    let original_head = head(root)?;
    for (label, language) in [
        ("removed-language", None),
        ("unknown-language", Some("unknown_language")),
    ] {
        let mut ambiguous = original.clone();
        ambiguous
            .as_object_mut()
            .ok_or("ledger must be an object")?
            .remove("language");
        let records = ambiguous["records"]
            .as_array_mut()
            .ok_or("ledger must have real records")?;
        require(records.len() == 1, || {
            "scope control requires one genuine pricing record".into()
        })?;
        let record = records[0]
            .as_object_mut()
            .ok_or("real record must be an object")?;
        match language {
            Some(language) => {
                record.insert("language".into(), json!(language));
            }
            None => {
                record.remove("language");
            }
        }
        // Retain the real stamp, root, identity, anchor and complete command
        // payload. Neither a Rust-looking seam ID/file nor a matching Rust
        // corpus envelope may supply missing language authority.
        require(
            ambiguous["source_subject"] == original["source_subject"]
                && ambiguous["root"] == original["root"]
                && ambiguous["records"][0]["seam_id"] == SEAM
                && ambiguous["records"][0]["canonical_gap_id"] == GAP,
            || format!("scope negative must alter only language provenance: {ambiguous}"),
        )?;
        assert_original_named_bytes(root, &ambiguous)?;
        require(head(root)? == original_head, || {
            "language scope control changed actual HEAD".into()
        })?;
        install(root, &ambiguous)?;
        assert_corpus_refused(
            &framed_status(root)?,
            "unverifiable_subject",
            Some("source_subject_corpus_scope_missing"),
        )
        .map_err(|error| format!("{label}: {error}"))?;
    }
    install(root, &original)?;
    assert_current_action(&framed_status(root)?)
}

#[cfg(feature = "lang-python")]
fn real_preview_and_mixed_scope_ledgers(root: &Path) -> ProofResult<(Value, Value, Value)> {
    // Python is a genuine check-output producer. Mixed records are a
    // test-authored compound input, projected by the actual --records
    // writer using only records and named-file digests copied from the
    // two real producers. This is scope/admission proof, not qualification
    // of a single mixed-analysis producer capability.
    std::fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"rust\", \"python\"]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"pricing-preview\"\nversion = \"0.0.0\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/pricing.py"),
        "def calculate_discount(amount, threshold):\n    if amount > threshold:\n        return amount - 10\n    return amount\n")
        .map_err(|error| error.to_string())?;
    std::fs::write(root.join("tests/test_pricing.py"),
        "from src.pricing import calculate_discount\n\n\ndef test_calculate_discount_smoke():\n    result = calculate_discount(125, 100)\n    assert result\n")
        .map_err(|error| error.to_string())?;
    fixture_git::fixture_git_ok(
        root,
        &[
            "add",
            "ripr.toml",
            "pyproject.toml",
            "src/pricing.py",
            "tests/test_pricing.py",
        ],
    )?;
    fixture_git::fixture_git_ok(root, &["commit", "-m", "Python pricing base"])?;
    let python_base = head(root)?;
    std::fs::write(root.join("src/pricing.py"),
        "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n")
        .map_err(|error| error.to_string())?;
    fixture_git::fixture_git_ok(root, &["add", "src/pricing.py"])?;
    fixture_git::fixture_git_ok(root, &["commit", "-m", "include Python equality boundary"])?;

    let rust_capture = capture(root)?;
    let rust_corpus = captured_corpus(&rust_capture)?.clone();
    cache_corpus_authority(root, &rust_corpus)?;
    let rust_ledger = derive(root, &rust_capture, "scope-rust")?;
    assert_capture_is_actionable(&rust_capture, &rust_ledger)?;
    let inputs = root.join("target/corpus-proof");
    std::fs::create_dir_all(&inputs).map_err(|error| error.to_string())?;
    let python_diff = fixture_git::fixture_git_output(
        root,
        &["diff", &python_base, "HEAD", "--", "src/pricing.py"],
    )?;
    require(python_diff.contains("+    if amount >= threshold:"), || {
        format!(
            "Python-only producer stimulus must contain the real changed boundary: {python_diff}"
        )
    })?;
    let diff_path = inputs.join("python.diff");
    std::fs::write(&diff_path, python_diff).map_err(|error| error.to_string())?;
    let python_capture = cli_json(
        root,
        &[
            "check",
            "--diff",
            diff_path.to_str().ok_or("diff path is not UTF-8")?,
            "--json",
        ],
    )?;
    let python_ledger = derive_check_scope_ledger(root, &python_capture, "python-preview")?;
    let python_records = python_ledger["records"]
        .as_array()
        .ok_or("Python producer omitted records")?;
    require(
        !python_records.is_empty()
            && python_records.iter().all(|record| {
                record["language"] == "python" && record["language_status"] == "preview"
            }),
        || {
            format!(
                "real Python-only diff must produce nonempty typed Python preview records: {python_ledger}"
            )
        },
    )?;
    let mixed_ledger = project_copied_mixed_records(root, &rust_ledger, &python_ledger)?;
    Ok((rust_corpus, python_ledger, mixed_ledger))
}

#[cfg(feature = "lang-python")]
fn derive_check_scope_ledger(root: &Path, captured: &Value, label: &str) -> ProofResult<Value> {
    let inputs = root.join("target/corpus-proof");
    let input = inputs.join(format!("{label}.check.json"));
    let output = inputs.join(format!("{label}.gap-ledger.json"));
    write_json(&input, captured)?;
    let produced = ripr_binary_probe(
        Some(root),
        &[
            "reports",
            "gap-ledger",
            "--check-output",
            input.to_str().ok_or("check path is not UTF-8")?,
            "--out",
            output.to_str().ok_or("ledger path is not UTF-8")?,
        ],
    )?;
    require(produced.status.success(), || {
        format!("real check-output ledger producer failed: {produced:?}")
    })?;
    let ledger = read_json(&output)?;
    let records = ledger["records"]
        .as_array()
        .ok_or("real check-output ledger omitted records")?;
    require(!records.is_empty(), || {
        format!("scope proof cannot use a zero-subject producer: {captured}; {ledger}")
    })?;
    require(
        records.iter().all(|record| {
            record["canonical_gap_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        }),
        || format!("real scope records must have nonempty canonical identities: {ledger}"),
    )?;
    require(
        ledger["source_subject"]["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty()),
        || format!("known-scope fixture requires actual nonempty named-file provenance: {ledger}"),
    )?;
    Ok(ledger)
}

#[cfg(feature = "lang-python")]
fn project_copied_mixed_records(root: &Path, rust: &Value, python: &Value) -> ProofResult<Value> {
    require(rust["root"] == python["root"], || {
        format!(
            "compound input must copy producer records from the same actual canonical root: {rust}; {python}"
        )
    })?;
    let mut records = rust["records"]
        .as_array()
        .ok_or("Rust ledger omitted records")?
        .clone();
    records.extend(
        python["records"]
            .as_array()
            .ok_or("Python ledger omitted records")?
            .iter()
            .cloned(),
    );
    let mut files = std::collections::BTreeMap::<String, Value>::new();
    for ledger in [rust, python] {
        require(
            ledger["source_subject"]["digest_algorithm"] == "sha256",
            || {
                format!(
                    "compound input requires the actual existing named-file hash type: {ledger}"
                )
            },
        )?;
        for file in ledger["source_subject"]["files"]
            .as_array()
            .ok_or("real ledger omitted named-file digests")?
        {
            let path = file["path"]
                .as_str()
                .ok_or("real named subject omitted path")?
                .to_string();
            if let Some(previous) = files.insert(path.clone(), file.clone()) {
                require(previous == *file, || {
                    format!("same-root producers disagree on copied subject {path}")
                })?;
            }
        }
    }
    let compound = json!({
        "root": rust["root"],
        "records": records.clone(),
        "source_subject": {
            "digest_algorithm": "sha256",
            "files": files.into_values().collect::<Vec<_>>(),
            "corpus": rust["source_subject"]["corpus"]
        }
    });
    let inputs = root.join("target/corpus-proof");
    let input = inputs.join("test-authored-mixed-records.json");
    let output = inputs.join("mixed-projection.gap-ledger.json");
    write_json(&input, &compound)?;
    let projected = ripr_binary_probe(
        Some(root),
        &[
            "reports",
            "gap-ledger",
            "--records",
            input.to_str().ok_or("compound path is not UTF-8")?,
            "--out",
            output.to_str().ok_or("mixed output path is not UTF-8")?,
        ],
    )?;
    require(projected.status.success(), || {
        format!("actual mixed-record projection failed: {projected:?}")
    })?;
    let mixed = read_json(&output)?;
    let mixed_records = mixed["records"]
        .as_array()
        .ok_or("real records writer omitted mixed records")?;
    require(
        mixed_records.len() == records.len()
            && mixed_records
                .iter()
                .any(|record| record["language"] == "rust" && record["seam_id"] == SEAM)
            && mixed_records.iter().any(|record| {
                record["language"] == "python" && record["language_status"] == "preview"
            })
            && records.iter().all(|copied| {
                mixed_records.iter().any(|record| {
                    record["canonical_gap_id"] == copied["canonical_gap_id"]
                        && record["language"] == copied["language"]
                })
            }),
        || {
            format!(
                "records projection must retain all copied native identities and both literal languages: {compound}; {mixed}"
            )
        },
    )?;
    Ok(mixed)
}

fn assert_rust_corpus_currentness_row(status: &Value) -> ProofResult {
    require(
        status["gap_artifact_currentness"]
            == json!([{
                "kind": "gap_decision_ledger",
                "scope": "rust_workspace",
                "corpus_currentness": "current",
                "limitation": null
            }]),
        || {
            format!(
                "actual admitted Rust ledger must disclose its bounded current corpus scope: {status}"
            )
        },
    )
}

#[test]
fn passive_rust_ledger_discloses_actual_corpus_currentness_scope() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-rust-scope-disclosure")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let ledger = derive(root, &captured, "current")?;
    assert_capture_is_actionable(&captured, &ledger)?;
    install(root, &ledger)?;
    let status = framed_status(root)?;
    assert_current_action(&status)?;
    assert_rust_corpus_currentness_row(&status)
}

#[cfg(feature = "lang-python")]
fn assert_preview_corpus_unavailable(status: &Value) -> ProofResult {
    require(
        status["diagnostics"]["gap_artifacts"] == 1
            && status["diagnostics"]["gap_artifact_rejections"] == 0,
        || {
            format!(
                "known preview/mixed scope must retain old named-file admission of its real nonempty ledger: {status}"
            )
        },
    )?;
    require(
        status["gap_artifact_currentness"]
            == json!([{
                "kind": "gap_decision_ledger",
                "scope": "preview_or_mixed_language",
                "corpus_currentness": "unavailable",
                "limitation": "preview_or_mixed_corpus_provenance_unavailable"
            }]),
        || {
            format!(
                "a Rust hash must never authenticate known preview/mixed corpus currentness: {status}"
            )
        },
    )?;
    let components = status["analysis_status"]["components"]
        .as_array()
        .ok_or("scope status omitted components")?;
    require(
        components.iter().any(|component| {
            component["component"] == "gap_ledger" && component["state"] == "complete"
        }),
        || {
            format!(
                "known preview scope must preserve actual named-file ledger admission: {status}"
            )
        },
    )
}

#[cfg(feature = "lang-python")]
fn assert_preview_projection_retained(before: &Value, after: &Value) -> ProofResult {
    for path in [
        "/run_status",
        "/diagnostics/gap_artifacts",
        "/diagnostics/gap_artifact_rejections",
        "/diagnostics/actionable_gap_artifacts",
        "/diagnostics/actionable_diagnostics",
        "/diagnostics/seam_diagnostics",
        "/diagnostics/total",
        "/top_actionable_packet",
    ] {
        let before_value = before
            .pointer(path)
            .ok_or_else(|| format!("native preview baseline omitted {path}"))?;
        let after_value = after
            .pointer(path)
            .ok_or_else(|| format!("native preview scope result omitted {path}"))?;
        require(before_value == after_value, || {
            format!(
                "attaching a wrong-scope Rust corpus stamp must preserve existing preview projection at {path}: {before}; {after}"
            )
        })?;
    }
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn native_python_and_copied_mixed_records_cannot_claim_a_current_rust_corpus() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-preview-mixed-scope")?;
    let root = fixture.root();
    let (rust_corpus, python, mixed) = real_preview_and_mixed_scope_ledgers(root)?;
    for (label, ledger) in [
        ("native-python-preview", python),
        ("copied-mixed-records", mixed),
    ] {
        let mut baseline = ledger.clone();
        baseline["source_subject"]
            .as_object_mut()
            .ok_or("real scope ledger omitted source_subject")?
            .remove("corpus");
        install(root, &baseline)?;
        let admitted = framed_status(root)?;
        assert_preview_corpus_unavailable(&admitted)
            .map_err(|error| format!("{label} baseline: {error}"))?;

        let mut wrong_scope = baseline.clone();
        wrong_scope["source_subject"]
            .as_object_mut()
            .ok_or("scope ledger omitted named stamp")?
            .insert("corpus".into(), rust_corpus.clone());
        require(
            wrong_scope["records"] == baseline["records"]
                && wrong_scope["source_subject"]["files"] == baseline["source_subject"]["files"],
            || "wrong-scope control must not edit real records or named-file provenance".into(),
        )?;
        install(root, &wrong_scope)?;
        let qualified = framed_status(root)?;
        assert_preview_corpus_unavailable(&qualified)
            .map_err(|error| format!("{label} wrong-scope envelope: {error}"))?;
        assert_preview_projection_retained(&admitted, &qualified)?;
    }
    Ok(())
}

#[cfg(feature = "lang-python")]
#[test]
fn unknown_corpus_language_precedes_known_preview_in_both_record_orders() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-unknown-preview-order")?;
    let root = fixture.root();
    let (_, _, mixed) = real_preview_and_mixed_scope_ledgers(root)?;
    install(root, &mixed)?;
    assert_preview_corpus_unavailable(&framed_status(root)?)?;
    let original_head = head(root)?;
    for language in [None, Some("unknown_language")] {
        for preview_first in [false, true] {
            let mut ambiguous = mixed.clone();
            let records = ambiguous["records"]
                .as_array_mut()
                .ok_or("mixed ledger omitted records")?;
            for record in records
                .iter_mut()
                .filter(|record| record["language"] == "rust")
            {
                let object = record
                    .as_object_mut()
                    .ok_or("native Rust record must be an object")?;
                match language {
                    None => {
                        object.remove("language");
                    }
                    Some(language) => {
                        object.insert("language".into(), json!(language));
                    }
                }
            }
            records.sort_by_key(|record| {
                let is_preview = record["language"] == "python";
                if preview_first {
                    !is_preview
                } else {
                    is_preview
                }
            });
            require(
                records.iter().any(|record| {
                    record["language"] == "python" && record["language_status"] == "preview"
                }) && records
                    .iter()
                    .any(|record| record["seam_id"] == SEAM && record["language"] != "rust"),
                || {
                    "order control must retain genuine preview and the original now-unproven Rust subject".into()
                },
            )?;
            require(
                ambiguous["source_subject"] == mixed["source_subject"]
                    && head(root)? == original_head,
                || {
                    "unknown mixed-scope control must retain actual provenance bytes and HEAD"
                        .into()
                },
            )?;
            install(root, &ambiguous)?;
            let status = framed_status(root)?;
            assert_corpus_refused(
                &status,
                "unverifiable_subject",
                Some("source_subject_corpus_scope_missing"),
            )?;
            require(status["gap_artifact_currentness"] == json!([]), || {
                format!(
                    "refused unknown scope must not inherit a preview or current Rust admitted row: {status}"
                )
            })?;
        }
    }
    install(root, &mixed)?;
    assert_preview_corpus_unavailable(&framed_status(root)?)
}
