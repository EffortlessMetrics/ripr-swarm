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
