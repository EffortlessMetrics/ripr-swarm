//! Same-run analysis identity carried by the review-comments receipt.
//!
//! ripr's producer-admission flow (`admit_producer_evidence`, `admit_review_input`) is not
//! ported: swarm's own outcome loader owns that decision (see the #12009 residuals). Only the
//! identity shape the receipt schema 0.2 records is kept here.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewAnalysisIdentity {
    pub(crate) schema_version: String,
    pub(crate) repository_identity: String,
    pub(crate) root: String,
    pub(crate) base_sha: String,
    pub(crate) head_sha: String,
    pub(crate) head_tree: String,
    pub(crate) canonical_diff_sha256: String,
    pub(crate) mode: String,
    pub(crate) configuration_fingerprint: String,
    pub(crate) producer_schema: String,
    pub(crate) analyzer_generation: String,
    pub(crate) check_sha256: String,
}
