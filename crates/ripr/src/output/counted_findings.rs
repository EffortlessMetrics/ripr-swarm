//! Finding-origin dispositions. Independent seam guidance is never a join source.
use crate::app::CheckOutput;
use crate::config::RiprConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const FIELD: &str = "counted_finding_dispositions";
const BASIS: &str = "complete_raw_findings_before_consumer_filters";
const MAX_CONTRACT_INPUT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    producer_version: String,
    producer_commit: String,
    producer_identity: String,
    cargo_input_identity: String,
    environment_identity: String,
    rust_sources: BTreeMap<String, String>,
    head_commit: String,
    base_commit: String,
    diff_base_commit: String,
    diff_sha256: String,
    analysis_config_identity: String,
    count_policy_identity: String,
    mode: String,
    languages: Vec<String>,
    include_unchanged_tests: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Disposition {
    finding_id: String,
    classification: String,
    status: String,
    gap_state: String,
    reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    schema_version: u32,
    status: String,
    basis: String,
    analysis_binding: Binding,
    findings_sha256: String,
    dispositions: Vec<Disposition>,
}

fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn value_digest(value: &Value) -> String {
    digest(value.to_string().as_bytes())
}

// This policy identity is deliberately separate from the finding-affecting
// identity, which excludes severity overrides. Its representation is bound to
// the exact clean producer build, not a cross-version serialization promise.
fn count_policy_identity(config: &RiprConfig) -> String {
    digest(format!("{:?}|{:?}", config.severity, config.overrides).as_bytes())
}

fn build_commit() -> Option<&'static str> {
    let commit = env!("RIPR_BUILD_COMMIT");
    (env!("RIPR_BUILD_COMMIT_DIRTY") == "false"
        && matches!(commit.len(), 40 | 64)
        && commit.bytes().all(|b| b.is_ascii_hexdigit()))
    .then_some(commit)
}

fn git_budget() -> std::time::Duration {
    crate::analysis::cancellation::remaining_budget()
        .map_or(std::time::Duration::from_secs(5), |remaining| {
            remaining.min(std::time::Duration::from_secs(5))
        })
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = crate::git::run_git_output_with_deadline_and_limit(
        root,
        args,
        git_budget(),
        MAX_CONTRACT_INPUT_BYTES as usize,
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim_end_matches('\n').to_string())
}

