//! Report-only parser for `.ripr/tool-capabilities.toml` (#2355, RIPR-SPEC-0377).
//!
//! The parser is deterministic, network-free, and injectable by `as_of`. It
//! does not classify the full command surface or change CI behavior.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{FixKind, PolicyReportSpec, finish_policy_report, write_report};

pub(crate) const DEFAULT_REGISTRY_PATH: &str = ".ripr/tool-capabilities.toml";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const REGISTRY_KIND: &str = "ripr.tool_capabilities";
pub(crate) const REPORT_SCHEMA: &str = "ripr.tool_capabilities_report.v1";
pub(crate) const JSON_REPORT_FILE: &str = "tool-capabilities.json";
pub(crate) const MD_REPORT_FILE: &str = "tool-capabilities.md";

const RULE_SCHEMA: &str = "schema";
const RULE_UNIQUENESS: &str = "uniqueness";
const RULE_STATE_REQUIRED: &str = "state_required";
const RULE_KIND_REQUIRED: &str = "kind_required";
const RULE_DENY_HONESTY: &str = "deny_honesty";
const RULE_EVIDENCE_HONESTY: &str = "evidence_honesty";
const RULE_LIFECYCLE: &str = "lifecycle";
const RULE_ISSUE_REF: &str = "issue_ref";

const USAGE: &str =
    "usage: cargo xtask check-tool-capabilities [--as-of YYYY-MM-DD] [--registry <path>]";

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CapabilityKind {
    Generic,
    Hybrid,
    #[serde(rename = "ripr-specific")]
    RiprSpecific,
    Deprecated,
}

