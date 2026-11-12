//! Diagnostic receipt for exact native file-policy enumeration.
//!
//! This format intentionally cannot authorize full proof: declared context is
//! compared, but effective Cargo feature/configuration provenance is unresolved.
//! Hosted workflows must not enable this path until that gap is closed.
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    NativeDeadline, TestCoverageObservation, accept_file_policy_coverage, capture_with_remaining,
    covered_by_listing_args, validate_test_covered_by_recorded_budgeted,
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

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

fn bounded_probe(program: &str, args: &[&str], timeout: Duration) -> Result<Vec<u8>, String> {
    bounded_probe_with_limit(program, args, timeout, 8 * 1024 * 1024)
}

fn bounded_probe_with_limit(
    program: &str,
    args: &[&str],
    timeout: Duration,
    stdout_limit: usize,
) -> Result<Vec<u8>, String> {
    let args: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
    let output = crate::run::capture_bytes_in_dir_with_budget(
        Path::new(program),
        &args,
        (Path::new("."), None),
        &[],
        crate::run::ByteCaptureBudget {
            timeout,
            stdout_bytes: stdout_limit,
            stderr_bytes: 64 * 1024,
        },
        "file-policy native receipt probe",
    )?;
    if output.timed_out {
        return Err(format!("native receipt {program} probe timed out"));
    }
    if !output.status.is_some_and(|status| status.success()) {
        return Err(format!("native receipt {program} probe failed"));
    }
    Ok(output.stdout)
}
fn probe_text(program: &str, args: &[&str]) -> Result<String, String> {
    String::from_utf8(bounded_probe(program, args, PROBE_TIMEOUT)?)
        .map_err(|_| format!("native receipt {program} probe output is not UTF-8"))
}

/// Resolve immutable source inputs from the verifier's expected commit, not the receipt.
pub(crate) fn expected_source(sha: &str) -> Result<ExpectedSource, String> {
    if sha.len() != 40 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected checkout must be a full commit SHA".into());
    }
    let tree_arg = format!("{sha}^{{tree}}");
    let ledger_arg = format!("{sha}:policy/non-rust-allowlist.toml");
    let lock_arg = format!("{sha}:Cargo.lock");
    let tree = probe_text(
        "git",
        &["--no-replace-objects", "rev-parse", "--verify", &tree_arg],
    )?;
    let ledger = bounded_probe(
        "git",
        &["--no-replace-objects", "show", &ledger_arg],
        PROBE_TIMEOUT,
    )?;
    let lock = bounded_probe(
        "git",
        &["--no-replace-objects", "show", &lock_arg],
        PROBE_TIMEOUT,
    )?;
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
    context_sha256: String,
    cargo_version_sha256: String,
    rustc_version_sha256: String,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
const INPUT_LIMIT: u64 = 8 * 1024 * 1024;
/// Refuses symlinks and special files before opening. This check and the later
/// open are not atomic against a concurrent hostile filesystem mutation.
/// Build each path to inspect, never probing a Windows drive/UNC prefix
/// without its root. Drive-relative and root-without-drive paths are refused.
fn inspection_paths(path: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    #[cfg(windows)]
    if path.has_root() && !path.is_absolute() {
        return Err("rooted path without drive is unsupported".into());
    }
    let mut current = std::path::PathBuf::new();
    let mut parts = path.components();
    let mut paths = Vec::new();
    while let Some(part) = parts.next() {
        if matches!(part, std::path::Component::ParentDir) {
            return Err("path contains parent traversal".into());
        }
        if matches!(part, std::path::Component::Prefix(_)) {
            current.push(part.as_os_str());
            match parts.next() {
                Some(std::path::Component::RootDir) => {
                    current.push(std::path::Component::RootDir.as_os_str())
                }
                _ => return Err("drive-relative path is unsupported".into()),
            }
        } else {
            current.push(part.as_os_str());
        }
        paths.push(current.clone());
    }
    if paths.is_empty() {
        return Err("native input path is empty".into());
    }
    Ok(paths)
}

