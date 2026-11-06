//! Observational inventory of the existing CI feature family (#8197).
//!
//! This describes source, not executions. It neither supplies the shared
//! reconciler's expected set nor selects, authenticates, reuses or authorizes
//! any job. In particular, a successful job is not a receipt for every command
//! below, and a generated readiness report is not finalizer authority.

mod inventory;
#[cfg(test)]
mod tests;

pub use inventory::source_inventory;

pub const CI_WORKFLOW: &str = ".github/workflows/ci.yml";
pub const VSCODE_WORKFLOW: &str = ".github/workflows/vscode-e2e.yml";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Authority {
    NotEstablished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Owner {
    pub workflow: &'static str,
    pub job: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandRole {
    Proof,
    Prerequisite,
}

/// Ordered source commands, not independent successful evidence rows. `name`
/// is absent for unnamed steps; no Actions step id is invented for them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    pub name: Option<&'static str>,
    pub run: &'static str,
    pub condition: Option<&'static str>,
    /// Explicit per-step declaration. None retains the workflow's implicit
    /// Bash shell; it does not permit an arbitrary inherited override.
    pub shell: Option<&'static str>,
    pub role: CommandRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Child {
    /// The job whose `needs` result ci-result actually consumes.
    pub caller_job: &'static str,
    pub selection_input: &'static str,
    pub condition: String,
    /// VSCode has a distinct nested workflow/job execution owner.
    pub producer: Owner,
    pub runner: &'static str,
    pub working_directory: &'static str,
    pub commands: Vec<Command>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Upload {
    pub owner: Owner,
    pub step: &'static str,
    pub condition: &'static str,
    pub name: &'static str,
    pub paths: &'static [&'static str],
    pub missing_files: &'static str,
    pub retention_days: u8,
    /// Neither upload has an Actions step id or an exported artifact id/digest.
    pub exports_artifact_identity: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallEdge {
    pub caller: Owner,
    pub callee: Owner,
    pub ref_expression: &'static str,
    pub budget_inputs: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResultEdge {
    pub child_job: &'static str,
    pub env: &'static str,
    pub selection_env: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Aggregate {
    pub owner: Owner,
    pub condition: &'static str,
    pub step: &'static str,
    pub needs: &'static str,
    pub edges: &'static [ResultEdge],
    /// The installed shell checks these exact pairs, not step observations.
    pub accepted_pairs: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gap {
    /// The source model is versioned; actual producer/plan/runtime versions are
    /// not authenticated by it or exported by these leaf jobs.
    ProducerVersionCustody,
    /// source-identity checks admitted source, but commands lack individual
    /// subject/tree, run/attempt, step outcome and observed-test bindings.
    PerCommandExecutionCustody,
    /// Release report schema 0.1 has a package version, not source/run identity.
    /// `warn`/`not_run` can produce successful release-readiness exit status.
    ReadinessIsNotCompleteProof,
    /// Release upload is always-on and missing-file-warn; VSCode diagnostics
    /// are failure-only and missing-file-ignore. Neither is success evidence.
    ArtifactCompletenessAndCustody,
    /// npm -> xtask -> test host has real journey/nonzero-test guards, but no
    /// successful host/cell receipts exported to ci-result. The VSCode binary
    /// is downloaded without a version argument; this is not a pinned matrix.
    NestedVscodeExecutionAndVersion,
    /// Reconciliation accepts caller-declared Features rows without a canonical
    /// feature denominator. This inventory does not change that authority.
    ReconcilerDenominator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    /// Version of this observational format, not a producer version receipt.
    pub version: u32,
    pub authority: Authority,
    pub children: Vec<Child>,
    pub nested_call: CallEdge,
    pub uploads: Vec<Upload>,
    pub aggregate: Aggregate,
    /// `surface.rust` is a local step output, not a job/workflow output.
    pub rust_surface_output: &'static str,
    pub gaps: &'static [Gap],
}