impl CapabilityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Hybrid => "hybrid",
            Self::RiprSpecific => "ripr-specific",
            Self::Deprecated => "deprecated",
        }
    }

    fn needs_destination_contract(self) -> bool {
        matches!(self, Self::Generic | Self::Hybrid)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DecisionKind {
    Observation,
    Policy,
    Proof,
    Action,
    Orchestration,
    Transaction,
}

impl DecisionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Policy => "policy",
            Self::Proof => "proof",
            Self::Action => "action",
            Self::Orchestration => "orchestration",
            Self::Transaction => "transaction",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MigrationState {
    Local,
    Classified,
    DestinationReady,
    DualRunning,
    ParityBlocked,
    CutoverReady,
    CutOver,
    Retired,
    Deprecated,
}

impl MigrationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Classified => "classified",
            Self::DestinationReady => "destination-ready",
            Self::DualRunning => "dual-running",
            Self::ParityBlocked => "parity-blocked",
            Self::CutoverReady => "cutover-ready",
            Self::CutOver => "cut-over",
            Self::Retired => "retired",
            Self::Deprecated => "deprecated",
        }
    }

    fn is_blocking_transition(self) -> bool {
        matches!(self, Self::CutoverReady | Self::CutOver | Self::Retired)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ControlSemantics {
    Deny,
    Warn,
    ReportOnly,
    MetadataOnly,
}

impl ControlSemantics {
    fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Warn => "warn",
            Self::ReportOnly => "report-only",
            Self::MetadataOnly => "metadata-only",
        }
    }

    fn is_non_blocking_evidence(self) -> bool {
        matches!(self, Self::ReportOnly | Self::MetadataOnly)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum LifecycleSemantics {
    HardExpiry,
    ReviewOnly,
}

impl LifecycleSemantics {
    fn as_str(self) -> &'static str {
        match self {
            Self::HardExpiry => "hard-expiry",
            Self::ReviewOnly => "review-only",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExceptionModel {
    None,
    Receipt,
    CountAllowlist,
    Ungoverned,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CountSemantics {
    Exact,
    Advisory,
    NotApplicable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryDocument {
    schema_version: u32,
    kind: String,
    #[serde(default)]
    capability: Vec<CapabilityEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilityEntry {
    pub(crate) id: String,
    pub(crate) claim_id: String,
    pub(crate) kind: CapabilityKind,
    pub(crate) decision_kind: DecisionKind,
    pub(crate) claim: String,
    pub(crate) scanner_scope: String,
    pub(crate) current_authority: String,
    pub(crate) destination_authority: String,
    pub(crate) state: MigrationState,
    #[serde(default)]
    pub(crate) source_commands: Vec<String>,
    #[serde(default)]
    pub(crate) source_paths: Vec<String>,
    #[serde(default)]
    pub(crate) policy_paths: Vec<String>,
    #[serde(default)]
    pub(crate) consumer_surfaces: Vec<String>,
    #[serde(default)]
    pub(crate) rationale_refs: Vec<String>,
    pub(crate) owner: String,
    pub(crate) support_tier: String,
    pub(crate) control_semantics: ControlSemantics,
    pub(crate) exception_model: ExceptionModel,
    pub(crate) lifecycle_semantics: LifecycleSemantics,
    pub(crate) count_semantics: CountSemantics,
    #[serde(default)]
    pub(crate) negative_proof_refs: Vec<String>,
    #[serde(default)]
    pub(crate) required_claim_id: Option<String>,
    #[serde(default)]
    pub(crate) engine_contract: Option<String>,
    #[serde(default)]
    pub(crate) destination_issue: Option<String>,
    #[serde(default)]
    pub(crate) consumer_issue: Option<String>,
    #[serde(default)]
    pub(crate) dual_run_expires: Option<String>,
    #[serde(default)]
    pub(crate) review_after: Option<String>,
    #[serde(default)]
    pub(crate) review_by: Option<String>,
    #[serde(default)]
    pub(crate) comparison_artifact: Option<String>,
    #[serde(default)]
    pub(crate) parity_receipt: Option<String>,
    #[serde(default)]
    pub(crate) currentness_receipt: Option<String>,
    #[serde(default)]
    pub(crate) required_route_receipt: Option<String>,
    #[serde(default)]
    pub(crate) cutover_receipt: Option<String>,
    #[serde(default)]
    pub(crate) deletion_receipt: Option<String>,
    #[serde(default)]
    pub(crate) rollback_receipt: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Outcome {
    pub(crate) diagnostics: Vec<String>,
    pub(crate) entries: Vec<CapabilityEntry>,
    pub(crate) as_of: Option<String>,
    pub(crate) registry_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Options {
    as_of: Option<String>,
    registry: String,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let text = match std::fs::read_to_string(&options.registry) {
        Ok(text) => text,
        Err(err) => {
            let diagnostics = vec![diag(
                RULE_SCHEMA,
                "<document>",
                &format!("{} cannot be read: {err}", options.registry),
            )];
            let outcome = Outcome {
                diagnostics: diagnostics.clone(),
                entries: Vec::new(),
                as_of: options.as_of.clone(),
                registry_path: options.registry.clone(),
            };
            return finish(outcome);
        }
    };
    let outcome = evaluate(&text, options.as_of.as_deref(), &options.registry);
    finish(outcome)
}

fn finish(outcome: Outcome) -> Result<(), String> {
    write_report(JSON_REPORT_FILE, &render_json(&outcome)?)?;
    finish_policy_report(report_spec(), &outcome.diagnostics)
}

fn report_spec() -> PolicyReportSpec<'static> {
    PolicyReportSpec {
        report_file: MD_REPORT_FILE,
        check: "check-tool-capabilities",
        why_it_matters: "Capability and claim authority needs a versioned, offline registry so control semantics, lifecycle, and evidence class are explicit before later inventory or blocking enforcement.",
        fix_kind: FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Keep capability ID, claim ID, kind, decision kind, and control semantics as separate fields.",
            "Use dual_run_expires for hard invalidation and review_after/review_by for warning-only dates.",
            "Give deny controls a test: or receipt: negative proof and a required_claim_id.",
            "Do not treat rationale_refs, documentation, metadata-only, or report-only data as parity, currentness, required-route, cutover, or retirement evidence.",
            "Do not fetch GitHub; issue references are authored syntax only.",
        ],
        rerun_command: "cargo xtask check-tool-capabilities --as-of YYYY-MM-DD",
        exception_template: None,
    }
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut as_of = None;
    let mut registry = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--as-of" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("missing value for --as-of\n{USAGE}"))?;
                validate_iso_date(value)?;
                as_of = Some(value.clone());
            }
            "--registry" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("missing value for --registry\n{USAGE}"))?;
                registry = Some(value.clone());
            }
            other => {
                return Err(format!(
                    "unknown check-tool-capabilities argument `{other}`\n{USAGE}"
                ));
            }
        }
        index += 1;
    }
    Ok(Options {
        as_of,
        registry: registry.unwrap_or_else(|| DEFAULT_REGISTRY_PATH.to_string()),
    })
}

pub(crate) fn evaluate(text: &str, as_of: Option<&str>, registry_path: &str) -> Outcome {
    let mut diagnostics = Vec::new();
    if let Some(as_of) = as_of
        && let Err(error) = validate_iso_date(as_of)
    {
        diagnostics.push(diag(RULE_LIFECYCLE, "<document>", &error));
    }
    match toml::from_str::<RegistryDocument>(text) {
        Ok(document) => {
            diagnostics.extend(validate_document(&document, as_of));
            Outcome {
                diagnostics,
                entries: document.capability,
                as_of: as_of.map(str::to_string),
                registry_path: registry_path.to_string(),
            }
        }
        Err(error) => {
            diagnostics.push(diag(RULE_SCHEMA, "<document>", &error.to_string()));
            Outcome {
                diagnostics,
                entries: Vec::new(),
                as_of: as_of.map(str::to_string),
                registry_path: registry_path.to_string(),
            }
        }
    }
}

fn validate_document(document: &RegistryDocument, as_of: Option<&str>) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if document.schema_version != SCHEMA_VERSION {
        diagnostics.push(diag(
            RULE_SCHEMA,
            "<document>",
            &format!(
                "unknown schema_version {}; expected {SCHEMA_VERSION}",
                document.schema_version
            ),
        ));
    }
    if document.kind != REGISTRY_KIND {
        diagnostics.push(diag(
            RULE_SCHEMA,
            "<document>",
            &format!(
                "unknown registry kind `{}`; expected `{REGISTRY_KIND}`",
                document.kind
            ),
        ));
    }

    let mut ids = BTreeSet::new();
    let mut claim_ids = BTreeSet::new();
    for entry in &document.capability {
        let subject = if entry.id.trim().is_empty() {
            "<capability>"
        } else {
            entry.id.as_str()
        };
        if entry.id.trim().is_empty() {
            diagnostics.push(diag(RULE_SCHEMA, subject, "id must be nonempty"));
        } else if !ids.insert(entry.id.as_str()) {
            diagnostics.push(diag(
                RULE_UNIQUENESS,
                subject,
                &format!("duplicate capability id `{}`", entry.id),
            ));
        }
        if entry.claim_id.trim().is_empty() {
            diagnostics.push(diag(RULE_SCHEMA, subject, "claim_id must be nonempty"));
        } else if !claim_ids.insert(entry.claim_id.as_str()) {
            diagnostics.push(diag(
                RULE_UNIQUENESS,
                subject,
                &format!("duplicate claim id `{}`", entry.claim_id),
            ));
        }
        diagnostics.extend(validate_entry(entry, as_of));
    }
    diagnostics
}

