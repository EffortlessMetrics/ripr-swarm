//! B6 edit-cage provenance binding bench oracle.
//!
//! Self-contained stimulus/oracle harness for the `benchmarks/agentic/edit-cage`
//! fixture. It mirrors the product semantics it binds against:
//! `CagePathRule::{Exact, Subtree}` matching (`crates/ripr/src/edit_cage`),
//! `sha256:<hex>` digest rendering (`crates/ripr/src/agent/provenance`), and
//! the `compliant` / `violated` / `incomparable` verdict vocabulary, plus a
//! bench-local `superseded` re-read state for receipts whose after-snapshot a
//! newer capture superseded.
//!
//! The oracle never trusts a stale receipt: every re-read rebinds the policy
//! digest and the snapshot digest, consults an explicit supersession ledger,
//! and fails closed (`incomparable`) on unknown bytes.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MANIFEST_SCHEMA: &str = "ripr-agentic-bench-manifest-v1";
const BENCH_ID: &str = "edit-cage";

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ripr-agentic-bench-cage-{label}-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}

/// `sha256:<hex>` rendering, mirroring `agent::provenance::sha256_file`.
fn sha256_digest(bytes: &[u8]) -> String {
    let sum = Sha256::digest(bytes);
    let mut rendered = String::from("sha256:");
    for byte in sum {
        rendered.push_str(&format!("{byte:02x}"));
    }
    rendered
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(sha256_digest(&bytes))
}