fn validate_components(path: &Path, final_file: bool) -> Result<(), String> {
    let paths = inspection_paths(path)?;
    for (index, current) in paths.iter().enumerate() {
        let metadata = fs::symlink_metadata(current)
            .map_err(|e| format!("cannot inspect {}: {e}", current.display()))?;
        let kind = metadata.file_type();
        if kind.is_symlink() {
            return Err(format!(
                "native input path contains symlink: {}",
                current.display()
            ));
        }
        let last = index + 1 == paths.len();
        if (last && final_file && !kind.is_file()) || ((!last || !final_file) && !kind.is_dir()) {
            return Err(format!(
                "native input path is not an expected regular file/directory: {}",
                current.display()
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_directory(path: &Path) -> Result<(), String> {
    validate_components(path, false)
}

pub(crate) fn read_input(path: &Path) -> Result<Vec<u8>, String> {
    validate_components(path, true)?;
    let file = fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(INPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if bytes.len() as u64 > INPUT_LIMIT {
        return Err(format!("{} exceeds input budget", path.display()));
    }
    Ok(bytes)
}
fn file_hash(path: &str) -> Result<String, String> {
    read_input(Path::new(path)).map(|bytes| hash(&bytes))
}

fn snapshot(context_path: &str, deadline: &NativeDeadline) -> Result<Snapshot, String> {
    let bounded = |program: &str, args: &[&str]| -> Result<Vec<u8>, String> {
        capture_with_remaining(
            || deadline.remaining(),
            PROBE_TIMEOUT,
            |timeout| bounded_probe(program, args, timeout),
        )
    };
    let git = |args: &[&str]| -> Result<String, String> {
        String::from_utf8(bounded("git", args)?)
            .map_err(|_| "native receipt git probe output is not UTF-8".to_string())
    };
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
        context_sha256: file_hash(context_path)?,
        cargo_version_sha256: hash(&bounded("cargo", &["--version"])?),
        rustc_version_sha256: hash(&bounded("rustc", &["-vV"])?),
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
    let mut declarations = BTreeSet::new();
    for observation in coverage {
        let host = observation
            .selector
            .host
            .map_or("all", crate::FilePolicyHost::name)
            .to_string();
        let identity = (
            observation.selector.line,
            covered_by_listing_args("native receipt", &observation.selector)?,
            host.clone(),
        );
        if !declarations.insert(identity) {
            return Err("native receipt repeats a selector declaration identity".into());
        }
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
            declared_host: host,
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
    let deadline = NativeDeadline::start();
    let output_path = crate::reports_dir().join(RECEIPT_NAME);
    let output = output_path.as_path();
    prepare_output(args, output)?;
    deadline.remaining()?;
    let context_text = String::from_utf8(read_input(Path::new(&args[1]))?)
        .map_err(|_| "declared context is not UTF-8".to_string())?;
    let context: DeclaredContext = serde_json::from_str(&context_text)
        .map_err(|e| format!("invalid declared context: {e}"))?;
    if !context_is_well_formed(&context) {
        return Err("incomplete declared context".into());
    }
    produce_after_clear(
        context,
        output,
        &hash(context_text.as_bytes()),
        || deadline.remaining().map(|_| ()),
        || snapshot(&args[1], &deadline),
        || {
            let commands = crate::read_file_policy_test_commands("policy/non-rust-allowlist.toml")?;
            let mut diagnostics = Vec::new();
            accept_file_policy_coverage(|| {
                validate_test_covered_by_recorded_budgeted(
                    "policy/non-rust-allowlist.toml",
                    &commands,
                    &mut diagnostics,
                    Some(&deadline),
                )
            })
        },
    )
}

/// Check existing output ancestors without following symlinks. Missing
/// directories are permitted for later creation. This is not atomic with
/// removal, directory creation, or publication against concurrent mutation.
fn validate_output_parent(output: &Path) -> Result<(), String> {
    let parent = output.parent().ok_or("receipt has no parent")?;
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    for current in inspection_paths(parent)? {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "receipt output parent contains symlink: {}",
                    current.display()
                ));
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(format!(
                    "receipt output parent is not a directory: {}",
                    current.display()
                ));
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => {
                return Err(format!(
                    "cannot inspect receipt output parent {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

fn prepare_output(args: &[String], output: &Path) -> Result<(), String> {
    validate_output_parent(output)?;
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
    check_deadline: impl Fn() -> Result<(), String>,
    mut capture: impl FnMut() -> Result<Snapshot, String>,
    native: impl FnOnce() -> Result<Vec<TestCoverageObservation>, String>,
) -> Result<(), String> {
    check_deadline()?;
    let before = capture()?;
    check_deadline()?;
    if before.context_sha256 != expected_context_sha256 {
        return Err("declared context changed before native validation".into());
    }
    if before.head != context.checkout_sha {
        return Err("declared checkout differs from actual HEAD".into());
    }
    let rows = rows(native()?)?;
    check_deadline()?;
    let after = capture()?;
    check_deadline()?;
    if before != after {
        return Err(
            "native receipt source, declared context or tool observation changed during validation"
                .into(),
        );
    }
    let receipt = Receipt {
        schema: SCHEMA.into(),
        status: STATUS.into(),
        context,
        source_tree: before.tree,
        ledger_sha256: before.ledger,
        lock_sha256: before.lock,
        cargo_version_sha256: before.cargo_version_sha256,
        rustc_version_sha256: before.rustc_version_sha256,
        rows,
    };
    let text = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?;
    check_deadline()?;
    publish_receipt_with(output, format!("{text}\n").as_bytes(), |file, bytes| {
        file.write_all(bytes)
    })?;
    if let Err(error) = check_deadline() {
        return match clear_output(output) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!("{error}; {cleanup}")),
        };
    }
    println!(
        "native validation completed; full_proof_pending: effective Cargo context is unverified"
    );
    Ok(())
}

/// Publish only complete bytes. The temporary path is owned only after an
/// exclusive create; a preexisting path is never overwritten or removed.
fn publish_receipt_with(
    output: &Path,
    bytes: &[u8],
    write: impl FnOnce(&mut fs::File, &[u8]) -> std::io::Result<()>,
) -> Result<(), String> {
    validate_output_parent(output)?;
    let parent = output.parent().ok_or("receipt has no parent")?;
    fs::create_dir_all(parent).map_err(|e| format!("cannot create receipt directory: {e}"))?;
    validate_output_parent(output)?;
    // Different publications in one process must not share an owned temporary
    // path. This does not relax the fixed output's single-producer requirement.
    static TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = parent.join(format!(
        ".{RECEIPT_NAME}.{}.{sequence}.tmp",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| format!("cannot exclusively create receipt temporary file: {e}"))?;
    let written = write(&mut file, bytes).and_then(|()| file.flush());
    drop(file);
    let result = written.and_then(|()| {
        validate_output_parent(output).map_err(std::io::Error::other)?;
        fs::rename(&temp, output)
    });
    if let Err(error) = result {
        return match fs::remove_file(&temp) {
            Ok(()) => Err(format!("cannot publish receipt: {error}")),
            Err(cleanup) => Err(format!(
                "cannot publish receipt: {error}; cannot remove owned temporary file: {cleanup}"
            )),
        };
    }
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
    let hex = |value: &str, len: usize| {
        value.len() == len && value.bytes().all(|b| b.is_ascii_hexdigit())
    };
    if !hex(&receipt.source_tree, 40)
        || !hex(&receipt.ledger_sha256, 64)
        || !hex(&receipt.lock_sha256, 64)
        || !hex(&receipt.cargo_version_sha256, 64)
        || !hex(&receipt.rustc_version_sha256, 64)
        || !hex(&source.tree, 40)
        || !hex(&source.ledger_sha256, 64)
        || !hex(&source.lock_sha256, 64)
    {
        return Err("native receipt or expected source has malformed digest".into());
    }
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
    let mut declarations = BTreeSet::new();
    for row in &receipt.rows {
        if row.line == 0
            || !crate::is_cargo_test_command(&row.command)
            || !matches!(row.declared_host.as_str(), "all" | "unix" | "windows")
        {
            return Err("native receipt has an invalid selector declaration".into());
        }
        let selector = crate::FilePolicyTestCommand {
            line: row.line,
            command: row.command.clone(),
            host: None,
        };
        let args = covered_by_listing_args("native receipt", &selector)?;
        if !declarations.insert((row.line, args, row.declared_host.as_str())) {
            return Err("native receipt repeats a selector declaration identity".into());
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
    Err("native receipt cannot authorize full proof: environment, tool and build context, row completeness, packet identities and producer provenance are unverified".into())
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
        let root = crate::tests::temp_dir("ripr-native-prepare");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let output = root.join("receipt.json");
        fs::write(&output, "stale").map_err(|e| e.to_string())?;
        let error = prepare_output(&[], &output)
            .err()
            .ok_or("malformed invocation accepted")?;
        assert!(error.contains("expects --context"), "{error}");
        assert!(!output.exists());
        fs::write(&output, "context").map_err(|e| e.to_string())?;
        let args = vec![
            "--context".to_string(),
            output.to_string_lossy().to_string(),
        ];
        let error = prepare_output(&args, &output)
            .err()
            .ok_or("context alias accepted")?;
        assert!(error.contains("must not be the receipt output"), "{error}");
        assert_eq!(
            fs::read_to_string(&output).map_err(|e| e.to_string())?,
            "context"
        );
        fs::remove_file(&output).map_err(|e| e.to_string())?;
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn failed_native_or_changed_snapshot_cannot_publish_a_receipt() -> Result<(), String> {
        let root = crate::tests::temp_dir("ripr-native-receipt");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let output = root.join("receipt.json");
        fs::write(&output, "stale").map_err(|e| e.to_string())?;
        clear_output(&output)?;
        let error = produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || Ok(()),
            || Ok(snapshot_fixture()),
            || Err("native producer failed".into()),
        )
        .err()
        .ok_or("failed producer accepted")?;
        assert_eq!(error, "native producer failed");
        assert!(!output.exists());
        let mut n = 0;
        let result = produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || Ok(()),
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
        let error = result.err().ok_or("changed snapshot accepted")?;
        assert!(error.contains("changed during validation"), "{error}");
        assert!(!output.exists());
        produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || Ok(()),
            || Ok(snapshot_fixture()),
            || Ok(vec![observation()]),
        )?;
        let text = fs::read_to_string(&output).map_err(|e| e.to_string())?;
        fs::remove_file(&output).map_err(|e| e.to_string())?;
        validate_binding(&text, &context(), &source(), 2, "success", 2)?;
        assert!(require_full(&text, &context(), &source(), 2, "success", 2).is_err());
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn aggregate_deadline_exhaustion_never_publishes_or_starts_native() -> Result<(), String> {
        use std::cell::Cell;
        let root = crate::tests::temp_dir("native-receipt-deadline");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let output = root.join(RECEIPT_NAME);
        let called = Cell::new(false);
        let error = produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || Err("native receipt aggregate deadline exhausted".to_string()),
            || Ok(snapshot_fixture()),
            || {
                called.set(true);
                Ok(vec![observation()])
            },
        )
        .err()
        .ok_or("expired producer unexpectedly succeeded")?;
        assert!(error.contains("aggregate deadline exhausted"), "{error}");
        assert!(!called.get());
        assert!(!output.exists());

        let checks = Cell::new(0);
        let error = produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || {
                let next = checks.get() + 1;
                checks.set(next);
                if next == 3 {
                    Err("native receipt aggregate deadline exhausted".to_string())
                } else {
                    Ok(())
                }
            },
            || Ok(snapshot_fixture()),
            || Ok(vec![observation()]),
        )
        .err()
        .ok_or("deadline exhausted after native unexpectedly succeeded")?;
        assert!(error.contains("aggregate deadline exhausted"), "{error}");
        assert!(!output.exists());

        let checks = Cell::new(0);
        let error = produce_after_clear(
            context(),
            &output,
            &"4".repeat(64),
            || {
                let next = checks.get() + 1;
                checks.set(next);
                if next == 6 {
                    Err("native receipt aggregate deadline exhausted".to_string())
                } else {
                    Ok(())
                }
            },
            || Ok(snapshot_fixture()),
            || Ok(vec![observation()]),
        )
        .err()
        .ok_or("deadline exhausted during publication unexpectedly succeeded")?;
        assert!(error.contains("aggregate deadline exhausted"), "{error}");
        assert!(!output.exists());
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn aggregate_deadline_clamps_each_child_and_refuses_exhaustion() -> Result<(), String> {
        assert_eq!(
            super::super::remaining_native_budget(Duration::from_secs(10), Duration::from_secs(7))?,
            Duration::from_secs(3)
        );
        assert!(
            super::super::remaining_native_budget(Duration::from_secs(10), Duration::from_secs(10))
                .is_err()
        );
        let started = std::time::Instant::now()
            .checked_sub(Duration::from_secs(1_171))
            .ok_or("clock underflow")?;
        let expired = NativeDeadline { started };
        assert!(expired.timeout(Duration::from_secs(30)).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn hung_probe_is_refused() -> Result<(), String> {
        let ready = bounded_probe("sh", &["-c", "printf ready"], Duration::from_secs(2))?;
        assert_eq!(ready, b"ready");
        let result = bounded_probe("sh", &["-c", "sleep 2"], Duration::from_millis(50));
        let error = result
            .err()
            .ok_or("sleeping probe unexpectedly succeeded")?;
        assert!(error.contains("timed out"), "{error}");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn oversized_probe_output_is_refused() -> Result<(), String> {
        let within =
            bounded_probe_with_limit("sh", &["-c", "printf 1234"], Duration::from_secs(2), 4)?;
        assert_eq!(within, b"1234");
        let too_large =
            bounded_probe_with_limit("sh", &["-c", "printf 12345"], Duration::from_secs(2), 4);
        assert!(too_large.is_err());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn windows_prefixes_are_rooted_before_inspection() -> Result<(), String> {
        let drive = inspection_paths(Path::new(r"C:\alpha\beta"))?;
        assert_eq!(drive.first(), Some(&std::path::PathBuf::from(r"C:\")));
        let unc = inspection_paths(Path::new(r"\\server\share\folder"))?;
        assert_eq!(
            unc.first(),
            Some(&std::path::PathBuf::from(r"\\server\share\"))
        );
        assert!(inspection_paths(Path::new(r"C:relative\file")).is_err());
        assert!(inspection_paths(Path::new(r"\relative\file")).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn fifo_without_writer_is_refused_before_open() -> Result<(), String> {
        if std::env::var("RIPR_NATIVE_FIFO_GUARD_CHILD")
            .ok()
            .as_deref()
            == Some("1")
        {
            let name = std::env::var("RIPR_NATIVE_FIFO_GUARD_PATH").map_err(|e| e.to_string())?;
            let error = read_input(Path::new(&name))
                .err()
                .ok_or("FIFO was accepted")?;
            assert!(error.contains("expected regular file/directory"), "{error}");
            println!("native FIFO refused before open");
            return Ok(());
        }
        let root = crate::tests::temp_dir("ripr-native-fifo");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let fifo = root.join("receipt");
        let name = fifo.to_str().ok_or("non-UTF-8 FIFO path")?;
        bounded_probe("mkfifo", &[name], Duration::from_secs(2))?;
        use std::os::unix::fs::FileTypeExt;
        assert!(
            fs::symlink_metadata(&fifo)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_fifo()
        );
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args = ["--exact".to_string(), "policy::file_policy::native_receipt::tests::fifo_without_writer_is_refused_before_open".to_string(), "--nocapture".to_string()];
        let env = [
            ("RIPR_NATIVE_FIFO_GUARD_CHILD", "1"),
            ("RIPR_NATIVE_FIFO_GUARD_PATH", name),
        ];
        let output = crate::run::capture_bytes_in_dir_with_env_budget(
            &exe,
            &args,
            Path::new("."),
            (&env, &[]),
            crate::run::ByteCaptureBudget {
                timeout: Duration::from_secs(5),
                stdout_bytes: 64 * 1024,
                stderr_bytes: 64 * 1024,
            },
            "native FIFO reader control",
        )?;
        assert!(
            !output.timed_out && output.status.is_some_and(|s| s.success()),
            "FIFO reader did not complete successfully"
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("native FIFO refused before open")
        );
        let regular = root.join("regular");
        fs::write(&regular, "ok").map_err(|e| e.to_string())?;
        assert_eq!(read_input(&regular)?, b"ok");
        let link = root.join("link");
        std::os::unix::fs::symlink(&regular, &link).map_err(|e| e.to_string())?;
        let error = read_input(&link).err().ok_or("symlink input accepted")?;
        assert!(error.contains("symlink"), "{error}");
        fs::remove_file(&link).map_err(|e| e.to_string())?;
        fs::remove_file(&regular).map_err(|e| e.to_string())?;
        fs::remove_file(&fifo).map_err(|e| e.to_string())?;
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn symlink_output_parent_preserves_unrelated_sentinel() -> Result<(), String> {
        let root = crate::tests::temp_dir("native-output-parent");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let outside = root.join("outside");
        fs::create_dir(&outside).map_err(|e| e.to_string())?;
        let sentinel = outside.join(RECEIPT_NAME);
        fs::write(&sentinel, "do not replace").map_err(|e| e.to_string())?;
        let link = root.join("output-link");
        std::os::unix::fs::symlink(&outside, &link).map_err(|e| e.to_string())?;
        let output = link.join(RECEIPT_NAME);
        let args = vec![
            "--context".to_string(),
            root.join("context").to_string_lossy().to_string(),
        ];
        let error = prepare_output(&args, &output)
            .err()
            .ok_or("symlink parent accepted")?;
        assert!(error.contains("symlink"), "{error}");
        let error = publish_receipt_with(&output, b"new", |file, bytes| file.write_all(bytes))
            .err()
            .ok_or("symlink parent published")?;
        assert!(error.contains("symlink"), "{error}");
        assert_eq!(
            fs::read_to_string(&sentinel).map_err(|e| e.to_string())?,
            "do not replace"
        );
        let missing_output = root.join("new/deep").join(RECEIPT_NAME);
        prepare_output(&args, &missing_output)?;
        publish_receipt_with(&missing_output, b"new", |file, bytes| file.write_all(bytes))?;
        assert_eq!(
            fs::read(&missing_output).map_err(|e| e.to_string())?,
            b"new"
        );
        fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn overlapping_publications_keep_distinct_temporary_files() -> Result<(), String> {
        let root = crate::tests::temp_dir("native-overlapping-publication");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let first = root.join(RECEIPT_NAME);
        let second = root.join("other-receipt.json");
        publish_receipt_with(&first, b"first", |file, bytes| {
            // The outer temporary file is still open when the inner publication
            // starts. A PID-only name deterministically refuses here.
            publish_receipt_with(&second, b"second", |inner, body| inner.write_all(body))
                .map_err(std::io::Error::other)?;
            file.write_all(bytes)
        })?;
        assert_eq!(fs::read(&first).map_err(|e| e.to_string())?, b"first");
        assert_eq!(fs::read(&second).map_err(|e| e.to_string())?, b"second");
        assert_eq!(fs::read_dir(&root).map_err(|e| e.to_string())?.count(), 2);
        fs::remove_file(&first).map_err(|e| e.to_string())?;
        fs::remove_file(&second).map_err(|e| e.to_string())?;
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn overlapping_write_failure_cleans_only_its_owned_temporary() -> Result<(), String> {
        let root = crate::tests::temp_dir("native-overlapping-failure");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let first = root.join(RECEIPT_NAME);
        let second = root.join("other-receipt.json");
        let error = publish_receipt_with(&first, b"first", |file, bytes| {
            publish_receipt_with(&second, b"second", |inner, body| inner.write_all(body))
                .map_err(std::io::Error::other)?;
            file.write_all(bytes)?;
            Err(std::io::Error::other("injected outer write failure"))
        })
        .err()
        .ok_or("failed outer write was accepted")?;
        assert!(error.contains("injected outer write failure"), "{error}");
        assert!(!first.exists());
        assert_eq!(fs::read(&second).map_err(|e| e.to_string())?, b"second");
        assert_eq!(fs::read_dir(&root).map_err(|e| e.to_string())?.count(), 1);
        fs::remove_file(&second).map_err(|e| e.to_string())?;
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn partial_write_failure_never_publishes_receipt() -> Result<(), String> {
        let root = crate::tests::temp_dir("ripr-native-atomic");
        let root = fs::canonicalize(&root).map_err(|e| e.to_string())?;
        let output = root.join(RECEIPT_NAME);
        let result = publish_receipt_with(&output, b"complete", |file, _| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected write failure"))
        });
        let error = result.err().ok_or("partial write accepted")?;
        assert!(error.contains("injected write failure"), "{error}");
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&root).map_err(|e| e.to_string())?.count(), 0);
        publish_receipt_with(&output, b"complete", |file, bytes| file.write_all(bytes))?;
        assert_eq!(fs::read(&output).map_err(|e| e.to_string())?, b"complete");
        fs::remove_file(&output).map_err(|e| e.to_string())?;
        fs::remove_dir(&root).map_err(|e| e.to_string())?;
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
        let mut malformed_digest = receipt();
        malformed_digest.cargo_version_sha256 = "not-a-digest".into();
        assert!(
            validate_binding(
                &text(&malformed_digest)?,
                &context(),
                &source(),
                2,
                "success",
                2
            )
            .is_err()
        );
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
        let mut duplicate_row = receipt();
        duplicate_row.rows.push(Row {
            line: 1,
            command: "cargo\ttest  -p probe exact".into(),
            declared_host: "all".into(),
            selected: Some(vec!["tests::other".into()]),
        });
        assert!(
            validate_binding(
                &text(&duplicate_row)?,
                &context(),
                &source(),
                2,
                "success",
                2
            )
            .is_err()
        );
        duplicate_row.rows[1].line = 2;
        validate_binding(
            &text(&duplicate_row)?,
            &context(),
            &source(),
            2,
            "success",
            2,
        )?;
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
        let mut same = observation(1, vec!["b"]);
        same.selector.command = "cargo\ttest  -p probe".into();
        assert!(rows(vec![observation(1, vec!["a"]), same]).is_err());
        let mut different = observation(2, vec!["a"]);
        different.selector.command = "cargo\ttest  -p probe".into();
        assert_eq!(rows(vec![observation(1, vec!["a"]), different])?.len(), 2);
        Ok(())
    }
}