fn validate_entry(entry: &CapabilityEntry, as_of: Option<&str>) -> Vec<String> {
    let subject = if entry.id.trim().is_empty() {
        "<capability>"
    } else {
        entry.id.as_str()
    };
    let mut diagnostics = Vec::new();

    for (field, value) in [
        ("claim", entry.claim.as_str()),
        ("scanner_scope", entry.scanner_scope.as_str()),
        ("current_authority", entry.current_authority.as_str()),
        (
            "destination_authority",
            entry.destination_authority.as_str(),
        ),
        ("owner", entry.owner.as_str()),
        ("support_tier", entry.support_tier.as_str()),
    ] {
        if value.trim().is_empty() {
            diagnostics.push(diag(
                RULE_SCHEMA,
                subject,
                &format!("{field} must be nonempty"),
            ));
        }
    }
    if entry.claim.trim() == entry.scanner_scope.trim() && !entry.claim.trim().is_empty() {
        diagnostics.push(diag(
            RULE_EVIDENCE_HONESTY,
            subject,
            "claim and scanner_scope must remain separate fields and must not be the same string",
        ));
    }

    if entry.kind.needs_destination_contract() {
        require_present(
            &mut diagnostics,
            subject,
            RULE_KIND_REQUIRED,
            "engine_contract",
            entry.engine_contract.as_deref(),
        );
        require_present(
            &mut diagnostics,
            subject,
            RULE_KIND_REQUIRED,
            "destination_issue",
            entry.destination_issue.as_deref(),
        );
        require_present(
            &mut diagnostics,
            subject,
            RULE_KIND_REQUIRED,
            "consumer_issue",
            entry.consumer_issue.as_deref(),
        );
    }

    match entry.state {
        MigrationState::DualRunning => {
            require_present(
                &mut diagnostics,
                subject,
                RULE_STATE_REQUIRED,
                "dual_run_expires",
                entry.dual_run_expires.as_deref(),
            );
            require_present(
                &mut diagnostics,
                subject,
                RULE_STATE_REQUIRED,
                "comparison_artifact",
                entry.comparison_artifact.as_deref(),
            );
            if entry.lifecycle_semantics != LifecycleSemantics::HardExpiry {
                diagnostics.push(diag(
                    RULE_LIFECYCLE,
                    subject,
                    "state dual-running requires lifecycle_semantics hard-expiry",
                ));
            }
        }
        MigrationState::CutoverReady => {
            require_execution_receipt(
                &mut diagnostics,
                subject,
                "parity_receipt",
                entry.parity_receipt.as_deref(),
            );
            require_execution_receipt(
                &mut diagnostics,
                subject,
                "currentness_receipt",
                entry.currentness_receipt.as_deref(),
            );
            require_execution_receipt(
                &mut diagnostics,
                subject,
                "required_route_receipt",
                entry.required_route_receipt.as_deref(),
            );
        }
        MigrationState::CutOver => {
            require_execution_receipt(
                &mut diagnostics,
                subject,
                "cutover_receipt",
                entry.cutover_receipt.as_deref(),
            );
        }
        MigrationState::Retired => {
            require_execution_receipt(
                &mut diagnostics,
                subject,
                "deletion_receipt",
                entry.deletion_receipt.as_deref(),
            );
        }
        MigrationState::Local
        | MigrationState::Classified
        | MigrationState::DestinationReady
        | MigrationState::ParityBlocked
        | MigrationState::Deprecated => {}
    }

    if entry.control_semantics == ControlSemantics::Deny {
        if !has_execution_proof(&entry.negative_proof_refs) {
            diagnostics.push(diag(
                RULE_DENY_HONESTY,
                subject,
                "control_semantics deny requires a named negative proof with a test: or receipt: prefix",
            ));
        }
        if !present(entry.required_claim_id.as_deref()) {
            diagnostics.push(diag(
                RULE_DENY_HONESTY,
                subject,
                "control_semantics deny requires required_claim_id",
            ));
        }
        for proof in &entry.negative_proof_refs {
            if !is_execution_ref(proof) {
                diagnostics.push(diag(
                    RULE_DENY_HONESTY,
                    subject,
                    &format!(
                        "negative_proof_refs entry `{proof}` is not execution evidence; documentation belongs in rationale_refs"
                    ),
                ));
            }
        }
    }

    if entry.control_semantics.is_non_blocking_evidence() {
        if entry.state.is_blocking_transition() {
            diagnostics.push(diag(
                RULE_EVIDENCE_HONESTY,
                subject,
                &format!(
                    "control_semantics {} cannot satisfy blocking state {}",
                    entry.control_semantics.as_str(),
                    entry.state.as_str()
                ),
            ));
        }
        for (field, value) in [
            ("parity_receipt", entry.parity_receipt.as_deref()),
            ("currentness_receipt", entry.currentness_receipt.as_deref()),
            (
                "required_route_receipt",
                entry.required_route_receipt.as_deref(),
            ),
            ("cutover_receipt", entry.cutover_receipt.as_deref()),
            ("deletion_receipt", entry.deletion_receipt.as_deref()),
        ] {
            if present(value) {
                diagnostics.push(diag(
                    RULE_EVIDENCE_HONESTY,
                    subject,
                    &format!(
                        "control_semantics {} cannot satisfy {field}",
                        entry.control_semantics.as_str()
                    ),
                ));
            }
        }
    }

    if entry.lifecycle_semantics == LifecycleSemantics::ReviewOnly
        && present(entry.dual_run_expires.as_deref())
    {
        diagnostics.push(diag(
            RULE_LIFECYCLE,
            subject,
            "review-only lifecycle cannot use dual_run_expires; use review_after or review_by",
        ));
    }

    validate_optional_date(
        &mut diagnostics,
        subject,
        "dual_run_expires",
        entry.dual_run_expires.as_deref(),
    );
    validate_optional_date(
        &mut diagnostics,
        subject,
        "review_after",
        entry.review_after.as_deref(),
    );
    validate_optional_date(
        &mut diagnostics,
        subject,
        "review_by",
        entry.review_by.as_deref(),
    );

    if let (Some(as_of), Some(expires)) = (as_of, entry.dual_run_expires.as_deref())
        && validate_iso_date(as_of).is_ok()
        && validate_iso_date(expires).is_ok()
        && as_of > expires
    {
        diagnostics.push(diag(
            RULE_LIFECYCLE,
            subject,
            &format!("dual run expired as of {as_of} (dual_run_expires {expires})"),
        ));
    }

    validate_issue_ref(
        &mut diagnostics,
        subject,
        "destination_issue",
        entry.destination_issue.as_deref(),
    );
    validate_issue_ref(
        &mut diagnostics,
        subject,
        "consumer_issue",
        entry.consumer_issue.as_deref(),
    );
    diagnostics
}

