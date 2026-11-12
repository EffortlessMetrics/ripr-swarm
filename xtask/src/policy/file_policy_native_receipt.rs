//! Diagnostic receipt for exact native file-policy enumeration.
//!
//! This format intentionally cannot authorize full proof: declared context is
//! compared, but effective Cargo feature/configuration provenance is unresolved.
//! Hosted workflows must not enable this path until that gap is closed.
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    TestCoverageObservation, accept_file_policy_coverage, validate_test_covered_by_recorded,
};

pub(crate) const RECEIPT_NAME: &str = "file-policy-native-receipt.json";
const SCHEMA: &str = "ripr.file-policy-native-receipt.v1";
const STATUS: &str = "native_valid_context_unverified";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeclaredContext {
    pub repository: String,
    pub checkout_sha: String,
    pub run_id: String,
    pub attempt: u32,
    pub target: String,
    pub cargo_profile: String,
    pub configuration_sha256: String,
    pub features_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpectedSource {
    pub tree: String,
    pub ledger_sha256: String,
    pub lock_sha256: String,
}

/// Resolve immutable source inputs from the verifier's expected commit, not the receipt.
pub(crate) fn expected_source(sha: &str) -> Result<ExpectedSource, String> {
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected checkout must be a full commit SHA".into());
    }
    let tree_arg = format!("{sha}^{{tree}}");
    let ledger_arg = format!("{sha}:policy/non-rust-allowlist.toml");
    let lock_arg = format!("{sha}:Cargo.lock");
    let tree = crate::run::run_output(
        "git",
        &["--no-replace-objects", "rev-parse", "--verify", &tree_arg],
    )?;
    let ledger =
        crate::run::run_output_bytes("git", &["--no-replace-objects", "show", &ledger_arg])?;
    let lock = crate::run::run_output_bytes("git", &["--no-replace-objects", "show", &lock_arg])?;
    Ok(ExpectedSource {
        tree: tree.trim().to_string(),
        ledger_sha256: hash(&ledger),
        lock_sha256: hash(&lock),
    })
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Row {
    line: usize,
    command: String,
    declared_host: String,
    selected: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: String,
    status: String,
    context: DeclaredContext,
    source_tree: String,
    ledger_sha256: String,
    lock_sha256: String,
    environment_sha256: String,
    cargo_version_sha256: String,
    rustc_version_sha256: String,
    rows: Vec<Row>,
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    head: String,
    tree: String,
    ledger: String,
    lock: String,
    environment: String,
    context_sha256: String,
    cargo_version_sha256: String,
    rustc_version_sha256: String,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn file_hash(path: &str) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| hash(&bytes))
        .map_err(|e| format!("cannot read {path}: {e}"))
}
fn environment_hash() -> String {
    let mut vars: Vec<_> = std::env::vars_os().collect();
    vars.sort_by(|a, b| a.0.cmp(&b.0));
    let mut digest = Sha256::new();
    for (key, value) in vars {
        for part in [key, value] {
            let bytes = part.as_encoded_bytes();
            digest.update((bytes.len() as u64).to_be_bytes());
            digest.update(bytes);
        }
    }
    format!("{:x}", digest.finalize())
}
fn snapshot(context_path: &str) -> Result<Snapshot, String> {
    let git = |args: &[&str]| crate::run::run_output("git", args);
    let head = git(&["--no-replace-objects", "rev-parse", "HEAD"])?;
    let tree = git(&["--no-replace-objects", "rev-parse", "HEAD^{tree}"])?;
    let dirty = git(&["status", "--porcelain", "--untracked-files=all"])?;
    if !dirty.trim().is_empty() {
        return Err("native receipt requires a clean checkout including untracked files".into());
    }
    Ok(Snapshot {
        head: head.trim().to_string(),
        tree: tree.trim().to_string(),
        ledger: file_hash("policy/non-rust-allowlist.toml")?,
        lock: file_hash("Cargo.lock")?,
        environment: environment_hash(),
        context_sha256: file_hash(context_path)?,
        cargo_version_sha256: hash(crate::run::run_output("cargo", &["--version"])?.as_bytes()),
        rustc_version_sha256: hash(crate::run::run_output("rustc", &["-vV"])?.as_bytes()),
    })
}
fn context_is_well_formed(c: &DeclaredContext) -> bool {
    let digest = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let sha = |s: &str| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'))
    };
    sha(&c.checkout_sha)
        && c.attempt > 0
        && token(&c.repository)
        && !c.run_id.is_empty()
        && c.run_id.bytes().all(|b| b.is_ascii_digit())
        && token(&c.target)
        && token(&c.cargo_profile)
        && digest(&c.configuration_sha256)
        && digest(&c.features_sha256)
}
fn rows(coverage: Vec<TestCoverageObservation>) -> Result<Vec<Row>, String> {
    let mut any = false;
    let mut rows = Vec::new();
    for observation in coverage {
        if let Some(selected) = &observation.selected {
            if selected.is_empty() {
                return Err("native receipt has an empty applicable selection".into());
            }
            let unique: BTreeSet<_> = selected.iter().collect();
            if unique.len() != selected.len() {
                return Err("native receipt has duplicate or ambiguous names within a selector; binary identity is unavailable".into());
            }
            any = true;
        }
        rows.push(Row {
            line: observation.selector.line,
            command: observation.selector.command,
            declared_host: observation
                .selector
                .host
                .map_or("all", crate::FilePolicyHost::name)
                .to_string(),
            selected: observation.selected,
        });
    }
    if !any {
        return Err("native receipt has no applicable subjects".into());
    }
    Ok(rows)
}