/// Fail-closed repo-relative normalization: rejects absolute paths,
/// backslashes, empty paths, and `.` / `..` components.
fn normalize_repo_path(raw: &str) -> Result<String, String> {
    if raw.is_empty() {
        return Err("path must not be empty".to_string());
    }
    if raw.starts_with('/') || raw.contains('\\') {
        return Err(format!("path escapes the snapshot root: {raw}"));
    }
    let mut parts = Vec::new();
    for component in raw.split('/') {
        if component.is_empty() || component == "." {
            return Err(format!("path has an empty component: {raw}"));
        }
        if component == ".." {
            return Err(format!("path escapes the snapshot root: {raw}"));
        }
        parts.push(component);
    }
    Ok(parts.join("/"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    Exact,
    Subtree,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    path: String,
    scope: Scope,
}

impl Rule {
    /// Mirrors `CagePathRule::matches`: exact equality, or subtree prefix
    /// bounded at a `/` separator so `tests` never matches `tests2/x`.
    fn matches(&self, candidate: &str) -> bool {
        match self.scope {
            Scope::Exact => candidate == self.path,
            Scope::Subtree => {
                candidate == self.path
                    || candidate
                        .strip_prefix(self.path.as_str())
                        .is_some_and(|tail| tail.starts_with('/'))
            }
        }
    }

    fn validate(&mut self) -> Result<(), String> {
        self.path = normalize_repo_path(&self.path)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureRef {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    max_paths: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    bench: String,
    bench_index: String,
    title: String,
    stimulus: String,
    oracle_states: Vec<String>,
    oracle_command: String,
    selected_target: Rule,
    allowed_surface: Vec<Rule>,
    #[serde(default)]
    forbidden_paths: Vec<Rule>,
    fixtures: Vec<FixtureRef>,
    bounds: Bounds,
}

fn manifest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/agentic/edit-cage")
}

fn valid_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].chars().all(|char| char.is_ascii_hexdigit())
}

/// Strict manifest load: unknown fields, schema drift, escaping paths, an
/// empty surface, and malformed digests are all rejected before any receipt
/// is issued.
fn load_manifest(path: &Path) -> Result<Manifest, String> {
    let bytes = fs::read(path).map_err(|error| format!("read manifest: {error}"))?;
    let mut manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse manifest: {error}"))?;
    if manifest.schema_version != MANIFEST_SCHEMA {
        return Err(format!(
            "manifest schema is {}, want {MANIFEST_SCHEMA}",
            manifest.schema_version
        ));
    }
    if manifest.bench != BENCH_ID {
        return Err(format!(
            "manifest bench is {}, want {BENCH_ID}",
            manifest.bench
        ));
    }
    if manifest.bench_index.trim().is_empty()
        || manifest.title.trim().is_empty()
        || manifest.stimulus.trim().is_empty()
        || manifest.oracle_command.trim().is_empty()
    {
        return Err("manifest identity/stimulus/oracle text must be non-empty".to_string());
    }
    if manifest.oracle_states.is_empty() {
        return Err("manifest must declare oracle states".to_string());
    }
    manifest.selected_target.validate()?;
    if manifest.allowed_surface.is_empty() {
        return Err("manifest allowed surface must be non-empty".to_string());
    }
    for rule in &mut manifest.allowed_surface {
        rule.validate()?;
    }
    for rule in &mut manifest.forbidden_paths {
        rule.validate()?;
    }
    if manifest.fixtures.is_empty() {
        return Err("manifest must bind at least one fixture".to_string());
    }
    for fixture in &manifest.fixtures {
        normalize_repo_path(&fixture.path)?;
        if !valid_digest(&fixture.sha256) {
            return Err(format!("fixture has a malformed digest: {}", fixture.path));
        }
    }
    if manifest.bounds.max_paths == 0
        || manifest.bounds.max_file_bytes == 0
        || manifest.bounds.max_total_bytes == 0
    {
        return Err("manifest bounds must be positive".to_string());
    }
    let policy = Policy::from_manifest(&manifest);
    if !policy
        .allowed_surface
        .iter()
        .any(|rule| rule.matches(&policy.selected_target.path))
    {
        return Err("manifest selected target is outside the allowed surface".to_string());
    }
    Ok(manifest)
}

#[derive(Clone, Debug)]
struct Policy {
    selected_target: Rule,
    allowed_surface: Vec<Rule>,
    forbidden_paths: Vec<Rule>,
}

impl Policy {
    fn from_manifest(manifest: &Manifest) -> Self {
        Self {
            selected_target: manifest.selected_target.clone(),
            allowed_surface: manifest.allowed_surface.clone(),
            forbidden_paths: manifest.forbidden_paths.clone(),
        }
    }

    /// Canonical policy rendering bound into every receipt, so a re-read
    /// under a widened surface refuses to confirm an old verdict.
    fn digest(&self) -> String {
        let mut rendered = String::from("b6-policy-v1\n");
        rendered.push_str(&format!("target exact {}\n", self.selected_target.path));
        let mut allowed: Vec<String> = self
            .allowed_surface
            .iter()
            .map(|rule| format!("{:?} {}", rule.scope, rule.path))
            .collect();
        allowed.sort();
        for entry in allowed {
            rendered.push_str(&format!("allow {entry}\n"));
        }
        let mut forbidden: Vec<String> = self
            .forbidden_paths
            .iter()
            .map(|rule| format!("{:?} {}", rule.scope, rule.path))
            .collect();
        forbidden.sort();
        for entry in forbidden {
            rendered.push_str(&format!("forbid {entry}\n"));
        }
        sha256_digest(rendered.as_bytes())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Snapshot {
    digest: String,
    files: BTreeMap<String, String>,
}

impl Snapshot {
    fn digest_of(files: &BTreeMap<String, String>) -> String {
        let mut rendered = String::from("b6-snapshot-v1\n");
        for (path, digest) in files {
            rendered.push_str(path);
            rendered.push('\0');
            rendered.push_str(digest);
            rendered.push('\n');
        }
        sha256_digest(rendered.as_bytes())
    }
}

/// Bounded baseline capture: path count, per-file bytes, and total bytes are
/// all capped, and symlinks or non-regular files fail closed instead of being
/// silently skipped.
fn capture_snapshot(root: &Path, bounds: &Bounds) -> Result<Snapshot, String> {
    let mut files = BTreeMap::new();
    let mut total_bytes: u64 = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|error| format!("list {}: {error}", dir.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("list {}: {error}", dir.display()))?
            .into_iter()
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            let kind = fs::symlink_metadata(&path)
                .map_err(|error| format!("stat {}: {error}", path.display()))?
                .file_type();
            if kind.is_symlink() {
                return Err(format!("symlink refused: {}", path.display()));
            }
            if kind.is_dir() {
                stack.push(path);
                continue;
            }
            if !kind.is_file() {
                return Err(format!("non-regular file refused: {}", path.display()));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| format!("relativize {}: {error}", path.display()))?;
            let name = relative
                .to_str()
                .ok_or_else(|| format!("non-utf8 path: {}", path.display()))?
                .replace('\\', "/");
            let name = normalize_repo_path(&name)?;
            let bytes =
                fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
            if bytes.len() as u64 > bounds.max_file_bytes {
                return Err(format!("file over per-file budget: {name}"));
            }
            total_bytes = total_bytes
                .checked_add(bytes.len() as u64)
                .filter(|total| *total <= bounds.max_total_bytes)
                .ok_or_else(|| "snapshot over total-bytes budget".to_string())?;
            if files.len() >= bounds.max_paths {
                return Err("snapshot over path-count budget".to_string());
            }
            files.insert(name, sha256_digest(&bytes));
        }
    }
    Ok(Snapshot {
        digest: Snapshot::digest_of(&files),
        files,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VerdictState {
    Compliant,
    Violated,
}

impl VerdictState {
    fn as_str(self) -> &'static str {
        match self {
            VerdictState::Compliant => "compliant",
            VerdictState::Violated => "violated",
        }
    }
}

#[derive(Clone, Debug)]
struct Verdict {
    state: VerdictState,
    changed_paths: Vec<String>,
    violations: Vec<String>,
}

/// Pure delta evaluation over two snapshots: forbidden or out-of-surface
/// changes, deletions, and a missing selected-target movement all violate.
fn evaluate(policy: &Policy, before: &Snapshot, after: &Snapshot) -> Verdict {
    let mut paths = BTreeSet::new();
    paths.extend(before.files.keys().cloned());
    paths.extend(after.files.keys().cloned());
    let mut changed_paths = Vec::new();
    let mut violations = Vec::new();
    let mut target_moved = false;
    for path in paths {
        let old = before.files.get(&path);
        let new = after.files.get(&path);
        if old == new {
            continue;
        }
        changed_paths.push(path.clone());
        if policy
            .forbidden_paths
            .iter()
            .any(|rule| rule.matches(&path))
        {
            violations.push(format!("forbidden path changed: {path}"));
        }
        if !policy
            .allowed_surface
            .iter()
            .any(|rule| rule.matches(&path))
        {
            violations.push(format!("change outside the allowed surface: {path}"));
        }
        match (old, new) {
            (Some(_), None) => {
                violations.push(format!("unexpected deletion: {path}"));
            }
            (_, Some(_)) if policy.selected_target.matches(&path) => {
                target_moved = true;
            }
            _ => {}
        }
    }
    if !target_moved {
        violations.push(format!(
            "selected target did not move: {}",
            policy.selected_target.path
        ));
    }
    changed_paths.sort();
    violations.sort();
    violations.dedup();
    Verdict {
        state: if violations.is_empty() {
            VerdictState::Compliant
        } else {
            VerdictState::Violated
        },
        changed_paths,
        violations,
    }
}

#[derive(Clone, Debug)]
struct Receipt {
    before_digest: String,
    after_digest: String,
    policy_digest: String,
    verdict: Verdict,
}

fn issue_receipt(policy: &Policy, before: &Snapshot, after: &Snapshot) -> Receipt {
    Receipt {
        before_digest: before.digest.clone(),
        after_digest: after.digest.clone(),
        policy_digest: policy.digest(),
        verdict: evaluate(policy, before, after),
    }
}

/// Explicit supersession ledger: a re-capture records its predecessor, so a
/// receipt re-read against a newer snapshot reports `superseded` instead of
/// replaying a stale verdict or guessing ancestry.
#[derive(Clone, Debug, Default)]
struct Ledger {
    successors: BTreeMap<String, String>,
}

impl Ledger {
    fn supersede(&mut self, old: &Snapshot, new: &Snapshot) {
        self.successors
            .insert(old.digest.clone(), new.digest.clone());
    }

    fn supersedes(&self, old_digest: &str, current_digest: &str) -> bool {
        let mut cursor = old_digest;
        let mut hops = 0;
        while let Some(next) = self.successors.get(cursor) {
            if next == current_digest {
                return true;
            }
            cursor = next;
            hops += 1;
            if hops > self.successors.len() {
                return false;
            }
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReRead {
    Compliant,
    Violated,
    Incomparable,
    Superseded,
}

impl ReRead {
    fn as_str(self) -> &'static str {
        match self {
            ReRead::Compliant => "compliant",
            ReRead::Violated => "violated",
            ReRead::Incomparable => "incomparable",
            ReRead::Superseded => "superseded",
        }
    }
}

/// Digest-bound re-read: the policy binding and the after-snapshot binding
/// must both hold, else the ledger decides between `superseded` and a
/// fail-closed `incomparable`. Unknown bytes never confirm a stale verdict.
fn reread(receipt: &Receipt, policy: &Policy, current: &Snapshot, ledger: &Ledger) -> ReRead {
    if policy.digest() != receipt.policy_digest {
        return ReRead::Incomparable;
    }
    if current.digest == receipt.after_digest {
        return match receipt.verdict.state {
            VerdictState::Compliant => ReRead::Compliant,
            VerdictState::Violated => ReRead::Violated,
        };
    }
    if ledger.supersedes(&receipt.after_digest, &current.digest) {
        return ReRead::Superseded;
    }
    ReRead::Incomparable
}

/// Materialize the committed `input/` worktree into a temp snapshot root, so
/// `input/src/lib.rs` becomes snapshot path `src/lib.rs`.
fn materialize_worktree(manifest_path: &Path, root: &Path) -> Result<(), String> {
    let input = manifest_path
        .parent()
        .ok_or_else(|| "manifest has no parent directory".to_string())?
        .join("input");
    let mut stack = vec![input.clone()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("list fixture {}: {error}", dir.display()))?
        {
            let entry = entry.map_err(|error| format!("list fixture: {error}"))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(&input)
                .map_err(|error| format!("relativize fixture: {error}"))?;
            if path.is_dir() {
                fs::create_dir_all(root.join(relative))
                    .map_err(|error| format!("create worktree dir: {error}"))?;
                stack.push(path);
            } else {
                let bytes = fs::read(&path).map_err(|error| format!("read fixture: {error}"))?;
                let target = root.join(relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("create worktree dir: {error}"))?;
                }
                fs::write(&target, bytes).map_err(|error| format!("write worktree: {error}"))?;
            }
        }
    }
    Ok(())
}

fn with_worktree(
    label: &str,
    run: impl FnOnce(&Manifest, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let manifest_path = manifest_dir().join("manifest.json");
    let manifest = load_manifest(&manifest_path)?;
    let root = temp_dir(label);
    if root.exists() {
        fs::remove_dir_all(&root).map_err(|error| format!("clear temp root: {error}"))?;
    }
    fs::create_dir_all(&root).map_err(|error| format!("create temp root: {error}"))?;
    materialize_worktree(&manifest_path, &root)?;
    let outcome = run(&manifest, &root);
    let _ = fs::remove_dir_all(&root);
    outcome
}

fn assert_reread(actual: ReRead, expected: ReRead, context: &str) -> Result<(), String> {
    if actual != expected {
        return Err(format!(
            "{context}: re-read is {}, want {}",
            actual.as_str(),
            expected.as_str()
        ));
    }
    Ok(())
}

#[test]
fn b6_manifest_fixture_digests_verify() -> Result<(), String> {
    let dir = manifest_dir();
    let manifest = load_manifest(&dir.join("manifest.json"))?;
    if manifest.bench_index != "B6" {
        return Err(format!("bench index is {}, want B6", manifest.bench_index));
    }
    for state in ["compliant", "violated", "incomparable", "superseded"] {
        if !manifest.oracle_states.iter().any(|entry| entry == state) {
            return Err(format!("oracle states omit {state}"));
        }
    }
    for fixture in &manifest.fixtures {
        let observed = sha256_file(&dir.join(&fixture.path))?;
        if observed != fixture.sha256 {
            return Err(format!(
                "fixture digest drift for {}: manifest {}, worktree {observed}",
                fixture.path, fixture.sha256
            ));
        }
    }
    Ok(())
}

#[test]
fn b6_in_surface_edit_is_compliant_and_digest_bound() -> Result<(), String> {
    with_worktree("in-surface", |manifest, root| {
        let policy = Policy::from_manifest(manifest);
        let ledger = Ledger::default();
        let before = capture_snapshot(root, &manifest.bounds)?;
        let target = root.join("tests/cage_target.rs");
        let mut bytes = fs::read(&target).map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b"\n#[test]\nfn bench_probe() {}\n");
        fs::write(&target, bytes).map_err(|error| error.to_string())?;
        let after = capture_snapshot(root, &manifest.bounds)?;
        if before.digest == after.digest {
            return Err("in-surface edit left the snapshot digest unchanged".to_string());
        }
        let receipt = issue_receipt(&policy, &before, &after);
        if receipt.verdict.state != VerdictState::Compliant {
            return Err(format!(
                "in-surface edit verdict is {}, want compliant: {:?}",
                receipt.verdict.state.as_str(),
                receipt.verdict.violations
            ));
        }
        if receipt.before_digest != before.digest || receipt.after_digest != after.digest {
            return Err("receipt does not bind the captured snapshot digests".to_string());
        }
        assert_reread(
            reread(&receipt, &policy, &after, &ledger),
            ReRead::Compliant,
            "digest-bound re-read",
        )?;
        // A re-read under a widened policy must not confirm the old verdict.
        let mut widened = policy.clone();
        widened.allowed_surface.push(Rule {
            path: "src".to_string(),
            scope: Scope::Subtree,
        });
        assert_reread(
            reread(&receipt, &widened, &after, &ledger),
            ReRead::Incomparable,
            "policy-drift re-read",
        )
    })
}

#[test]
fn b6_out_of_surface_edit_fails_closed() -> Result<(), String> {
    with_worktree("out-surface", |manifest, root| {
        let policy = Policy::from_manifest(manifest);
        let ledger = Ledger::default();
        let before = capture_snapshot(root, &manifest.bounds)?;
        let target = root.join("tests/cage_target.rs");
        let mut bytes = fs::read(&target).map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b"\n#[test]\nfn bench_probe() {}\n");
        fs::write(&target, bytes).map_err(|error| error.to_string())?;
        let outside = root.join("src/lib.rs");
        let mut bytes = fs::read(&outside).map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b"\n// out-of-surface edit\n");
        fs::write(&outside, bytes).map_err(|error| error.to_string())?;
        let after = capture_snapshot(root, &manifest.bounds)?;
        let receipt = issue_receipt(&policy, &before, &after);
        if receipt.verdict.state != VerdictState::Violated {
            return Err(format!(
                "out-of-surface edit verdict is {}, want violated",
                receipt.verdict.state.as_str()
            ));
        }
        if !receipt
            .verdict
            .changed_paths
            .iter()
            .any(|path| path == "src/lib.rs")
        {
            return Err("out-of-surface path missing from the receipt".to_string());
        }
        assert_reread(
            reread(&receipt, &policy, &after, &ledger),
            ReRead::Violated,
            "violated re-read",
        )?;
        // Bytes moved under the receipt with no superseding capture: the
        // re-read must refuse the stale verdict instead of replaying it.
        let mut bytes = fs::read(&outside).map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b"// tampered after receipt\n");
        fs::write(&outside, bytes).map_err(|error| error.to_string())?;
        let tampered = capture_snapshot(root, &manifest.bounds)?;
        assert_reread(
            reread(&receipt, &policy, &tampered, &ledger),
            ReRead::Incomparable,
            "tampered re-read",
        )
    })
}

