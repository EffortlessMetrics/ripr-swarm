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
const CURRENT_CORPUS_CACHE_SCHEMA_VERSION: &str = "1.83";
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
    AddEquality,
    EditUnclaimed,
    DeleteUnclaimed,
}

impl CorpusMutation {
    fn label(self) -> &'static str {
        match self {
            Self::AddEquality => "added-test",
            Self::EditUnclaimed => "changed-test",
            Self::DeleteUnclaimed => "deleted-test",
        }
    }

    fn apply(self, root: &Path) -> ProofResult {
        match self {
            Self::AddEquality => std::fs::write(root.join("tests/new_boundary.rs"), EQUALITY_TEST),
            Self::EditUnclaimed => {
                std::fs::write(root.join("tests/corpus_control.rs"), CORPUS_CONTROL_EDITED)
            }
            Self::DeleteUnclaimed => std::fs::remove_file(root.join("tests/corpus_control.rs")),
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
        let expected_state = if name == "gap_ledger" {
            "failed"
        } else {
            "limited"
        };
        require(
            component["state"] == expected_state && component["kind"] == kind,
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
        CorpusMutation::AddEquality => "strongly_gripped",
        CorpusMutation::EditUnclaimed | CorpusMutation::DeleteUnclaimed => "weakly_gripped",
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
        CorpusMutation::AddEquality => {
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
        CorpusMutation::EditUnclaimed | CorpusMutation::DeleteUnclaimed => {
            assert_capture_is_actionable(&fresh_capture, &fresh)?;
            assert_current_action(&recovered)
        }
    }
}

#[test]
fn passive_corpus_rejects_added_test_at_same_head_and_recovers() -> ProofResult {
    corpus_mutation_contract(CorpusMutation::AddEquality, "stale_corpus")
}

#[test]
fn passive_corpus_rejects_changed_unclaimed_test_at_same_head_and_recovers() -> ProofResult {
    corpus_mutation_contract(CorpusMutation::EditUnclaimed, "stale_corpus")
}

#[test]
fn passive_corpus_rejects_deleted_unclaimed_test_at_same_head_and_recovers() -> ProofResult {
    corpus_mutation_contract(CorpusMutation::DeleteUnclaimed, "stale_corpus")
}

fn captured_corpus(captured: &Value) -> ProofResult<&Value> {
    let corpus = captured["source_subject"]
        .get("corpus")
        .ok_or("analysis producer omitted source_subject.corpus")?;
    require(
        corpus["scope"] == "rust_workspace"
            && corpus["cache_schema_version"] == CURRENT_CORPUS_CACHE_SCHEMA_VERSION
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
                && receipt["classified_seams"]["seams"]
                    .as_array()
                    .is_some_and(|seams| {
                        seams.len() == 1
                            && seams[0]["seam"]["id"] == SEAM
                            && seams[0]["seam"]["file"] == "src/lib.rs"
                            && seams[0]["seam"]["owner"] == "src/lib.rs::discounted_total"
                    })
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
                        corpus.insert("cache_schema_version".into(), json!("1.62"));
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
    CorpusMutation::EditUnclaimed.apply(root)?;
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
    CorpusMutation::AddEquality.apply(root)?;
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
    for (label, language, unsupported_string) in [
        ("removed-language", None, false),
        ("null-language", Some(Value::Null), false),
        ("number-language", Some(json!(7)), false),
        ("object-language", Some(json!({"name": "rust"})), false),
        ("unknown-language", Some(json!("unknown_language")), true),
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
                record.insert("language".into(), language.clone());
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
        let status = framed_status(root)?;
        if unsupported_string {
            // Preserve the pre-existing unsupported string language contract.
            // The corpus gate must not downgrade malformed native schema.
            assert_unsupported_language_refused(&status)
        } else {
            assert_corpus_refused(
                &status,
                "unverifiable_subject",
                Some("source_subject_corpus_scope_missing"),
            )
        }
        .map_err(|error| format!("{label}: {error}"))?;
    }
    install(root, &original)?;
    assert_current_action(&framed_status(root)?)
}

fn assert_unsupported_language_refused(status: &Value) -> ProofResult {
    require(
        status["run_status"] == "cache_limited"
            && status["diagnostics"]["gap_artifacts"] == 0
            && status["diagnostics"]["gap_artifact_rejections"] == 1
            && status["diagnostics"]["actionable_gap_artifacts"] == 0
            && status["diagnostics"]["actionable_diagnostics"] == 0
            && status["top_actionable_packet"].is_null()
            && status["gap_artifact_currentness"] == json!([])
            && status["top_limitation"]["category"] == "malformed_artifact"
            && status["top_limitation"]["repair_route"] == "regenerate_gap_artifacts",
        || {
            format!(
                "unsupported language strings must retain malformed-artifact refusal and no actions: {status}"
            )
        },
    )?;
    let components = status["analysis_status"]["components"]
        .as_array()
        .ok_or("unsupported language refusal omitted components")?;
    require(
        components.iter().any(|component| {
            component["component"] == "gap_ledger"
                && component["state"] == "failed"
                && component["kind"] == "malformed_artifact"
        }) && components.iter().any(|component| {
            component["component"] == "cache"
                && component["state"] == "limited"
                && component["kind"] == "gap_artifact_rejected"
        }),
        || {
            format!(
                "unsupported language must preserve existing named schema/ledger rejection: {status}"
            )
        },
    )?;
    for collection in ["selected", "omitted"] {
        let items = status["diagnostic_budget"][collection]
            .as_array()
            .ok_or("unsupported language refusal omitted diagnostic evidence")?;
        require(
            !items.iter().any(|item| item["canonical_id"] == GAP),
            || format!("unsupported-language gap must not remain {collection}: {status}"),
        )?;
    }
    Ok(())
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
    // Reuse the governed python_return_value_shape fixture. Its exact
    // assertion makes the genuine native finding exposed and its ledger a
    // supported NoActionAlreadyObserved preview record. No unsupported
    // generic preview limit or receipt-comparator rejection supplies scope.
    std::fs::write(
        root.join("src/price.py"),
        "def final_price(amount):\n    return amount - 10\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("tests/test_price.py"),
        include_str!("../../../../fixtures/python_return_value_shape/input/tests/test_price.py"),
    )
    .map_err(|error| error.to_string())?;
    fixture_git::fixture_git_ok(
        root,
        &[
            "add",
            "ripr.toml",
            "pyproject.toml",
            "src/price.py",
            "tests/test_price.py",
        ],
    )?;
    fixture_git::fixture_git_ok(root, &["commit", "-m", "Python return-value base"])?;
    let python_base = head(root)?;
    std::fs::write(
        root.join("src/price.py"),
        include_str!("../../../../fixtures/python_return_value_shape/input/src/price.py"),
    )
    .map_err(|error| error.to_string())?;
    fixture_git::fixture_git_ok(root, &["add", "src/price.py"])?;
    fixture_git::fixture_git_ok(
        root,
        &["commit", "-m", "change observed Python return value"],
    )?;

    let rust_capture = capture(root)?;
    let rust_corpus = captured_corpus(&rust_capture)?.clone();
    cache_corpus_authority(root, &rust_corpus)?;
    let rust_ledger = derive(root, &rust_capture, "scope-rust")?;
    assert_capture_is_actionable(&rust_capture, &rust_ledger)?;
    let inputs = root.join("target/corpus-proof");
    std::fs::create_dir_all(&inputs).map_err(|error| error.to_string())?;
    let python_diff = fixture_git::fixture_git_output(
        root,
        &["diff", &python_base, "HEAD", "--", "src/price.py"],
    )?;
    require(python_diff.contains("+    return amount - 12"), || {
        format!(
            "Python-only producer stimulus must contain the governed changed return value: {python_diff}"
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
    // Pin the unmodified native producer's supported nonempty no-action
    // disposition before passive qualification. The exact fixture supplies
    // real Python preview tags, identities and named-file provenance; no
    // limits or commands are stripped to manufacture admission.
    let python_id = "gap:python:src/price.py:final_price:return_value:return_value:amount-12";
    require(
        python_capture["findings"]
            .as_array()
            .is_some_and(|findings| {
                findings.len() == 1
                    && findings[0]["classification"] == "exposed"
                    && findings[0]["canonical_gap_id"] == python_id
                    && findings[0]["language"] == "python"
                    && findings[0]["language_status"] == "preview"
            })
            && python_records.len() == 1
            && python_records[0]["canonical_gap_id"] == python_id
            && python_records[0]["kind"] == "NoActionAlreadyObserved"
            && python_records[0]["gap_state"] == "already_observed"
            && python_records[0]["repairability"] == "no_action"
            && python_records[0]["static_limit_kind"].is_null()
            && python_records[0].get("static_limits").is_none()
            && python_records[0]["verification_commands"] == json!([])
            && python_records[0]["receipt_command"].is_null()
            && python_ledger["summary"]["records_total"] == 1
            && python_ledger["summary"]["repairable_total"] == 0
            && python_ledger["summary"]["no_action_total"] == 1,
        || {
            format!(
                "genuine Python exact-return producer must retain its supported observed preview record: {python_capture}; {python_ledger}"
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
    for language in [
        None,
        Some(Value::Null),
        Some(json!(7)),
        Some(json!("unknown_language")),
    ] {
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
                match language.as_ref() {
                    None => {
                        object.remove("language");
                    }
                    Some(language) => {
                        object.insert("language".into(), language.clone());
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
            if language.as_ref().is_some_and(Value::is_string) {
                assert_unsupported_language_refused(&status)?;
            } else {
                assert_corpus_refused(
                    &status,
                    "unverifiable_subject",
                    Some("source_subject_corpus_scope_missing"),
                )?;
            }
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

fn assert_all_captured_named_bytes(root: &Path, ledger: &Value) -> ProofResult {
    let files = ledger["source_subject"]["files"]
        .as_array()
        .ok_or("genuine producer omitted named-file provenance")?;
    require(!files.is_empty(), || {
        "named-byte control cannot be vacuous".into()
    })?;
    for file in files {
        let path = file["path"].as_str().ok_or("named subject has no path")?;
        require(
            Path::new(path)
                .components()
                .all(|component| matches!(component, std::path::Component::Normal(_))),
            || format!("captured named subject must be a confined relative path: {path}"),
        )?;
        let bytes = std::fs::read(root.join(path)).map_err(|error| error.to_string())?;
        require(
            file["digest"] == format!("sha256:{:x}", Sha256::digest(&bytes)),
            || format!("old named subject changed: {file}"),
        )?;
    }
    Ok(())
}

#[test]
fn passive_nonempty_no_action_rust_ledger_still_requires_current_corpus() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-no-action-stale")?;
    let root = fixture.root();
    CorpusMutation::AddEquality.apply(root)?;
    let captured = capture(root)?;
    let old = derive(root, &captured, "already-observed")?;
    require(
        captured["seams"][0]["grip_class"] == "strongly_gripped"
            && old["summary"]["records_total"] == 1
            && old["summary"]["no_action_total"] == 1
            && old["summary"]["repairable_total"] == 0
            && old["records"][0]["canonical_gap_id"] == "gap:rust:67f9714ba37b3967"
            && old["records"][0]["gap_state"] == "already_observed"
            && old["records"][0]["language"] == "rust"
            && old["records"][0]["seam_id"] == SEAM
            && old["records"][0]["kind"] == "NoActionAlreadyObserved",
        || {
            format!(
                "no-action stimulus must be a real nonempty already-observed Rust record: {captured}; {old}"
            )
        },
    )?;
    let observed_id = old["records"][0]["canonical_gap_id"]
        .as_str()
        .ok_or("genuine no-action record omitted its canonical identity")?
        .to_string();
    // Closing the gap removes its actionable canonical group. The actual
    // ledger retains the exact seam via its existing observed fallback ID.
    require(
        captured["seams"][0]["evidence_record"]["canonical_gap_id"].is_null(),
        || {
            format!(
                "genuinely observed seam must not pretend its old actionable canonical group persists: {captured}"
            )
        },
    )?;
    assert_all_captured_named_bytes(root, &old)?;
    install(root, &old)?;
    let current = framed_status(root)?;
    assert_current_no_action(&current)?;
    assert_rust_corpus_currentness_row(&current)?;
    let old_head = head(root)?;
    let old_bytes = std::fs::read(root.join("target/ripr/reports/gap-decision-ledger.json"))
        .map_err(|error| error.to_string())?;
    CorpusMutation::EditUnclaimed.apply(root)?;
    require(head(root)? == old_head, || {
        "no-action corpus edit changed HEAD".into()
    })?;
    assert_all_captured_named_bytes(root, &old)?;
    require(
        std::fs::read(root.join("target/ripr/reports/gap-decision-ledger.json"))
            .map_err(|error| error.to_string())?
            == old_bytes,
        || "no-action stimulus edited the old ledger".into(),
    )?;
    assert_corpus_refused(&framed_status(root)?, "stale_corpus", None)?;
    let fresh_capture = capture(root)?;
    let fresh = derive(root, &fresh_capture, "regenerated-no-action")?;
    require(
        fresh["records"][0]["canonical_gap_id"] == observed_id
            && fresh["records"][0]["seam_id"] == SEAM
            && fresh["records"][0]["gap_state"] == "already_observed"
            && fresh["summary"]["records_total"] == 1
            && fresh["summary"]["no_action_total"] == 1
            && fresh["source_subject"]["corpus"]["files_content_hash"]
                != old["source_subject"]["corpus"]["files_content_hash"],
        || {
            format!(
                "real recovery must keep the no-action subject and recapture changed content authority: {fresh}"
            )
        },
    )?;
    install(root, &fresh)?;
    let recovered = framed_status(root)?;
    assert_current_no_action(&recovered)?;
    assert_rust_corpus_currentness_row(&recovered)
}

fn refreshed_command_session(root: &Path) -> ProofResult<LspSession> {
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
    Ok(session)
}

fn scoped_gap_command(session: &mut LspSession, command: &str) -> ProofResult<Value> {
    let response = session.request_with_timeout(
        "workspace/executeCommand",
        json!({"command": command, "arguments": [{"gap_id": GAP}]}),
        ANALYSIS_TIMEOUT,
    )?;
    Ok(expect_result(&response, command)?.clone())
}

#[test]
fn gap_commands_recheck_rust_corpus_without_a_refresh_or_document_notification() -> ProofResult {
    let fixture = CorpusFixture::new("corpus-command-time")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let old = derive(root, &captured, "original")?;
    assert_capture_is_actionable(&captured, &old)?;
    install(root, &old)?;
    let mut session = refreshed_command_session(root)?;
    let outcome: ProofResult = (|| {
        let initial = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_current_action(&initial)?;
        require(
            initial["analysis_status"]["state"] == "succeeded"
                && initial["analysis_status"]["reason"] == "explicit_refresh",
            || format!("command proof requires a completed genuine refresh: {initial}"),
        )?;
        let context = scoped_gap_command(&mut session, "ripr.collectContext")?;
        let packets = context["packets"].as_array().ok_or_else(|| {
            format!(
                "initial context must render the real nonempty selected ledger packet: {context}"
            )
        })?;
        require(
            packets.len() == 1
                && context["source"] == "gap_decision_ledger"
                && packets[0]["canonical_gap_id"] == GAP
                && packets[0]["gap_state"] == "actionable"
                && packets[0]["verification_commands"]
                    .as_array()
                    .is_some_and(|commands| !commands.is_empty()),
            || {
                format!(
                    "initial context must contain the exact actionable producer subject: {context}"
                )
            },
        )?;
        let repair = scoped_gap_command(&mut session, "ripr.collectRepairPacket")?;
        require(
            repair["kind"] == "repair_packet"
                && repair["canonical_gap_id"] == GAP
                && repair["verify_command"]
                    .as_str()
                    .is_some_and(|command| !command.is_empty())
                && repair["receipt_command"]
                    .as_str()
                    .is_some_and(|command| !command.is_empty()),
            || format!("initial repair command must return its real useful route: {repair}"),
        )?;
        let old_head = head(root)?;
        CorpusMutation::EditUnclaimed.apply(root)?;
        require(head(root)? == old_head, || {
            "command-time edit changed HEAD".into()
        })?;
        assert_original_named_bytes(root, &old)?;
        // No didChange, didSave, watched-file notification, or refresh occurs.
        // Each explicit packet read must inspect the saved corpus itself.
        for command in ["ripr.collectContext", "ripr.collectRepairPacket"] {
            let refused = scoped_gap_command(&mut session, command)?;
            require(
                refused["kind"] == "repair_packet"
                    && refused["status"] == "not_actionable_or_incomplete"
                    && refused["reason"].as_str().is_some_and(|reason| {
                        reason.starts_with("stale_corpus:")
                            && reason.contains("ripr check --format repo-exposure-json")
                            && reason.contains("ripr reports gap-ledger")
                    })
                    && refused.get("packets").is_none()
                    && refused.get("verify_command").is_none()
                    && refused.get("receipt_command").is_none(),
                || {
                    format!(
                        "{command} must refuse current repair authority after same-HEAD corpus drift: {refused}"
                    )
                },
            )?;
        }
        let retained = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        require(
            retained["analysis_status"]["snapshot_id"] == initial["analysis_status"]["snapshot_id"]
                && retained["analysis_status"]["current_input_identity"]
                    == initial["analysis_status"]["current_input_identity"]
                && retained["analysis_status"]["reason"] == "explicit_refresh",
            || {
                format!(
                    "the command refusal must be independent of another analysis refresh: {initial}; {retained}"
                )
            },
        )?;
        Ok(())
    })();
    let closed = shutdown_exit_and_wait(&mut session);
    outcome?;
    closed
}

#[test]
fn ledger_derivation_requires_an_absolute_captured_producer_frame_for_corpus_transfer()
-> ProofResult {
    let fixture = CorpusFixture::new("corpus-input-frame")?;
    let root = fixture.root();
    let captured = capture(root)?;
    let producer_root = captured
        .pointer("/artifact/repository/root")
        .and_then(Value::as_str)
        .ok_or("real capture omitted producer root frame")?;
    require(
        captured.get("root").is_none()
            && Path::new(producer_root).is_absolute()
            && std::fs::canonicalize(producer_root).map_err(|error| error.to_string())?
                == std::fs::canonicalize(root).map_err(|error| error.to_string())?,
        || format!("fixture must begin with the actual canonical producer frame: {captured}"),
    )?;
    let current = derive(root, &captured, "absolute-frame")?;
    assert_capture_is_actionable(&captured, &current)?;
    require(
        current["source_subject"]["corpus"] == captured["source_subject"]["corpus"],
        || "same-frame writer did not copy the actual captured aggregate".into(),
    )?;
    install(root, &current)?;
    assert_current_action(&framed_status(root)?)?;
    for (label, frame) in [
        ("missing-frame", None),
        ("relative-frame", Some(json!("."))),
        ("non-string-frame", Some(json!(7))),
    ] {
        let mut unbound = captured.clone();
        let repository = unbound["artifact"]["repository"]
            .as_object_mut()
            .ok_or("capture has no repository object")?;
        match frame {
            None => {
                repository.remove("root");
            }
            Some(frame) => {
                repository.insert("root".into(), frame);
            }
        }
        require(
            unbound["seams"] == captured["seams"]
                && unbound["source_subject"] == captured["source_subject"],
            || "frame control must retain all genuine records and captured digests".into(),
        )?;
        let derived = derive(root, &unbound, label)?;
        require(
            derived["records"] == current["records"]
                && derived["source_subject"]["files"] == current["source_subject"]["files"]
                && derived["source_subject"]["corpus"]
                    == json!({
                        "unavailable": "input_source_subject_corpus_root_missing"
                    }),
            || {
                format!(
                    "unbound input frame must not borrow the writer's current root/corpus authority: {unbound}; {derived}"
                )
            },
        )?;
        install(root, &derived)?;
        assert_corpus_refused(
            &framed_status(root)?,
            "unverifiable_subject",
            Some("source_subject_corpus_root_missing"),
        )?;
    }
    install(root, &current)?;
    assert_current_action(&framed_status(root)?)
}

#[test]
fn foreign_root_ledger_derivation_cannot_rebase_captured_named_subjects_or_mint_corpus()
-> ProofResult {
    let a = CorpusFixture::new("corpus-derive-source-a")?;
    let b = CorpusFixture::new("corpus-derive-foreign-b")?;
    let captured_a = capture(a.root())?;
    let captured_b = capture(b.root())?;
    let current_b = derive(b.root(), &captured_b, "own-b")?;
    assert_capture_is_actionable(&captured_b, &current_b)?;
    require(
        captured_a["source_subject"] == captured_b["source_subject"],
        || {
            "foreign root control must have genuinely equal source bytes and content receipts"
                .into()
        },
    )?;
    for path in ["src/lib.rs", "tests/pricing.rs", "tests/corpus_control.rs"] {
        require(
            std::fs::read(a.root().join(path)).map_err(|error| error.to_string())?
                == std::fs::read(b.root().join(path)).map_err(|error| error.to_string())?,
            || format!("byte-identical foreign control differs at {path}"),
        )?;
    }
    install(b.root(), &current_b)?;
    assert_current_action(&framed_status(b.root())?)?;
    let foreign = derive(b.root(), &captured_a, "foreign-a")?;
    require(
        foreign["records"][0]["canonical_gap_id"] == GAP
            && foreign["records"][0]["seam_id"] == SEAM
            && foreign["records"]
                .as_array()
                .is_some_and(|records| records.len() == 1)
            && foreign["source_subject"].is_null()
            && foreign["source_subject_unavailable"] == "input_source_subject_incomplete",
        || {
            format!(
                "distinct producer frame cannot rebase nonempty named subjects into sibling B, even with equal bytes: {foreign}"
            )
        },
    )?;
    install(b.root(), &foreign)?;
    assert_corpus_refused(
        &framed_status(b.root())?,
        "unverifiable_subject",
        Some("source_subject_missing"),
    )?;
    install(b.root(), &current_b)?;
    assert_current_action(&framed_status(b.root())?)
}

fn assert_current_gap_command_packets(session: &mut LspSession) -> ProofResult {
    let context = scoped_gap_command(session, "ripr.collectContext")?;
    let packets = context["packets"].as_array().ok_or_else(|| {
        format!(
            "current context must contain its genuine nonempty selected ledger packet: {context}"
        )
    })?;
    require(
        context["source"] == "gap_decision_ledger"
            && packets.len() == 1
            && packets[0]["canonical_gap_id"] == GAP
            && packets[0]["gap_state"] == "actionable"
            && packets[0]["verification_commands"]
                .as_array()
                .is_some_and(|commands| !commands.is_empty()),
        || format!("current context must select the exact real producer gap: {context}"),
    )?;
    let repair = scoped_gap_command(session, "ripr.collectRepairPacket")?;
    require(
        repair["kind"] == "repair_packet"
            && repair["canonical_gap_id"] == GAP
            && repair["verify_command"]
                .as_str()
                .is_some_and(|command| !command.is_empty())
            && repair["receipt_command"]
                .as_str()
                .is_some_and(|command| !command.is_empty()),
        || format!("current repair command must preserve a meaningful complete route: {repair}"),
    )
}

fn assert_command_snapshot_retained(initial: &Value, retained: &Value) -> ProofResult {
    let before = &initial["analysis_status"];
    let after = &retained["analysis_status"];
    require(
        before["state"] == "succeeded"
            && before["reason"] == "explicit_refresh"
            && before["snapshot_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && before["current_input_identity"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && after["state"] == "succeeded"
            && after["reason"] == "explicit_refresh"
            && after["snapshot_id"] == before["snapshot_id"]
            && after["current_input_identity"] == before["current_input_identity"],
        || {
            format!(
                "packet admission must recheck its artifact independently of another workspace refresh: {initial}; {retained}"
            )
        },
    )
}

#[test]
fn gap_commands_refuse_a_copied_foreign_ledger_after_a_healthy_byte_identical_workspace_refresh()
-> ProofResult {
    let a = CorpusFixture::new("corpus-command-source-a")?;
    let b = CorpusFixture::new("corpus-command-selected-b")?;
    require(
        std::fs::canonicalize(a.root()).map_err(|error| error.to_string())?
            != std::fs::canonicalize(b.root()).map_err(|error| error.to_string())?,
        || "foreign-root command control requires two physically distinct roots".into(),
    )?;
    for path in [
        "Cargo.toml",
        "src/lib.rs",
        "tests/pricing.rs",
        "tests/corpus_control.rs",
    ] {
        require(
            std::fs::read(a.root().join(path)).map_err(|error| error.to_string())?
                == std::fs::read(b.root().join(path)).map_err(|error| error.to_string())?,
            || {
                format!(
                    "foreign-root control must have genuinely identical producer bytes at {path}"
                )
            },
        )?;
    }
    let captured_a = capture(a.root())?;
    let ledger_a = derive(a.root(), &captured_a, "producer-a")?;
    assert_capture_is_actionable(&captured_a, &ledger_a)?;
    install(a.root(), &ledger_a)?;
    assert_current_action(&framed_status(a.root())?)?;
    let captured_b = capture(b.root())?;
    let ledger_b = derive(b.root(), &captured_b, "producer-b")?;
    assert_capture_is_actionable(&captured_b, &ledger_b)?;
    require(
        ledger_a["source_subject"] == ledger_b["source_subject"]
            && ledger_a["root"] != ledger_b["root"],
        || {
            format!(
                "named and corpus receipts must be equal while real native producer roots differ: {ledger_a}; {ledger_b}"
            )
        },
    )?;
    install(b.root(), &ledger_b)?;
    let mut session = refreshed_command_session(b.root())?;
    let outcome: ProofResult = (|| {
        let initial = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_current_action(&initial)?;
        assert_current_gap_command_packets(&mut session)?;
        let old_head = head(b.root())?;
        let source = a
            .root()
            .join("target/ripr/reports/gap-decision-ledger.json");
        let destination = b
            .root()
            .join("target/ripr/reports/gap-decision-ledger.json");
        let original_bytes = std::fs::read(&source).map_err(|error| error.to_string())?;
        // Copy the real A artifact only after B has a healthy, nonempty
        // committed snapshot. No JSON provenance, records, or stamps change.
        std::fs::copy(&source, &destination).map_err(|error| error.to_string())?;
        require(
            std::fs::read(&destination).map_err(|error| error.to_string())? == original_bytes
                && std::fs::read(&source).map_err(|error| error.to_string())? == original_bytes
                && head(b.root())? == old_head,
            || {
                "foreign artifact transfer must copy A byte-for-byte without changing either producer or B's HEAD".into()
            },
        )?;
        assert_original_named_bytes(b.root(), &ledger_a)?;
        // No notification or refresh occurs. Equal current content hashes
        // cannot establish that A's ledger belongs to the selected B root.
        for command in ["ripr.collectContext", "ripr.collectRepairPacket"] {
            let refused = scoped_gap_command(&mut session, command)?;
            require(
                refused["kind"] == "repair_packet"
                    && refused["status"] == "not_actionable_or_incomplete"
                    && refused["reason"].as_str().is_some_and(|reason| {
                        reason.starts_with("wrong_root:")
                            && reason.contains("workspace")
                            && reason.contains("regenerate")
                    })
                    && refused.get("packets").is_none()
                    && refused.get("verify_command").is_none()
                    && refused.get("receipt_command").is_none(),
                || {
                    format!(
                        "{command} must refuse foreign root authority before exposing a current packet: {refused}"
                    )
                },
            )?;
        }
        let retained = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_command_snapshot_retained(&initial, &retained)?;
        // The original real B artifact remains a useful recovery control,
        // admitted on command read without restarting or refreshing B.
        install(b.root(), &ledger_b)?;
        assert_current_gap_command_packets(&mut session)?;
        let recovered = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_command_snapshot_retained(&initial, &recovered)
    })();
    let closed = shutdown_exit_and_wait(&mut session);
    outcome?;
    closed
}

#[cfg(unix)]
#[test]
fn gap_commands_admit_the_real_canonical_ledger_from_a_same_physical_alias_after_refresh()
-> ProofResult {
    let a = CorpusFixture::new("corpus-command-alias-source")?;
    let alias_owner = unique_compat_fixture_root("corpus-command-alias-selection")?;
    let alias = alias_owner.path.join("selected-workspace");
    std::os::unix::fs::symlink(a.root(), &alias).map_err(|error| error.to_string())?;
    require(
        alias.as_os_str() != a.root().as_os_str()
            && std::fs::canonicalize(&alias).map_err(|error| error.to_string())?
                == std::fs::canonicalize(a.root()).map_err(|error| error.to_string())?,
        || {
            "alias command control must select a different spelling of the same physical root"
                .into()
        },
    )?;
    let captured = capture(a.root())?;
    let ledger = derive(a.root(), &captured, "physical-producer")?;
    assert_capture_is_actionable(&captured, &ledger)?;
    install(a.root(), &ledger)?;
    let original_bytes = std::fs::read(
        a.root()
            .join("target/ripr/reports/gap-decision-ledger.json"),
    )
    .map_err(|error| error.to_string())?;
    let mut session = refreshed_command_session(&alias)?;
    let outcome: ProofResult = (|| {
        let initial = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_current_action(&initial)?;
        assert_current_gap_command_packets(&mut session)?;
        // Reinstall the same real canonical producer bytes through the
        // selected alias after refresh. Same-physical-root admission must
        // survive the full command-time artifact validator as well.
        std::fs::write(
            alias.join("target/ripr/reports/gap-decision-ledger.json"),
            &original_bytes,
        )
        .map_err(|error| error.to_string())?;
        require(
            std::fs::read(
                a.root()
                    .join("target/ripr/reports/gap-decision-ledger.json"),
            )
            .map_err(|error| error.to_string())?
                == original_bytes,
            || "alias transfer must retain the real producer artifact byte-for-byte".into(),
        )?;
        assert_current_gap_command_packets(&mut session)?;
        let retained = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_command_snapshot_retained(&initial, &retained)
    })();
    let closed = shutdown_exit_and_wait(&mut session);
    outcome?;
    closed
}

#[cfg(feature = "lang-python")]
fn genuine_python_operator_ledger(
    label: &str,
    before_operator: &str,
    current_operator: &str,
) -> ProofResult<(CorpusFixture, Value)> {
    let fixture = CorpusFixture::new(label)?;
    let root = fixture.root();
    std::fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled=[\"rust\",\"python\"]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("pyproject.toml"),
        "[project]\nname=\"python-admission-proof\"\nversion=\"0.0.0\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join("src/__init__.py"), "").map_err(|error| error.to_string())?;
    let source = include_str!("../../../../fixtures/python_boundary_gap/input/src/discount.py");
    std::fs::write(
        root.join("src/discount.py"),
        source.replace(">=", before_operator),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("tests/test_discount.py"),
        include_str!("../../../../fixtures/python_boundary_gap/input/tests/test_discount.py"),
    )
    .map_err(|error| error.to_string())?;
    fixture_git::fixture_git_ok(
        root,
        &[
            "add",
            "ripr.toml",
            "pyproject.toml",
            "src/__init__.py",
            "src/discount.py",
            "tests/test_discount.py",
        ],
    )?;
    fixture_git::fixture_git_ok(root, &["commit", "-m", "Python comparison base"])?;
    let before = head(root)?;
    std::fs::write(
        root.join("src/discount.py"),
        source.replace(">=", current_operator),
    )
    .map_err(|error| error.to_string())?;
    fixture_git::fixture_git_ok(root, &["add", "src/discount.py"])?;
    fixture_git::fixture_git_ok(root, &["commit", "-m", "Python comparison current"])?;
    let inputs = root.join("target/corpus-proof");
    std::fs::create_dir_all(&inputs).map_err(|error| error.to_string())?;
    let diff_path = inputs.join("python-admission.diff");
    let diff =
        fixture_git::fixture_git_output(root, &["diff", &before, "HEAD", "--", "src/discount.py"])?;
    std::fs::write(&diff_path, diff).map_err(|error| error.to_string())?;
    let captured = cli_json(
        root,
        &[
            "check",
            "--diff",
            diff_path.to_str().ok_or("diff path is not UTF-8")?,
            "--json",
        ],
    )?;
    let id = format!(
        "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount{current_operator}threshold"
    );
    require(
        captured["findings"].as_array().is_some_and(|findings| {
            findings.len() == 1
                && findings[0]["canonical_gap_id"] == id
                && findings[0]["language"] == "python"
                && findings[0]["language_status"] == "preview"
        }),
        || {
            format!(
                "actual Python check must establish exact nonempty producer subject before consumer proof: {captured}"
            )
        },
    )?;
    let ledger = derive_check_scope_ledger(root, &captured, "python-admission")?;
    require(
        ledger["records"].as_array().is_some_and(|records| {
            records.len() == 1
                && records[0]["canonical_gap_id"] == id
                && records[0]["language"] == "python"
                && records[0]["language_status"] == "preview"
        }),
        || format!("actual ledger must preserve exact typed producer identity: {ledger}"),
    )?;
    install(root, &ledger)?;
    Ok((fixture, ledger))
}

#[test]
#[cfg(feature = "lang-python")]
fn genuine_python_quoted_comparison_ledger_reaches_passive_and_live_action_admission() -> ProofResult
{
    let (fixture, ledger) = genuine_python_operator_ledger("python-quoted-identity", ">", ">=")?;
    let root = fixture.root();
    let record = &ledger["records"][0];
    let id =
        "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount>=threshold";
    let verify = "python -m pytest tests/test_discount.py::test_apply_discount_smoke";
    require(
        record["canonical_gap_id"] == id
            && record["kind"] == "MissingBoundaryAssertion"
            && record["gap_state"] == "actionable"
            && record["repairability"] == "repairable"
            && record["static_limit_kind"] == "python_preview"
            && record["verification_commands"] == json!([verify])
            && record["projection_eligibility"]["agent_packet"]["eligible"] == true
            && record["receipt_command"].as_str().is_some_and(|command| {
                command.starts_with(&format!("ripr receipt write --gap '{id}'"))
            }),
        || {
            format!(
                "genuine comparison producer must retain native card/quote/operational marker, never rewrite them for admission: {ledger}"
            )
        },
    )?;
    let installed = root.join("target/ripr/reports/gap-decision-ledger.json");
    let original = std::fs::read(&installed).map_err(|error| error.to_string())?;
    let mut session = refreshed_command_session(root)?;
    let outcome: ProofResult = (|| {
        let status = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_preview_corpus_unavailable(&status)?;
        require(
            status["diagnostics"]["actionable_gap_artifacts"] == 1
                && status["top_actionable_packet"]["canonical_gap_id"] == id,
            || format!("real quoted comparison must remain meaningful and actionable: {status}"),
        )?;
        for command in ["ripr.collectContext", "ripr.collectRepairPacket"] {
            let response = session.request_with_timeout(
                "workspace/executeCommand",
                json!({"command": command, "arguments": [{"gap_id": id}]}),
                ANALYSIS_TIMEOUT,
            )?;
            let actual = expect_result(&response, command)?;
            if command == "ripr.collectContext" {
                require(
                    actual["source"] == "gap_decision_ledger"
                        && actual["packets"]
                            .as_array()
                            .is_some_and(|packets| packets.len() == 1)
                        && actual["packets"][0]["canonical_gap_id"] == id
                        && actual["packets"][0]["verification_commands"]
                            == record["verification_commands"],
                    || {
                        format!(
                            "live context must carry the exact real Python producer packet: {actual}"
                        )
                    },
                )?;
            } else {
                require(
                    actual["kind"] == "repair_packet"
                        && actual["canonical_gap_id"] == id
                        && actual["verify_command"] == verify
                        && actual["receipt_command"] == record["receipt_command"]
                        && actual["raw_evidence_refs"] == record["evidence_ids"]
                        && actual["allowed_edit_surface"]
                            .as_array()
                            .is_some_and(|paths| !paths.is_empty()),
                    || format!("live repair must preserve native commands and subject: {actual}"),
                )?;
            }
        }
        require(
            std::fs::read(&installed).map_err(|error| error.to_string())? == original,
            || "consumer must replay unchanged genuine producer ledger bytes".into(),
        )
    })();
    let shutdown = shutdown_exit_and_wait(&mut session);
    outcome?;
    shutdown
}

#[test]
#[cfg(feature = "lang-python")]
fn genuine_python_unavailable_repair_card_remains_admitted_advisory_without_actions() -> ProofResult
{
    let (fixture, ledger) = genuine_python_operator_ledger("python-operational-limit", "!=", "==")?;
    let root = fixture.root();
    let record = &ledger["records"][0];
    let id =
        "gap:python:src/discount.py:apply_discount:predicate_boundary:predicate:amount==threshold";
    require(
        record["canonical_gap_id"] == id
            && record["kind"] == "StaticLimitation"
            && record["gap_state"] == "static_limitation"
            && record["repairability"] == "analyzer_limitation"
            && record["static_limit_kind"] == "python_repair_card_unavailable"
            && record["static_limit_detail"]
                .as_str()
                .is_some_and(|detail| {
                    detail.contains("no repair card")
                        && detail.contains("no concrete missing discriminator")
                })
            && record["verification_commands"] == json!([])
            && record["receipt_command"].is_null()
            && record["repair_route"].is_null()
            && record["projection_eligibility"]["agent_packet"]["eligible"] == false,
        || {
            format!(
                "real unavailable-card record must remain nonempty/advisory and preserve detail, not gain a repair route: {ledger}"
            )
        },
    )?;
    let mut session = refreshed_command_session(root)?;
    let outcome: ProofResult = (|| {
        let status = execute_compat_command(&mut session, "ripr.collectWorkspaceStatus")?;
        assert_preview_corpus_unavailable(&status)?;
        require(
            status["diagnostics"]["actionable_gap_artifacts"] == 0
                && status["top_actionable_packet"].is_null(),
            || format!("operational preview limitation must never become actionable: {status}"),
        )?;
        // Ineligible collectContext has an existing InvalidParams contract;
        // admission does not authorize that unsupported context projection.
        let response = session.request_with_timeout(
            "workspace/executeCommand",
            json!({"command": "ripr.collectRepairPacket", "arguments": [{"gap_id": id}]}),
            ANALYSIS_TIMEOUT,
        )?;
        let repair = expect_result(&response, "ripr.collectRepairPacket")?;
        require(
            repair["kind"] == "repair_packet"
                && repair["status"] == "not_actionable_or_incomplete"
                && repair["reason"].as_str().is_some_and(|reason| {
                    reason.contains("not agent-packet eligible")
                        && reason.contains("not_repairable")
                })
                && repair.get("verify_command").is_none()
                && repair.get("receipt_command").is_none(),
            || {
                format!(
                    "live read must preserve actual advisory exclusion, not unsupported-kind refusal or repair commands: {repair}"
                )
            },
        )
    })();
    let shutdown = shutdown_exit_and_wait(&mut session);
    outcome?;
    shutdown
}