fn require_present(
    diagnostics: &mut Vec<String>,
    subject: &str,
    rule: &str,
    field: &str,
    value: Option<&str>,
) {
    if !present(value) {
        diagnostics.push(diag(
            rule,
            subject,
            &format!("missing state-required field {field}"),
        ));
    }
}

fn require_execution_receipt(
    diagnostics: &mut Vec<String>,
    subject: &str,
    field: &str,
    value: Option<&str>,
) {
    match value.map(str::trim) {
        None | Some("") => diagnostics.push(diag(
            RULE_STATE_REQUIRED,
            subject,
            &format!("missing state-required field {field}"),
        )),
        Some(value) if !is_execution_ref(value) => diagnostics.push(diag(
            RULE_EVIDENCE_HONESTY,
            subject,
            &format!("{field} `{value}` is not execution evidence"),
        )),
        Some(_) => {}
    }
}

fn validate_optional_date(
    diagnostics: &mut Vec<String>,
    subject: &str,
    field: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty())
        && let Err(error) = validate_iso_date(value)
    {
        diagnostics.push(diag(RULE_LIFECYCLE, subject, &format!("{field} {error}")));
    }
}

fn validate_issue_ref(
    diagnostics: &mut Vec<String>,
    subject: &str,
    field: &str,
    value: Option<&str>,
) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    if !is_authored_issue_ref(value) {
        diagnostics.push(diag(
            RULE_ISSUE_REF,
            subject,
            &format!(
                "{field} `{value}` must be authored owner/repo#n syntax; live GitHub state is not registry truth"
            ),
        ));
    }
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn has_execution_proof(refs: &[String]) -> bool {
    refs.iter().any(|value| is_execution_ref(value))
}