fn bound_workspace_inputs(root: &Path) -> Option<()> {
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if root.canonicalize().ok()? != Path::new(&top).canonicalize().ok()? {
        return None;
    }
    let manifest_path = root.join("Cargo.toml");
    if !std::fs::symlink_metadata(&manifest_path)
        .ok()?
        .file_type()
        .is_file()
    {
        return None;
    }
    let manifest: toml::Value = toml::from_str(
        &crate::bounded_input::read_to_string_with_limit(&manifest_path, MAX_CONTRACT_INPUT_BYTES)
            .ok()?,
    )
    .ok()?;
    if manifest
        .get("package")
        .and_then(|p| p.get("workspace"))
        .is_some()
    {
        return None;
    }
    if manifest.get("workspace").is_none()
        && root
            .canonicalize()
            .ok()?
            .ancestors()
            .skip(1)
            .any(|parent| parent.join("Cargo.toml").exists())
    {
        return None;
    }
    if let Some(workspace) = manifest.get("workspace") {
        for field in ["members", "default-members", "exclude"] {
            if let Some(members) = workspace.get(field) {
                for member in members.as_array()? {
                    if Path::new(member.as_str()?).components().any(|component| {
                        !matches!(
                            component,
                            std::path::Component::Normal(_) | std::path::Component::CurDir
                        )
                    }) {
                        return None;
                    }
                }
            }
        }
    }
    // Git status does not expose assume-unchanged/skip-worktree modifications.
    // Refuse those index modes, rather than assuming their files match HEAD.
    let tracked = git(root, &["ls-files", "-v", "-z"])?;
    if tracked
        .split('\0')
        .filter(|s| !s.is_empty())
        .any(|s| !s.starts_with("H "))
    {
        return None;
    }
    // Gitlinks and symlinks can hide mutable inputs beyond this commit.
    let index = git(root, &["ls-files", "--stage", "-z"])?;
    if index
        .split('\0')
        .filter(|s| !s.is_empty())
        .any(|s| !s.starts_with("100644 ") && !s.starts_with("100755 "))
    {
        return None;
    }
    // Auxiliary module reads can use arbitrary extensions. Refuse every
    // ignored file outside the root Cargo target directory. The producer's
    // existing input Boundary separately refuses reading that directory.
    let ignored = git(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    for path in ignored.split('\0').filter(|s| !s.is_empty()) {
        if !path.starts_with("target/") {
            return None;
        }
    }
    Some(())
}

fn current_subject(root: &Path, base: &str, head: &str, binding: &Binding) -> Option<()> {
    bound_workspace_inputs(root)?;
    if binding.environment_identity != crate::config::counted_environment_identity() {
        return None;
    }
    if crate::analysis::cargo_input_identity(root).as_ref() != Some(&binding.cargo_input_identity)
        || binding.rust_sources.is_empty()
        || binding.rust_sources.len() > 4096
    {
        return None;
    }
    let canonical_root = root.canonicalize().ok()?;
    let mut remaining = 256 * 1024 * 1024_u64;
    for (relative, expected) in &binding.rust_sources {
        crate::analysis::cancellation::checkpoint().ok()?;
        if relative.contains('\\')
            || !relative.ends_with(".rs")
            || Path::new(relative)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return None;
        }
        let path = root.join(relative);
        if !std::fs::symlink_metadata(&path).ok()?.file_type().is_file() {
            return None;
        }
        let path = path.canonicalize().ok()?;
        if !path.starts_with(&canonical_root) {
            return None;
        }
        let bytes =
            crate::bounded_input::read_with_limit(&path, remaining.min(MAX_CONTRACT_INPUT_BYTES))
                .ok()?;
        remaining = remaining.checked_sub(u64::try_from(bytes.len()).ok()?)?;
        if digest(&bytes) != *expected {
            return None;
        }
    }
    if !git(root, &["status", "--porcelain", "--untracked-files=normal"])?.is_empty()
        || git(
            root,
            &["rev-parse", "--verify", &format!("{head}^{{commit}}")],
        )? != binding.head_commit
        || git(root, &["rev-parse", "--verify", "HEAD^{commit}"])? != binding.head_commit
        || git(
            root,
            &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
        )? != binding.base_commit
        || git(root, &["merge-base", base, head])? != binding.diff_base_commit
    {
        return None;
    }
    let diff = crate::analysis::load_diff_range_with_deadline_and_git_stdout_limit_core(
        root,
        &binding.base_commit,
        &binding.head_commit,
        Some(git_budget()),
        Some(MAX_CONTRACT_INPUT_BYTES as usize),
    )
    .ok()?;
    if digest(diff.as_bytes()) != binding.diff_sha256 {
        return None;
    }
    Some(())
}

/// One row per individual raw Finding.id, never a canonical-gap or probe-ID
/// aggregation. Unknown classes invalidate the complete manifest. Unsupported
/// actionability stays explicitly unjoinable rather than receiving static credit.
fn dispositions(findings: &Value) -> Option<Vec<Disposition>> {
    let mut seen = BTreeSet::new();
    findings
        .as_array()?
        .iter()
        .map(|finding| {
            let id = finding.get("id")?.as_str()?;
            if id.trim().is_empty()
                || !seen.insert(id)
                || finding.get("suppressed").is_some()
                || finding.get("withheld_by").is_some()
                || finding.get("suppressed_by").is_some()
            {
                return None;
            }
            let class = finding.get("classification")?.as_str()?;
            if !matches!(
                class,
                "exposed"
                    | "weakly_exposed"
                    | "reachable_unrevealed"
                    | "no_static_path"
                    | "infection_unknown"
                    | "propagation_unknown"
                    | "static_unknown"
            ) {
                return None;
            }
            let current = finding.get("source_currentness")?.as_str()? == "candidate_current";
            let rust = finding.get("language")?.as_str()? == "rust";
            let related = finding.get("related_tests")?.as_array()?;
            let limit = finding.get("static_limit_kind").and_then(Value::as_str);
            let (status, gap_state, reason) = if !current {
                ("outside_basis", "unknown", "source_not_candidate_current")
            } else if !rust || finding.get("analysis_scope").is_some() {
                ("unjoinable", "unknown", "unsupported_analysis_scope")
            } else if class == "no_static_path"
                && related.is_empty()
                && limit == Some("cross_language_oracle_visibility_unresolved")
            {
                // SPEC-0062's typed Rust producer identifies this exact finding's
                // external oracle boundary. Do not generalize to other limits,
                // weak assertions, or ordinary no_static_path findings.
                (
                    "joined",
                    "static_limitation",
                    "cross_language_oracle_visibility_unresolved",
                )
            } else if limit.is_some()
                || matches!(
                    class,
                    "infection_unknown" | "propagation_unknown" | "static_unknown"
                )
            {
                (
                    "unjoinable",
                    "unknown",
                    "unsupported_or_conflicting_disposition",
                )
            } else if class == "exposed" {
                ("joined", "already_observed", "producer_exposed_finding")
            } else {
                (
                    "joined",
                    "actionable",
                    "producer_gap_without_supported_static_disposition",
                )
            };
            Some(Disposition {
                finding_id: id.to_string(),
                classification: class.to_string(),
                status: status.to_string(),
                gap_state: gap_state.to_string(),
                reason: reason.to_string(),
            })
        })
        .collect()
}