/// The output is removed before any producer work. Only the existing exact
/// Cargo enumerator and packet acceptance boundary can reach the write.
pub(crate) fn produce(args: &[String]) -> Result<(), String> {
    let output_path = crate::reports_dir().join(RECEIPT_NAME);
    let output = output_path.as_path();
    prepare_output(args, output)?;
    let context_text =
        fs::read_to_string(&args[1]).map_err(|e| format!("cannot read declared context: {e}"))?;
    let context: DeclaredContext = serde_json::from_str(&context_text)
        .map_err(|e| format!("invalid declared context: {e}"))?;
    if !context_is_well_formed(&context) {
        return Err("incomplete declared context".into());
    }
    produce_after_clear(
        context,
        output,
        &hash(context_text.as_bytes()),
        || snapshot(&args[1]),
        || {
            let commands = crate::read_file_policy_test_commands("policy/non-rust-allowlist.toml")?;
            let mut diagnostics = Vec::new();
            accept_file_policy_coverage(|| {
                validate_test_covered_by_recorded(
                    "policy/non-rust-allowlist.toml",
                    &commands,
                    &mut diagnostics,
                )
            })
        },
    )
}

fn prepare_output(args: &[String], output: &Path) -> Result<(), String> {
    // Do not remove the user's context input if it aliases the receipt path.
    if args.len() == 2
        && args[0] == "--context"
        && let (Ok(context), Ok(prior)) = (fs::canonicalize(&args[1]), fs::canonicalize(output))
        && context == prior
    {
        return Err("declared context must not be the receipt output".into());
    }
    clear_output(output)?;
    if args.len() != 2 || args[0] != "--context" {
        return Err("file-policy-native-receipt expects --context <json>".into());
    }
    Ok(())
}

fn clear_output(output: &Path) -> Result<(), String> {
    match fs::remove_file(output) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot clear receipt: {error}")),
    }
}

fn produce_after_clear(
    context: DeclaredContext,
    output: &Path,
    expected_context_sha256: &str,
    mut capture: impl FnMut() -> Result<Snapshot, String>,
    native: impl FnOnce() -> Result<Vec<TestCoverageObservation>, String>,
) -> Result<(), String> {
    let before = capture()?;
    if before.context_sha256 != expected_context_sha256 {
        return Err("declared context changed before native validation".into());
    }
    if before.head != context.checkout_sha {
        return Err("declared checkout differs from actual HEAD".into());
    }
    let rows = rows(native()?)?;
    let after = capture()?;
    if before != after {
        return Err("native receipt source or execution context changed during validation".into());
    }
    let receipt = Receipt {
        schema: SCHEMA.into(),
        status: STATUS.into(),
        context,
        source_tree: before.tree,
        ledger_sha256: before.ledger,
        lock_sha256: before.lock,
        environment_sha256: before.environment,
        cargo_version_sha256: before.cargo_version_sha256,
        rustc_version_sha256: before.rustc_version_sha256,
        rows,
    };
    let text = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?;
    fs::create_dir_all(output.parent().ok_or("receipt has no parent")?)
        .map_err(|e| format!("cannot create receipt directory: {e}"))?;
    fs::write(output, format!("{text}\n")).map_err(|e| format!("cannot write receipt: {e}"))?;
    println!(
        "native validation completed; full_proof_pending: effective Cargo context is unverified"
    );
    Ok(())
}

