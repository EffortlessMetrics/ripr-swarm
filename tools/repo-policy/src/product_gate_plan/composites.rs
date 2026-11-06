//! Existing composite producers, subordinate to the canonical gate rows.
//!
//! Commands and artifact edges are ordered parts of one result, never separate
//! receipts. This is a source model, not an executor, selector or authenticator.
use super::ProductGateId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    PullRequest,
    MergeGroup,
    Other,
}

impl Event {
    pub const fn has_pr_evidence(self) -> bool {
        matches!(self, Self::PullRequest | Self::MergeGroup)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepOwner {
    pub name: &'static str,
    /// Existing user-text steps have no Actions id. Do not invent one or infer
    /// cross-tree execution from the enclosing job's successful outcome.
    pub id: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositeProducer {
    BaseUserText,
    PrEvidence,
    SchemaSweep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositeSubject {
    FirstParentMatcherAgainstCheckout,
    /// Merge-group admission supplies B -> G. Retained PR callers supply B -> H.
    AdmittedComparison,
    CheckoutWithEventDependentEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactEdge {
    pub path: &'static str,
    pub producer: ProductGateId,
    /// Index in the producer's ordered command list (not a receipt identity).
    pub produced_by: usize,
    pub consumed_by: usize,
    pub schema: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositeContract {
    pub steps: Vec<StepOwner>,
    pub subject: CompositeSubject,
    pub pr_events_only: bool,
    pub commands: Vec<&'static str>,
    pub artifacts: Vec<ArtifactEdge>,
}

impl CompositeContract {
    pub fn result_owner(&self) -> Option<StepOwner> {
        self.steps.last().copied()
    }
}

/// The only permitted compiler recovery. The original matcher is never replaced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinnedRecovery {
    pub path: &'static str,
    pub before_blob: &'static str,
    pub after_blob: &'static str,
}

pub const BASE_RECOVERY: PinnedRecovery = PinnedRecovery {
    path: "crates/ripr/src/app/test_stub.rs",
    before_blob: "41ce81981802365b31e5182b7a7a51540ff8d7be",
    after_blob: "e934320980555de2d431b2aa8482a6059e9cf7f7",
};

pub const PR_COMMANDS: [&str; 10] = [
    "cargo xtask ripr-pr --base \"$BASE_SHA\" --head \"$HEAD_SHA\"",
    "cargo xtask ripr-pr --base \"$BASE_SHA\" --head \"$HEAD_SHA\" --check",
    "cargo xtask ripr-review-comments --base \"$BASE_SHA\" --head \"$HEAD_SHA\" --check-output target/ripr/pr/check.json",
    "cargo xtask ripr-review-comments --base \"$BASE_SHA\" --head \"$HEAD_SHA\" --check --check-output target/ripr/pr/check.json",
    "cargo xtask impacted-evidence",
    "cargo xtask impacted-evidence --check",
    "cargo xtask ripr-pr-summary",
    "cargo xtask ripr-pr-summary --check",
    "cargo xtask ripr-annotations",
    "cargo xtask ripr-annotations --check",
];

impl CompositeProducer {
    pub fn contract(self, event: Event) -> CompositeContract {
        match self {
            Self::BaseUserText => CompositeContract {
                steps: [
                    "Prepare independent user-text base",
                    "Recover pinned base compiler arity if needed",
                    "User-text goldens judged by the merge-base matcher",
                ]
                .into_iter()
                .map(|name| StepOwner { name, id: None })
                .collect(),
                subject: CompositeSubject::FirstParentMatcherAgainstCheckout,
                pr_events_only: true,
                commands: vec!["cargo xtask check-user-text --tree \"$GITHUB_WORKSPACE\""],
                artifacts: vec![],
            },
            Self::PrEvidence => CompositeContract {
                steps: vec![StepOwner {
                    name: "PR evidence",
                    id: Some("pr_evidence"),
                }],
                subject: CompositeSubject::AdmittedComparison,
                pr_events_only: true,
                commands: PR_COMMANDS.to_vec(),
                // Explicit same-run check-output edges in the shell. The
                // generate/check pairs remain ordered within this one owner;
                // this does not claim every internal xtask file read is modeled.
                artifacts: [2, 3]
                    .into_iter()
                    .map(|consumed_by| ArtifactEdge {
                        path: "target/ripr/pr/check.json",
                        producer: ProductGateId::PrEvidence,
                        produced_by: 0,
                        consumed_by,
                        schema: None,
                    })
                    .collect(),
            },
            Self::SchemaSweep => {
                let mut artifacts = vec![ArtifactEdge {
                    path: "target/ripr/sweep/doctor.json",
                    producer: ProductGateId::SchemaProducerSweep,
                    produced_by: 0,
                    consumed_by: 1,
                    schema: Some("schemas/ripr/doctor.schema.json"),
                }];
                if event.has_pr_evidence() {
                    artifacts.extend([
                        ArtifactEdge {
                            path: "target/ripr/pr/repo-exposure.json",
                            producer: ProductGateId::PrEvidence,
                            produced_by: 0,
                            consumed_by: 1,
                            schema: Some("schemas/ripr/pr-evidence.schema.json"),
                        },
                        ArtifactEdge {
                            path: "target/ripr/review/comments.json",
                            producer: ProductGateId::PrEvidence,
                            produced_by: 2,
                            consumed_by: 1,
                            schema: Some("schemas/ripr/review-comments.schema.json"),
                        },
                    ]);
                }
                CompositeContract {
                    steps: vec![StepOwner {
                        name: "Schema producer sweep",
                        id: Some("schema_producer_sweep"),
                    }],
                    subject: CompositeSubject::CheckoutWithEventDependentEvidence,
                    pr_events_only: false,
                    commands: vec![
                        "cargo run --quiet -p ripr -- doctor --json > target/ripr/sweep/doctor.json",
                        "cargo xtask schema-producer-sweep \"${artifacts[@]}\"",
                    ],
                    artifacts,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositeResult {
    Executed,
    /// A successful shell exit on an old base is only this weaker observation.
    MatcherPinOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactBinding {
    pub edge: ArtifactEdge,
    pub run_id: u64,
    pub run_attempt: u64,
    pub subject_sha: String,
}

/// Unauthenticated observations supplied to the existing structural inspector.
/// Each composite has one owner/result, however many commands it contains.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositeObservation {
    pub owner: StepOwner,
    pub completed_steps: Vec<StepOwner>,
    pub commands: Vec<String>,
    pub base_sha: String,
    pub target_sha: String,
    pub matcher_sha: Option<String>,
    pub supports_cross_tree: Option<bool>,
    pub recovery: Option<PinnedRecovery>,
    pub artifacts: Vec<ArtifactBinding>,
    pub result: CompositeResult,
}