fn is_execution_ref(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("test:") || trimmed.starts_with("receipt:")
}

fn is_authored_issue_ref(value: &str) -> bool {
    let Some((repo, number)) = value.split_once('#') else {
        return false;
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && !owner.contains('/')
        && !name.contains('/')
        && !value.contains("://")
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && !number.is_empty()
}

pub(crate) fn validate_iso_date(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(format!("expected YYYY-MM-DD date, got `{value}`"));
    }
    let year = value[0..4]
        .parse::<i32>()
        .map_err(|error| format!("invalid year in date `{value}`: {error}"))?;
    let month = value[5..7]
        .parse::<u32>()
        .map_err(|error| format!("invalid month in date `{value}`: {error}"))?;
    let day = value[8..10]
        .parse::<u32>()
        .map_err(|error| format!("invalid day in date `{value}`: {error}"))?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if year < 1 || max_day == 0 || day == 0 || day > max_day {
        return Err(format!("not a valid calendar date: `{value}`"));
    }
    Ok(())
}

fn diag(rule: &str, subject: &str, message: &str) -> String {
    format!("{subject} [{rule}] {message}")
}

pub(crate) fn render_json(outcome: &Outcome) -> Result<String, String> {
    let mut summaries = outcome
        .entries
        .iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "claim_id": entry.claim_id,
                "kind": entry.kind.as_str(),
                "decision_kind": entry.decision_kind.as_str(),
                "state": entry.state.as_str(),
                "control_semantics": entry.control_semantics.as_str(),
                "lifecycle_semantics": entry.lifecycle_semantics.as_str(),
            })
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let value = json!({
        "schema_version": REPORT_SCHEMA,
        "check": "check-tool-capabilities",
        "registry_path": outcome.registry_path,
        "as_of": outcome.as_of,
        "lifecycle_evaluated": outcome.as_of.is_some(),
        "network_access": "none",
        "status": if outcome.diagnostics.is_empty() { "pass" } else { "fail" },
        "entry_count": outcome.entries.len(),
        "entries": summaries,
        "diagnostics": outcome.diagnostics,
        "non_claim": "Report-only schema validation. This parser does not inventory commands, resolve GitHub, or enforce CI.",
    });
    let mut body = serde_json::to_string_pretty(&value)
        .map_err(|err| format!("encode tool-capabilities JSON: {err}"))?;
    body.push('\n');
    Ok(body)
}