/// Binding diagnostics never upgrade an unresolved receipt to full proof.
pub(crate) fn validate_binding(
    text: &str,
    expected: &DeclaredContext,
    source: &ExpectedSource,
    selected_attempt: u32,
    producer_result: &str,
    verifier_attempt: u32,
) -> Result<(), String> {
    let receipt: Receipt =
        serde_json::from_str(text).map_err(|e| format!("invalid native receipt: {e}"))?;
    if receipt.schema != SCHEMA || receipt.status != STATUS {
        return Err("unsupported or pending native receipt".into());
    }
    if !context_is_well_formed(expected) || &receipt.context != expected {
        return Err("native receipt declared context mismatch".into());
    }
    if receipt.source_tree != source.tree
        || receipt.ledger_sha256 != source.ledger_sha256
        || receipt.lock_sha256 != source.lock_sha256
    {
        return Err("native receipt source tree, ledger or lock mismatch".into());
    }
    if receipt.context.attempt != selected_attempt || verifier_attempt != selected_attempt {
        return Err("native receipt or verifier binary attempt mismatch".into());
    }
    if producer_result != "success" {
        return Err("native producer matrix did not succeed".into());
    }
    if receipt.rows.is_empty() || receipt.rows.iter().all(|row| row.selected.is_none()) {
        return Err("native receipt has no subjects".into());
    }
    for row in &receipt.rows {
        if row.line == 0
            || !crate::is_cargo_test_command(&row.command)
            || !matches!(row.declared_host.as_str(), "all" | "unix" | "windows")
        {
            return Err("native receipt has an invalid selector declaration".into());
        }
        if let Some(selected) = &row.selected {
            if selected.is_empty()
                || selected.iter().any(|name| name.is_empty())
                || selected.iter().collect::<BTreeSet<_>>().len() != selected.len()
            {
                return Err("native receipt has empty or ambiguous subject identities".into());
            }
        }
    }
    Ok(())
}

