//! Per-language run status DTOs shared by adapters and the facade.

/// Per-language run status for one language adapter invocation.
///
/// A `LanguageRun` is recorded for every language that was *attempted* but
/// did not complete successfully. Languages that ran to completion are
/// omitted (their findings speak for themselves). This keeps the field
/// conditional on non-success so the common single-language-success case
/// stays silent and no golden re-bless is needed.
///
/// This is the non-abort contract (Campaign 31 PR 10, ripr-swarm#1403): a
/// single language's failure (e.g. a Perl preview adapter that returns
/// `Err`, or a packet-ingestion rejection) must not abort the whole report.
/// Instead the failed language is recorded here and the other languages'
/// findings still emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageRun {
    /// Stable language wire string (e.g. `"rust"`, `"perl"`).
    pub language: String,
    /// Outcome of the run.
    pub status: LanguageRunStatus,
    /// Human-readable reason for a non-ok status (absent for `Ok`).
    pub reason: Option<String>,
}

/// Outcome of one language adapter invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageRunStatus {
    /// The adapter completed (findings, if any, are in `result.findings`).
    /// Recorded only when the caller explicitly asks for full accounting;
    /// the pipeline omits `Ok` runs by default to keep the field conditional.
    Ok,
    /// The adapter was invoked but returned a named limitation (e.g. a Perl
    /// preview adapter that is scaffold-only and returns `Err`).
    Unavailable,
    /// The adapter ran but produced a partial result (e.g. a packet was
    /// rejected by ingestion checks; some findings may still be present).
    Partial,
    /// The adapter could not run at all (e.g. required Cargo feature is off,
    /// or the producer binary is missing).
    Invalid,
    /// A *configured* managed fact producer was invoked and failed (spawn
    /// error, timeout, or non-zero exit). Distinct from `Unavailable`
    /// (nothing configured) so the typed limitation and `language_runs`
    /// reason carry the real exporter failure instead of re-advising a
    /// configuration the user already made (#6828).
    Failed,
}

impl LanguageRunStatus {
    /// Stable wire string for JSON / human output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unavailable => "unavailable",
            Self::Partial => "partial",
            Self::Invalid => "invalid",
            Self::Failed => "failed",
        }
    }
}
