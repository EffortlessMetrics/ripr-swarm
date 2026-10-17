//! Verification-receipt validation for the governed Python repair-trust
//! corpus — `cargo xtask python-repair-trust check-verification`
//! (RIPR-SPEC-0176, #3570).
//!
//! The crate-side verification phase (#3570) executes one applied attempt's
//! producer-owned verify route through bounded controls, reruns the analysis
//! against the exact post-edit state, and publishes ONE immutable candidate
//! receipt whose execution and static-movement axes stay separate. This
//! validator is the offline promotion-side counterpart: given the accepted
//! selection manifest (#3568) and a set of candidate receipts, it re-checks
//! every digest anchor, the closed vocabularies, and the separation laws the
//! receipts assert, so a promotion pass can read the retained evidence
//! without re-running the driver.
//!
//! What the check enforces, fail closed:
//!
//! - the receipt binds to the ACCEPTED selection manifest by exact-bytes
//!   sha256 and to one accepted selection row by its recomputed canonical
//!   `selection_digest` — a changed manifest or a replaced row makes every
//!   affected receipt stale;
//! - the receipt's native identity (family, owner, discriminator, relation,
//!   oracle) and target agree exactly with the accepted row — a rewritten
//!   copy fails even while the digest anchors stay valid;
//! - the execution axis carries the closed state vocabulary
//!   (`passed`/`failed`/`timed_out`/`cancelled`/`unavailable`/`not_run`/
//!   `invalid`) and each state agrees with the retained process disposition:
//!   `passed` is completed with exit status 0 and full output commitments; a
//!   non-verdict state carries no exit status; timeout, cancellation, spawn
//!   failure, route unavailability, and pre-execution rejection stay
//!   distinct;
//! - the movement axis carries the closed movement vocabulary
//!   (`closed`/`improved`/`unchanged`/`regressed`/`limited`/`stale`/
//!   `uncertain`) with the native-identity join recorded and typed: a
//!   confident state requires the complete before/after joins, its recorded
//!   state must be the one its own headline/oracle evidence implies (the same
//!   transition table the producing comparison applies), and the documented
//!   partial shapes for `stale`, `uncertain`, and `limited` are enforced —
//!   each degraded state carries a typed reason, so a degraded join is never
//!   presented as a confident classification and a fabricated join never
//!   validates;
//! - NEITHER AXIS IMPLIES THE OTHER: every state pair across the two blocks
//!   validates on its own merits — `passed` with `unchanged`, `failed` with
//!   `improved`, `unavailable` with `uncertain`, `closed` next to an
//!   unrelated-finding regression all stay representable, and the schema has
//!   no lifecycle field (`states`/`lifecycle`/`accepted` are denied) so a
//!   passing execution alone can never mark an attempt accepted or closed;
//! - unrelated actionable movement stays visible in its own block with a
//!   row-derived consistency check (`regressed` implies after > before,
//!   `improved` implies after < before, never both);
//! - the rollback block is one of `proved`/`blocked`/`not_run` with its
//!   evidence: `proved` requires the restored head, `blocked`/`not_run` a
//!   typed reason;
//! - the standing non-claims and the standing claim boundary ride on every
//!   receipt verbatim, and the execution block's producer-owned scalars are
//!   type-checked (`exit_status`/`exit_signal` signed integers — a Windows
//!   termination code is negative, `duration_ms` non-negative, truncation
//!   and cancellation flags booleans).
//!
//! Verdict vocabulary: `valid`/`inconsistent`/`not_run` (no receipts
//! supplied). Violations take precedence: any violation makes the verdict
//! `inconsistent` and the command exits nonzero. `not_run` is reserved for
//! an empty input with no violations and is not a pass. None of the three is
//! a support-tier, repair-correctness, gate, badge, or promotion claim; the
//! governed cohort run stays with #3571.
//!
//! Focused modules: `cli` (entry and publication), `schema` (DTOs and closed
//! key sets), `validate` (identity and currentness), `aggregate` (joins and
//! verdict), `render` (JSON/Markdown). Tests remain
//! `python_repair_verification_semantics`. The published JSON kind remains
//! `python_repair_verification_check_report`.

mod aggregate;
mod cli;
mod render;
mod schema;
mod validate;

#[cfg(test)]
#[path = "python_repair_verification/tests.rs"]
mod python_repair_verification_semantics;

pub(crate) use cli::run_check_verification;

// Re-imported at the facade so tests keep reaching the moved items
// through `super::` exactly as they did when this module was one file.
#[cfg(test)]
pub(crate) use super::python_repair_trust::canonical_selection_digest;
#[cfg(test)]
pub(crate) use aggregate::{VerificationCheckOutcome, check_verification_artifacts};
#[cfg(test)]
pub(crate) use schema::{RECEIPT_CLAIM_BOUNDARY, VERIFICATION_NON_CLAIMS, VerificationReceipt};
#[cfg(test)]
pub(crate) use serde_json::{Value, json};