/// Called only after the real check renderer has serialized all raw findings.
/// Ineligible runs omit the optional positive contract: absence grants no credit.
pub(crate) fn append_check(
    json_text: &mut String,
    output: &CheckOutput,
    config: &RiprConfig,
    bounded: bool,
) {
    if let Some(contract) = produce(json_text, output, config, bounded)
        && let Some(end) = json_text.rfind('}')
    {
        let addition = format!(",\n  \"{FIELD}\": {}\n", json!(contract));
        if json_text.len().saturating_add(addition.len()) <= MAX_CONTRACT_INPUT_BYTES as usize {
            json_text.insert_str(end, &addition);
        }
    }
}

fn produce(
    text: &str,
    output: &CheckOutput,
    config: &RiprConfig,
    bounded: bool,
) -> Option<Contract> {
    if text.len() > MAX_CONTRACT_INPUT_BYTES as usize {
        return None;
    }
    let outcome = output.analysis_outcome.as_ref()?;
    let inputs = outcome.check_findings_inputs.as_ref()?;
    let provenance = inputs.counted_provenance.as_ref()?;
    let revisions = output.analyzed_revisions.as_ref()?;
    if bounded
        || !outcome.kind.is_complete()
        || !outcome.limitations.is_empty()
        || output.suppression.is_some()
        || !output.owner_opt_outs.is_empty()
        || output.partial_scope.is_some()
        || !output.language_runs.is_empty()
        || output.no_scope_provided
        || output.unanalyzed_working_tree
        || revisions.working_tree
        || output.summary.conflicting_expressions != 0
        || inputs.perl_facts_used
        || !inputs.include_unchanged_tests
        || inputs.languages != [crate::domain::LanguageId::Rust]
        || inputs.config_identity != crate::config::check_artifact_config_identity_hash(config)
    {
        return None;
    }
    let binding = Binding {
        producer_version: env!("CARGO_PKG_VERSION").to_string(),
        producer_commit: build_commit()?.to_string(),
        producer_identity: crate::build_identity::cache_identity().to_string(),
        cargo_input_identity: provenance.cargo_input_identity.clone(),
        environment_identity: provenance.environment_identity.clone(),
        rust_sources: provenance.rust_sources.clone(),
        head_commit: revisions.head_commit.clone()?,
        base_commit: revisions.base_commit.clone()?,
        diff_base_commit: revisions
            .loaded_diff_origin
            .clone()
            .or_else(|| revisions.merge_base_commit.clone())
            .or_else(|| revisions.base_commit.clone())?,
        diff_sha256: outcome.identity.input_identity.clone()?,
        analysis_config_identity: inputs.config_identity.clone(),
        count_policy_identity: count_policy_identity(config),
        mode: output.mode.as_str().to_string(),
        languages: vec!["rust".to_string()],
        include_unchanged_tests: true,
    };
    current_subject(&output.root, output.base.as_deref()?, "HEAD", &binding)?;
    let document: Value = serde_json::from_str(text).ok()?;
    let findings = document.get("findings")?;
    let rows = dispositions(findings)?;
    if rows.len() != output.findings.len() || rows.len() != output.summary.findings {
        return None;
    }
    let contract = Contract {
        schema_version: 1,
        status: "complete".to_string(),
        basis: BASIS.to_string(),
        analysis_binding: binding.clone(),
        findings_sha256: value_digest(findings),
        dispositions: rows,
    };
    let mut checked = document;
    checked[FIELD] = json!(contract);
    validate(&checked, &binding)
}

