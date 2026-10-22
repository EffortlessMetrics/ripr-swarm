//! One DTO for the termination qualification packet (#4505). JSON and Markdown
//! are both rendered from [`Packet`]; neither re-derives a state.

use serde::Serialize;

pub(crate) const SCHEMA: &str = "ripr-termination-qualification-v1";
pub(crate) const CORPUS_SCHEMA: &str = "ripr-termination-corpus-v1";

/// The #4505 row taxonomy. Only the first two are passing product results.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RowResult {
    PassedSupportedAnalysis,
    PassedTypedLimitation,
    SemanticFailure,
    ProcessAbort,
    NonterminalDeadline,
    MissingOrFalseCleanLimitation,
    WrongSubject,
    InstrumentFailure,
    NotRun,
}

impl RowResult {
    pub(crate) fn is_pass(self) -> bool {
        matches!(
            self,
            Self::PassedSupportedAnalysis | Self::PassedTypedLimitation
        )
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::PassedSupportedAnalysis => "passed_supported_analysis",
            Self::PassedTypedLimitation => "passed_typed_limitation",
            Self::SemanticFailure => "semantic_failure",
            Self::ProcessAbort => "process_abort",
            Self::NonterminalDeadline => "nonterminal_deadline",
            Self::MissingOrFalseCleanLimitation => "missing_or_false_clean_limitation",
            Self::WrongSubject => "wrong_subject",
            Self::InstrumentFailure => "instrument_failure",
            Self::NotRun => "not_run",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Family {
    RustNesting,
    PythonNesting,
    PathGlobSuppression,
    PathGlobGeneratedFile,
}

impl Family {
    pub(crate) const ALL: [Family; 4] = [
        Family::RustNesting,
        Family::PythonNesting,
        Family::PathGlobSuppression,
        Family::PathGlobGeneratedFile,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::RustNesting => "rust_nesting",
            Self::PythonNesting => "python_nesting",
            Self::PathGlobSuppression => "path_glob_suppression",
            Self::PathGlobGeneratedFile => "path_glob_generated_file",
        }
    }
}

/// What a case is for. `Discriminator` rows are the ones a removal control
/// must turn red.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Role {
    /// Ordinary or deep-but-supported input: full analysis, no new limitation.
    Control,
    /// Previously aborted or hung; must end in a typed limitation (or, for
    /// globs, an unsuppressed/unexcluded nonmatch).
    Discriminator,
    /// Comment/string text that looks like the hazard and must not consume the
    /// budget.
    Lookalike,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct InputDigest {
    pub(crate) path: String,
    pub(crate) bytes: usize,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Subjects {
    /// Files placed in the workspace.
    pub(crate) workspace_files: usize,
    /// Files outside the diff (the hostile or unchanged neighbour).
    pub(crate) unchanged_outside_diff: usize,
    /// Findings the run reported for the changed file.
    pub(crate) findings_reported: usize,
    /// Findings the run reported for the changed file after suppression.
    pub(crate) findings_suppressed: usize,
    /// Files named by a typed limitation.
    pub(crate) declined_files: usize,
}

/// Semantic part of a row: everything that feeds the packet digest.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) family: Family,
    pub(crate) role: Role,
    pub(crate) shape: String,
    pub(crate) expected: String,
    pub(crate) result: RowResult,
    pub(crate) detail: String,
    pub(crate) argv: Vec<String>,
    pub(crate) cwd: String,
    pub(crate) deadline_seconds: u64,
    pub(crate) termination: String,
    pub(crate) limitation: Option<String>,
    pub(crate) inputs: Vec<InputDigest>,
    pub(crate) subjects: Subjects,
    pub(crate) stdout_sha256: String,
    pub(crate) stderr_sha256: String,
    pub(crate) artifact_sha256: Option<String>,
    pub(crate) cleanup: String,
}

/// Observations that vary run to run. They never feed the packet digest.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Observation {
    pub(crate) id: String,
    pub(crate) elapsed_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BinaryIdentity {
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
    pub(crate) reported_version: String,
    pub(crate) path_resolution: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct HostIdentity {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) family: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ControlKind {
    /// A stub binary the harness must classify as a failure. Proves the
    /// classifier discriminates without a second product build.
    Instrument,
    /// The real pre-repair or budget-disabled binary run over the corpus.
    Removal,
    /// The unchanged binary run over the corpus a second time.
    Repeat,
}

/// A negative control: `expected` is the failing state the control must reach.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Control {
    pub(crate) id: String,
    pub(crate) kind: ControlKind,
    pub(crate) family: Option<Family>,
    pub(crate) expected: String,
    /// `None` means the control did not run.
    pub(crate) observed: Option<String>,
    pub(crate) discriminated: bool,
    pub(crate) detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Aggregate {
    /// Every corpus row passed, every family executed, every control
    /// discriminated.
    Passed,
    /// A required row or control failed.
    Failed,
    /// Corpus rows passed but real removal controls were not supplied.
    Incomplete,
}

impl Aggregate {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FamilyCount {
    pub(crate) family: Family,
    pub(crate) selected: usize,
    pub(crate) executed: usize,
    pub(crate) passed: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Packet {
    pub(crate) schema: &'static str,
    pub(crate) corpus_schema: &'static str,
    pub(crate) binary: BinaryIdentity,
    pub(crate) host: HostIdentity,
    pub(crate) environment_boundary: Vec<String>,
    pub(crate) families: Vec<FamilyCount>,
    pub(crate) rows: Vec<Row>,
    pub(crate) controls: Vec<Control>,
    pub(crate) aggregate: Aggregate,
    pub(crate) aggregate_reasons: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    /// SHA-256 over the semantic packet (everything except `observations`
    /// and `digest` itself).
    pub(crate) digest: String,
    pub(crate) observations: Vec<Observation>,
}

pub(crate) fn render_json(packet: &Packet) -> Result<String, String> {
    serde_json::to_string_pretty(packet)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| format!("failed to render termination qualification JSON: {error}"))
}

pub(crate) fn render_markdown(packet: &Packet) -> String {
    let mut lines = vec![
        "# Termination qualification".to_string(),
        String::new(),
        format!("Aggregate: **{}**", packet.aggregate.as_str()),
        format!("Packet digest: `{}`", packet.digest),
        format!(
            "Binary: `{}` sha256 `{}` ({} bytes, `{}`)",
            packet.binary.path,
            packet.binary.sha256,
            packet.binary.bytes,
            packet.binary.reported_version
        ),
        format!("Binary resolution: {}", packet.binary.path_resolution),
        format!(
            "Host: {} {} ({})",
            packet.host.os, packet.host.arch, packet.host.family
        ),
        String::new(),
    ];
    if !packet.aggregate_reasons.is_empty() {
        lines.push("Aggregate reasons:".to_string());
        for reason in &packet.aggregate_reasons {
            lines.push(format!("- {reason}"));
        }
        lines.push(String::new());
    }
    lines.push("## Families".to_string());
    lines.push(String::new());
    lines.push("| Family | Selected | Executed | Passed |".to_string());
    lines.push("|---|---:|---:|---:|".to_string());
    for count in &packet.families {
        lines.push(format!(
            "| {} | {} | {} | {} |",
            count.family.as_str(),
            count.selected,
            count.executed,
            count.passed
        ));
    }
    lines.push(String::new());
    lines.push("## Rows".to_string());
    lines.push(String::new());
    lines.push("| Case | Role | Result | Termination | Declined | Findings | Detail |".to_string());
    lines.push("|---|---|---|---|---:|---:|---|".to_string());
    for row in &packet.rows {
        lines.push(format!(
            "| `{}` | {} | {} | {} | {} | {} | {} |",
            row.id,
            role_label(row.role),
            row.result.as_str(),
            row.termination,
            row.subjects.declined_files,
            row.subjects.findings_reported,
            row.detail.replace('|', "\\|")
        ));
    }
    lines.push(String::new());
    lines.push("## Controls".to_string());
    lines.push(String::new());
    lines.push("| Control | Kind | Expected | Observed | Discriminated |".to_string());
    lines.push("|---|---|---|---|---|".to_string());
    for control in &packet.controls {
        lines.push(format!(
            "| `{}` | {} | {} | {} | {} |",
            control.id,
            match control.kind {
                ControlKind::Instrument => "instrument",
                ControlKind::Removal => "removal",
                ControlKind::Repeat => "repeat",
            },
            control.expected,
            control.observed.as_deref().unwrap_or("not_run"),
            if control.discriminated { "yes" } else { "no" }
        ));
    }
    lines.push(String::new());
    lines.push("## Environment boundary".to_string());
    lines.push(String::new());
    for item in &packet.environment_boundary {
        lines.push(format!("- {item}"));
    }
    lines.push(String::new());
    lines.push("## Non-claims".to_string());
    lines.push(String::new());
    for item in &packet.non_claims {
        lines.push(format!("- {item}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn role_label(role: Role) -> &'static str {
    match role {
        Role::Control => "control",
        Role::Discriminator => "discriminator",
        Role::Lookalike => "lookalike",
    }
}