pub(crate) fn require_full(
    text: &str,
    expected: &DeclaredContext,
    source: &ExpectedSource,
    selected_attempt: u32,
    producer_result: &str,
    verifier_attempt: u32,
) -> Result<(), String> {
    validate_binding(
        text,
        expected,
        source,
        selected_attempt,
        producer_result,
        verifier_attempt,
    )?;
    Err("native receipt cannot authorize full proof: effective features and configuration are unverified".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> DeclaredContext {
        DeclaredContext {
            repository: "owner/repo".into(),
            checkout_sha: "a".repeat(40),
            run_id: "123".into(),
            attempt: 2,
            target: "x86_64-unknown-linux-gnu".into(),
            cargo_profile: "test".into(),
            configuration_sha256: "b".repeat(64),
            features_sha256: "c".repeat(64),
        }
    }
    fn source() -> ExpectedSource {
        ExpectedSource {
            tree: "d".repeat(40),
            ledger_sha256: "e".repeat(64),
            lock_sha256: "f".repeat(64),
        }
    }
    fn receipt() -> Receipt {
        let source = source();
        Receipt {
            schema: SCHEMA.into(),
            status: STATUS.into(),
            context: context(),
            source_tree: source.tree,
            ledger_sha256: source.ledger_sha256,
            lock_sha256: source.lock_sha256,
            environment_sha256: "1".repeat(64),
            cargo_version_sha256: "2".repeat(64),
            rustc_version_sha256: "3".repeat(64),
            rows: vec![Row {
                line: 1,
                command: "cargo test -p probe exact".into(),
                declared_host: "all".into(),
                selected: Some(vec!["tests::exact".into()]),
            }],
        }
    }
    fn text(receipt: &Receipt) -> Result<String, String> {
        serde_json::to_string(receipt).map_err(|e| e.to_string())
    }

    fn snapshot_fixture() -> Snapshot {
        Snapshot {
            head: "a".repeat(40),
            tree: "d".repeat(40),
            ledger: "e".repeat(64),
            lock: "f".repeat(64),
            environment: "1".repeat(64),
            context_sha256: "4".repeat(64),
            cargo_version_sha256: "2".repeat(64),
            rustc_version_sha256: "3".repeat(64),
        }
    }
    fn observation() -> TestCoverageObservation {
        TestCoverageObservation {
            selector: crate::FilePolicyTestCommand {
                line: 1,
                command: "cargo test -p probe exact".into(),
                host: None,
            },
            host: crate::FilePolicyHost::Unix,
            selected: Some(vec!["tests::exact".into()]),
        }
    }

    #[test]
    fn malformed_invocation_clears_stale_output_but_context_alias_is_preserved()
    -> Result<(), String> {
        let output =
            std::env::temp_dir().join(format!("ripr-native-prepare-{}.json", std::process::id()));
        fs::write(&output, "stale").map_err(|e| e.to_string())?;
        assert!(prepare_output(&[], &output).is_err());
        assert!(!output.exists());
        fs::write(&output, "context").map_err(|e| e.to_string())?;
        let args = vec![
            "--context".to_string(),
            output.to_string_lossy().to_string(),
        ];
        assert!(prepare_output(&args, &output).is_err());
        assert_eq!(
            fs::read_to_string(&output).map_err(|e| e.to_string())?,
            "context"
        );
        fs::remove_file(&output).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn failed_native_or_changed_snapshot_cannot_publish_a_receipt() -> Result<(), String> {
        let output =
            std::env::temp_dir().join(format!("ripr-native-receipt-{}.json", std::process::id()));
        fs::write(&output, "stale").map_err(|e| e.to_string())?;
        clear_output(&output)?;
        assert!(
            produce_after_clear(
                context(),
                &output,
                &"4".repeat(64),
                || Ok(snapshot_fixture()),
                || Err("native producer failed".into())
            )
            .is_err()
        );
        assert!(!output.exists());
        let mut n = 0;
        let result = produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || {
                n += 1;
                let mut value = snapshot_fixture();
                if n == 2 {
                    value.lock = "0".repeat(64);
                }
                Ok(value)
            },
            || Ok(vec![observation()]),
        );
        assert!(result.is_err());
        assert!(!output.exists());
        produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || Ok(snapshot_fixture()),
            || Ok(vec![observation()]),
        )?;
        let text = fs::read_to_string(&output).map_err(|e| e.to_string())?;
        fs::remove_file(&output).map_err(|e| e.to_string())?;
        validate_binding(&text, &context(), &source(), 2, "success", 2)?;
        assert!(require_full(&text, &context(), &source(), 2, "success", 2).is_err());
        Ok(())
    }

    #[test]
    fn binding_accepts_valid_diagnostic_but_full_proof_refuses_unverified_context()
    -> Result<(), String> {
        let text = text(&receipt())?;
        validate_binding(&text, &context(), &source(), 2, "success", 2)?;
        let error = require_full(&text, &context(), &source(), 2, "success", 2)
            .err()
            .ok_or("unverified receipt must fail full proof")?;
        assert!(error.contains("unverified"));
        Ok(())
    }

    #[test]
    fn binding_refuses_wrong_context_attempt_source_status_and_duplicate_identity()
    -> Result<(), String> {
        let baseline = text(&receipt())?;
        let mut changed = context();
        changed.target = "other-target".into();
        assert!(validate_binding(&baseline, &changed, &source(), 2, "success", 2).is_err());
        assert!(validate_binding(&baseline, &context(), &source(), 1, "success", 2).is_err());
        assert!(validate_binding(&baseline, &context(), &source(), 2, "success", 1).is_err());
        assert!(validate_binding(&baseline, &context(), &source(), 2, "failure", 2).is_err());
        let mut stale = source();
        stale.tree = "0".repeat(40);
        assert!(validate_binding(&baseline, &context(), &stale, 2, "success", 2).is_err());
        let mut pending = receipt();
        pending.status = "full_proof_pending".into();
        assert!(
            validate_binding(&text(&pending)?, &context(), &source(), 2, "success", 2).is_err()
        );
        let mut duplicate = receipt();
        duplicate.rows[0].selected = Some(vec!["tests::exact".into(), "tests::exact".into()]);
        assert!(
            validate_binding(&text(&duplicate)?, &context(), &source(), 2, "success", 2).is_err()
        );
        let mut empty = receipt();
        empty.rows[0].selected = Some(vec![]);
        assert!(validate_binding(&text(&empty)?, &context(), &source(), 2, "success", 2).is_err());
        let duplicate_key = baseline.replacen("\"schema\":", "\"schema\":\"x\",\"schema\":", 1);
        assert!(validate_binding(&duplicate_key, &context(), &source(), 2, "success", 2).is_err());
        Ok(())
    }

    #[test]
    fn duplicate_names_within_a_selector_refuse_but_repeated_declarations_remain_valid()
    -> Result<(), String> {
        let observation = |line, selected: Vec<&str>| TestCoverageObservation {
            selector: crate::FilePolicyTestCommand {
                line,
                command: "cargo test -p probe".into(),
                host: None,
            },
            host: crate::FilePolicyHost::Unix,
            selected: Some(selected.into_iter().map(str::to_string).collect()),
        };
        assert!(rows(vec![observation(1, vec!["a", "a"])]).is_err());
        assert_eq!(
            rows(vec![observation(1, vec!["a"]), observation(2, vec!["a"])])?.len(),
            2
        );
        Ok(())
    }
}