#[test]
fn b6_superseded_snapshot_never_replays_a_stale_verdict() -> Result<(), String> {
    with_worktree("superseded", |manifest, root| {
        let policy = Policy::from_manifest(manifest);
        let mut ledger = Ledger::default();
        let baseline = capture_snapshot(root, &manifest.bounds)?;
        let target = root.join("tests/cage_target.rs");
        let mut bytes = fs::read(&target).map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b"\n#[test]\nfn bench_probe() {}\n");
        fs::write(&target, bytes).map_err(|error| error.to_string())?;
        let first = capture_snapshot(root, &manifest.bounds)?;
        ledger.supersede(&baseline, &first);
        let receipt = issue_receipt(&policy, &baseline, &first);
        if receipt.verdict.state != VerdictState::Compliant {
            return Err("superseded-scenario receipt is not compliant".to_string());
        }
        let mut bytes = fs::read(&target).map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b"\n#[test]\nfn bench_probe_two() {}\n");
        fs::write(&target, bytes).map_err(|error| error.to_string())?;
        let second = capture_snapshot(root, &manifest.bounds)?;
        ledger.supersede(&first, &second);
        assert_reread(
            reread(&receipt, &policy, &second, &ledger),
            ReRead::Superseded,
            "superseded re-read",
        )?;
        // The ledger records forward succession only: re-reading against the
        // older baseline is unknown direction, so it fails closed.
        assert_reread(
            reread(&receipt, &policy, &baseline, &ledger),
            ReRead::Incomparable,
            "backwards re-read",
        )
    })
}