/// A complete raw basis is necessary but is NOT the downstream post-filter
/// counted basis. Consumers must validate every counted ID and reconcile their
/// own exclusions, classes and cap before subtracting any static disposition.
fn validate(document: &Value, expected: &Binding) -> Option<Contract> {
    let contract: Contract = serde_json::from_value(document.get(FIELD)?.clone()).ok()?;
    let findings = document.get("findings")?;
    let outcome: crate::analysis_outcome::AnalysisOutcome =
        serde_json::from_value(document.pointer("/analysis_outcome/outcome")?.clone()).ok()?;
    if contract.schema_version != 1
        || contract.status != "complete"
        || contract.basis != BASIS
        || &contract.analysis_binding != expected
        || contract.findings_sha256 != value_digest(findings)
        || contract.dispositions != dispositions(findings)?
        || document.get("tool") != Some(&json!("ripr"))
        || document.get("schema_version") != Some(&json!("0.2"))
        || !outcome.kind.is_complete()
        || !outcome.limitations.is_empty()
        || outcome.counts.finding_count != u64::try_from(contract.dispositions.len()).ok()?
        || document.pointer("/analysis_outcome/analysis_complete") != Some(&json!(true))
        || document.pointer("/analysis_outcome/outcome/identity/input_identity")
            != Some(&json!(expected.diff_sha256))
        || document.pointer("/head/commit") != Some(&json!(expected.head_commit))
        || document.pointer("/head/source") != Some(&json!("commit"))
        || document.get("base_commit") != Some(&json!(expected.base_commit))
        || document.get("mode") != Some(&json!(expected.mode))
        || document.get("suppression_policy").is_some()
        || document.get("owner_opt_outs").is_some()
        || document.get("run_limitations").is_some()
        || document
            .get("document_complete")
            .is_some_and(|v| v != &json!(true))
        || document
            .pointer("/summary/conflicting_expressions")
            .is_some_and(|v| v != &json!(0))
        || document.pointer("/summary/findings")?.as_u64()?
            != u64::try_from(contract.dispositions.len()).ok()?
        || crate::output::gate::discloses_limited_findings_bound(document)
        || crate::output::gate::discloses_bounded_analysis_document(document)
    {
        return None;
    }
    for class in [
        "exposed",
        "weakly_exposed",
        "reachable_unrevealed",
        "no_static_path",
        "infection_unknown",
        "propagation_unknown",
        "static_unknown",
    ] {
        let count = contract
            .dispositions
            .iter()
            .filter(|d| d.classification == class)
            .count();
        if document.get("summary")?.get(class)?.as_u64()? != u64::try_from(count).ok()? {
            return None;
        }
    }
    Some(contract)
}

pub(crate) fn attach_review(
    text: &str,
    check_path: Option<&Path>,
    diff: &str,
    context: &crate::output::review_comments::ReviewCommentsRenderContext<'_>,
) -> Result<String, String> {
    let mut review: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let verified = (|| {
        let check: Value = serde_json::from_str(
            &crate::bounded_input::read_to_string_with_limit(check_path?, MAX_CONTRACT_INPUT_BYTES)
                .ok()?,
        )
        .ok()?;
        if check.get("root") != Some(&json!(crate::output::outcome::display_path(context.root)))
            || check.get("base") != Some(&json!(context.base))
        {
            return None;
        }
        let contract: Contract = serde_json::from_value(check.get(FIELD)?.clone()).ok()?;
        let mut expected = contract.analysis_binding.clone();
        expected.producer_version = env!("CARGO_PKG_VERSION").to_string();
        expected.producer_commit = build_commit()?.to_string();
        expected.producer_identity = crate::build_identity::cache_identity().to_string();
        expected.environment_identity = crate::config::counted_environment_identity();
        expected.cargo_input_identity = crate::analysis::cargo_input_identity(context.root)?;
        expected.diff_sha256 = digest(diff.as_bytes());
        expected.analysis_config_identity =
            crate::config::check_artifact_config_identity_hash(context.config);
        expected.count_policy_identity = count_policy_identity(context.config);
        expected.mode = context.mode.as_str().to_string();
        expected.languages = vec!["rust".to_string()];
        expected.include_unchanged_tests = true;
        current_subject(context.root, context.base, context.head, &expected)?;
        validate(&check, &expected)
    })();
    // Existing independently generated seam cards remain unjoinable, even
    // when this separate finding-origin collection is positively bound.
    review[FIELD] = verified.map_or_else(
        || {
            json!({"schema_version":1,"status":"unjoinable",
        "reason":"missing_or_invalid_producer_basis","dispositions":[]})
        },
        |value| json!(value),
    );
    serde_json::to_string_pretty(&review).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