fn write_case_manifest(label: &str, body: &str) -> Result<PathBuf, String> {
    let dir = temp_dir(label);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("manifest.json");
    fs::write(&path, body).map_err(|error| error.to_string())?;
    Ok(path)
}

fn bench_manifest_case(patch: &str) -> Result<String, String> {
    let bytes =
        fs::read(manifest_dir().join("manifest.json")).map_err(|error| error.to_string())?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let patch: serde_json::Value =
        serde_json::from_str(patch).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "manifest is not an object".to_string())?;
    let patch_object = patch
        .as_object()
        .ok_or_else(|| "patch is not an object".to_string())?;
    for (key, entry) in patch_object {
        if entry.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), entry.clone());
        }
    }
    serde_json::to_string(&value).map_err(|error| error.to_string())
}

#[test]
fn b6_invalid_manifest_is_rejected_before_any_receipt() -> Result<(), String> {
    let cases: Vec<(&str, Result<String, String>)> = vec![
        ("malformed json", Ok("{not json".to_string())),
        (
            "unknown field",
            bench_manifest_case(r#"{"bench_sponsor": "mallory"}"#),
        ),
        (
            "schema drift",
            bench_manifest_case(r#"{"schema_version": "ripr-agentic-bench-manifest-v0"}"#),
        ),
        (
            "wrong bench",
            bench_manifest_case(r#"{"bench": "edit-cage-evil"}"#),
        ),
        (
            "empty surface",
            bench_manifest_case(r#"{"allowed_surface": []}"#),
        ),
        (
            "escaping rule",
            bench_manifest_case(
                r#"{"allowed_surface": [{"path": "../outside", "scope": "subtree"}]}"#,
            ),
        ),
        (
            "target outside surface",
            bench_manifest_case(r#"{"selected_target": {"path": "src/lib.rs", "scope": "exact"}}"#),
        ),
        (
            "malformed digest",
            bench_manifest_case(
                r#"{"fixtures": [{"path": "input/src/lib.rs", "sha256": "nope"}]}"#,
            ),
        ),
        ("no fixtures", bench_manifest_case(r#"{"fixtures": []}"#)),
        (
            "zero bounds",
            bench_manifest_case(
                r#"{"bounds": {"max_paths": 0, "max_file_bytes": 1, "max_total_bytes": 1}}"#,
            ),
        ),
        (
            "missing policy",
            bench_manifest_case(r#"{"selected_target": null}"#),
        ),
    ];
    for (label, body) in cases {
        let body = body?;
        let path = write_case_manifest("invalid-manifest", &body)?;
        let outcome = load_manifest(&path);
        let parent = path.parent().map(Path::to_path_buf);
        let _ = parent.map(fs::remove_dir_all);
        if outcome.is_ok() {
            return Err(format!("invalid manifest accepted: {label}"));
        }
    }
    Ok(())
}

#[test]
fn b6_bounded_capture_fails_closed_over_budget() -> Result<(), String> {
    with_worktree("over-budget", |manifest, root| {
        let tight_paths = Bounds {
            max_paths: 1,
            max_file_bytes: manifest.bounds.max_file_bytes,
            max_total_bytes: manifest.bounds.max_total_bytes,
        };
        if capture_snapshot(root, &tight_paths).is_ok() {
            return Err("path-count budget did not fail closed".to_string());
        }
        let tight_file = Bounds {
            max_paths: manifest.bounds.max_paths,
            max_file_bytes: 1,
            max_total_bytes: manifest.bounds.max_total_bytes,
        };
        if capture_snapshot(root, &tight_file).is_ok() {
            return Err("per-file budget did not fail closed".to_string());
        }
        let tight_total = Bounds {
            max_paths: manifest.bounds.max_paths,
            max_file_bytes: manifest.bounds.max_file_bytes,
            max_total_bytes: 1,
        };
        if capture_snapshot(root, &tight_total).is_ok() {
            return Err("total-bytes budget did not fail closed".to_string());
        }
        // The committed bounds admit the committed worktree: the oracle is
        // bounded but not vacuous.
        let snapshot = capture_snapshot(root, &manifest.bounds)?;
        if snapshot.files.len() != manifest.fixtures.len() {
            return Err(format!(
                "captured {} paths, manifest binds {}",
                snapshot.files.len(),
                manifest.fixtures.len()
            ));
        }
        Ok(())
    })
}
