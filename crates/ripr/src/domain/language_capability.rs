//! Per-language semantic capability cells.
//!
//! One closed, deterministic table answers, for a language + framework +
//! analysis mode + capability family: what RIPR can establish, how strongly,
//! what proof backs it, and where it must widen or stop (#5531).
//!
//! Semantic support is kept apart from adapter availability: whether an
//! adapter is compiled into this binary (`LanguageId::is_available`) says
//! nothing about what its cells may claim, and a cell never implies the
//! adapter is compiled or enabled. Renderers project the table; they cannot
//! strengthen a cell.

use super::LanguageId;
use sha2::{Digest, Sha256};

pub(crate) const LANGUAGE_CAPABILITIES_SCHEMA_VERSION: &str = "0.1";

/// Framework value for a cell whose semantics do not depend on a test
/// framework. A framework-specific cell never widens to this value.
pub(crate) const FRAMEWORK_ANY: &str = "any";

/// Closed support vocabulary. There is deliberately no numeric score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SupportState {
    ExactSupported,
    ConservativeOverapproximation,
    SupportedWithRequiredFallback,
    UnsupportedWithTypedLimit,
    NotApplicableWithEvidence,
    Stale,
    NotProven,
}

impl SupportState {
    pub(crate) const ALL: [SupportState; 7] = [
        SupportState::ExactSupported,
        SupportState::ConservativeOverapproximation,
        SupportState::SupportedWithRequiredFallback,
        SupportState::UnsupportedWithTypedLimit,
        SupportState::NotApplicableWithEvidence,
        SupportState::Stale,
        SupportState::NotProven,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SupportState::ExactSupported => "exact_supported",
            SupportState::ConservativeOverapproximation => "conservative_overapproximation",
            SupportState::SupportedWithRequiredFallback => "supported_with_required_fallback",
            SupportState::UnsupportedWithTypedLimit => "unsupported_with_typed_limit",
            SupportState::NotApplicableWithEvidence => "not_applicable_with_evidence",
            SupportState::Stale => "stale",
            SupportState::NotProven => "not_proven",
        }
    }
}

/// Analysis scope a cell applies to. Repo support may differ from diff
/// support for the same language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AnalysisMode {
    Diff,
    Repo,
}

impl AnalysisMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            AnalysisMode::Diff => "diff",
            AnalysisMode::Repo => "repo",
        }
    }
}

/// Semantic capability family a cell speaks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CapabilityFamily {
    /// Owners, tests, assertions/oracles, probes, related-test relations, and
    /// RIPR-stage evidence.
    StaticFacts,
    /// Bounded repair packets/cards, verify routes, and receipt movement.
    RepairRouting,
}

impl CapabilityFamily {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CapabilityFamily::StaticFacts => "static_facts",
            CapabilityFamily::RepairRouting => "repair_routing",
        }
    }
}

/// Highest support tier the cell may be read at, matching the rows of
/// `docs/status/SUPPORT_TIERS.md`. Promotion stays an explicit, separate
/// decision; this table never promotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TierCeiling {
    UsableAlpha,
    Preview,
}

impl TierCeiling {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            TierCeiling::UsableAlpha => "usable_alpha",
            TierCeiling::Preview => "preview",
        }
    }
}

/// One language/framework/mode/family capability cell.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CapabilityCell {
    pub(crate) language: LanguageId,
    pub(crate) framework: &'static str,
    pub(crate) mode: AnalysisMode,
    pub(crate) family: CapabilityFamily,
    pub(crate) state: SupportState,
    pub(crate) tier_ceiling: TierCeiling,
    /// Typed fallback or limitation the reader must honor; `None` only when
    /// the state carries no limit.
    pub(crate) limit: Option<&'static str>,
    /// Specs that define the claim.
    pub(crate) proof_refs: &'static [&'static str],
}

/// Claims every cell refuses, regardless of state.
pub(crate) const COMMON_NON_CLAIMS: &[&str] = &[
    "static evidence only: no runtime mutation outcome and no sufficiency claim",
    "a cell does not establish that the adapter is compiled or enabled",
    "no gate, badge, baseline, or RIPR Zero authority",
];

const RUST_FALLBACK: &str =
    "unknown stop reasons stay explicit; static findings never claim a runtime mutation outcome";
const PYTHON_FACTS_LIMIT: &str = "syntax-first and advisory; static-limit, heuristic-only, no-path, and already-observed states stay out of repair packets";
const PYTHON_REPO_LIMIT: &str = "bounded working set; capped or partial runs are disclosed on language_runs and never back a full-denominator, all-clear, badge, or baseline claim; repo-scoped seam formats render no Python seams (python_diff_first)";
const PYTHON_REPAIR_LIMIT: &str = "selected static repair routes only: direct weak evidence, concrete missing discriminator, safe test location, and verify command; no test execution, generated tests, or production edits";
const TS_FACTS_LIMIT: &str = "syntax-first preview; static limits such as mocked modules stay visible; no framework-specific activation claim";
const TS_REPO_LIMIT: &str = "typescript_diff_first: repo mode is a stub that returns an empty result; a zero-seam report is not evidence of clean coverage";
const TS_REPAIR_LIMIT: &str = "advisory preview actionability only; repair_packet_ready gates each packet; no default blocking";
const PERL_LIMIT: &str = "requires a ripr build with Cargo feature lang-perl plus a compatible fact exporter; default builds report changed Perl files as not analyzed and no released ripr plus exporter combination analyzes Perl yet";
const PERL_REPAIR_LIMIT: &str =
    "blocked on real-repository producer proof and real Perl repository receipts";

const SPEC_RUST: &[&str] = &["RIPR-SPEC-0001"];
const SPEC_RUST_REPAIR: &[&str] = &["RIPR-SPEC-0001", "RIPR-SPEC-0057"];
const SPEC_PYTHON: &[&str] = &["RIPR-SPEC-0026", "RIPR-SPEC-0028"];
const SPEC_TS: &[&str] = &["RIPR-SPEC-0026", "RIPR-SPEC-0027"];
const SPEC_PERL: &[&str] = &["RIPR-SPEC-0026", "RIPR-SPEC-0064"];

macro_rules! cell {
    ($language:expr, $framework:expr, $mode:expr, $family:expr, $state:expr, $tier:expr, $limit:expr, $proof:expr $(,)?) => {
        CapabilityCell {
            language: $language,
            framework: $framework,
            mode: $mode,
            family: $family,
            state: $state,
            tier_ceiling: $tier,
            limit: $limit,
            proof_refs: $proof,
        }
    };
}

use AnalysisMode::{Diff, Repo};
use CapabilityFamily::{RepairRouting, StaticFacts};
use SupportState::{NotProven, SupportedWithRequiredFallback, UnsupportedWithTypedLimit};
use TierCeiling::{Preview, UsableAlpha};

/// The current capability cells. Only the combinations listed here are
/// registered; a combination with no cell is `not_proven`, never implied by a
/// neighboring language, framework, or mode.
pub(crate) const CAPABILITY_CELLS: &[CapabilityCell] = &[
    cell!(
        LanguageId::Rust,
        FRAMEWORK_ANY,
        Diff,
        StaticFacts,
        SupportedWithRequiredFallback,
        UsableAlpha,
        Some(RUST_FALLBACK),
        SPEC_RUST,
    ),
    cell!(
        LanguageId::Rust,
        FRAMEWORK_ANY,
        Repo,
        StaticFacts,
        SupportedWithRequiredFallback,
        UsableAlpha,
        Some(RUST_FALLBACK),
        SPEC_RUST,
    ),
    cell!(
        LanguageId::Rust,
        FRAMEWORK_ANY,
        Diff,
        RepairRouting,
        SupportedWithRequiredFallback,
        UsableAlpha,
        Some(
            "bounded test-only repairs when a valid route exists; governed real-repository route yield is unestablished",
        ),
        SPEC_RUST_REPAIR,
    ),
    cell!(
        LanguageId::Python,
        FRAMEWORK_ANY,
        Diff,
        StaticFacts,
        SupportedWithRequiredFallback,
        Preview,
        Some(PYTHON_FACTS_LIMIT),
        SPEC_PYTHON,
    ),
    cell!(
        LanguageId::Python,
        FRAMEWORK_ANY,
        Repo,
        StaticFacts,
        SupportedWithRequiredFallback,
        Preview,
        Some(PYTHON_REPO_LIMIT),
        SPEC_PYTHON,
    ),
    cell!(
        LanguageId::Python,
        "pytest",
        Diff,
        RepairRouting,
        SupportedWithRequiredFallback,
        UsableAlpha,
        Some(PYTHON_REPAIR_LIMIT),
        SPEC_PYTHON,
    ),
    cell!(
        LanguageId::Python,
        "unittest",
        Diff,
        RepairRouting,
        SupportedWithRequiredFallback,
        UsableAlpha,
        Some(PYTHON_REPAIR_LIMIT),
        SPEC_PYTHON,
    ),
    cell!(
        LanguageId::TypeScript,
        FRAMEWORK_ANY,
        Diff,
        StaticFacts,
        SupportedWithRequiredFallback,
        Preview,
        Some(TS_FACTS_LIMIT),
        SPEC_TS,
    ),
    cell!(
        LanguageId::TypeScript,
        FRAMEWORK_ANY,
        Repo,
        StaticFacts,
        UnsupportedWithTypedLimit,
        Preview,
        Some(TS_REPO_LIMIT),
        SPEC_TS,
    ),
    cell!(
        LanguageId::TypeScript,
        FRAMEWORK_ANY,
        Diff,
        RepairRouting,
        SupportedWithRequiredFallback,
        Preview,
        Some(TS_REPAIR_LIMIT),
        SPEC_TS,
    ),
    cell!(
        LanguageId::JavaScript,
        FRAMEWORK_ANY,
        Diff,
        StaticFacts,
        SupportedWithRequiredFallback,
        Preview,
        Some(TS_FACTS_LIMIT),
        SPEC_TS,
    ),
    cell!(
        LanguageId::JavaScript,
        FRAMEWORK_ANY,
        Repo,
        StaticFacts,
        UnsupportedWithTypedLimit,
        Preview,
        Some(TS_REPO_LIMIT),
        SPEC_TS,
    ),
    cell!(
        LanguageId::JavaScript,
        FRAMEWORK_ANY,
        Diff,
        RepairRouting,
        SupportedWithRequiredFallback,
        Preview,
        Some(TS_REPAIR_LIMIT),
        SPEC_TS,
    ),
    cell!(
        LanguageId::Perl,
        FRAMEWORK_ANY,
        Diff,
        StaticFacts,
        SupportedWithRequiredFallback,
        Preview,
        Some(PERL_LIMIT),
        SPEC_PERL,
    ),
    cell!(
        LanguageId::Perl,
        FRAMEWORK_ANY,
        Repo,
        StaticFacts,
        SupportedWithRequiredFallback,
        Preview,
        Some(PERL_LIMIT),
        SPEC_PERL,
    ),
    cell!(
        LanguageId::Perl,
        FRAMEWORK_ANY,
        Diff,
        RepairRouting,
        NotProven,
        Preview,
        Some(PERL_REPAIR_LIMIT),
        SPEC_PERL,
    ),
];

/// Cells for one language, in registry order.
#[cfg(test)]
pub(crate) fn cells_for(language: LanguageId) -> impl Iterator<Item = &'static CapabilityCell> {
    CAPABILITY_CELLS
        .iter()
        .filter(move |cell| cell.language == language)
}

/// Why the table is not internally consistent, one sentence per problem.
/// Enforced by this module's tests so a bad edit to the const table fails CI.
#[cfg(test)]
pub(crate) fn capability_violations(cells: &[CapabilityCell]) -> Vec<String> {
    let mut violations = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let id = cell_id(cell);
        if cells[..index].iter().any(|prior| cell_id(prior) == id) {
            violations.push(format!("duplicate capability cell `{id}`"));
        }
        if cell.proof_refs.is_empty() {
            violations.push(format!("capability cell `{id}` names no proof reference"));
        }
        let needs_limit = !matches!(cell.state, SupportState::ExactSupported);
        if needs_limit && cell.limit.is_none() {
            violations.push(format!(
                "capability cell `{id}` is `{}` but names no limit or fallback",
                cell.state.as_str()
            ));
        }
        if cell.language == LanguageId::Rust && cell.tier_ceiling == TierCeiling::Preview {
            violations.push(format!(
                "capability cell `{id}` caps the reference language at preview"
            ));
        }
    }
    for language in LanguageId::ALL {
        if !cells.iter().any(|cell| cell.language == language) {
            violations.push(format!(
                "routed language `{}` has no capability cell",
                language.as_str()
            ));
        }
    }
    violations
}

/// Stable identity of one cell: `language/framework/mode/family`.
pub(crate) fn cell_id(cell: &CapabilityCell) -> String {
    format!(
        "{}/{}/{}/{}",
        cell.language.as_str(),
        cell.framework,
        cell.mode.as_str(),
        cell.family.as_str()
    )
}

/// Digest of the semantic content, independent of row order, so a consumer can
/// tell whether anything it cached about capability support moved.
pub(crate) fn semantic_digest(cells: &[CapabilityCell]) -> String {
    let mut lines: Vec<String> = cells
        .iter()
        .map(|cell| {
            format!(
                "{}|{}|{}|{}|{}",
                cell_id(cell),
                cell.state.as_str(),
                cell.tier_ceiling.as_str(),
                cell.limit.unwrap_or(""),
                cell.proof_refs.join(",")
            )
        })
        .collect();
    lines.sort();
    let mut hasher = Sha256::new();
    hasher.update(LANGUAGE_CAPABILITIES_SCHEMA_VERSION.as_bytes());
    for line in &lines {
        hasher.update(b"\n");
        hasher.update(line.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = String::from("sha256:");
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_table_is_consistent() {
        assert_eq!(
            capability_violations(CAPABILITY_CELLS),
            Vec::<String>::new()
        );
    }

    #[test]
    fn every_routed_language_has_a_cell() {
        for language in LanguageId::ALL {
            assert!(
                cells_for(language).next().is_some(),
                "missing cells for {}",
                language.as_str()
            );
        }
    }

    #[test]
    fn typescript_and_javascript_keep_distinct_identity() {
        let ts: Vec<_> = cells_for(LanguageId::TypeScript).map(cell_id).collect();
        let js: Vec<_> = cells_for(LanguageId::JavaScript).map(cell_id).collect();
        assert_eq!(ts.len(), js.len());
        assert!(ts.iter().all(|id| id.starts_with("typescript/")));
        assert!(js.iter().all(|id| id.starts_with("javascript/")));
    }

    #[test]
    fn framework_specific_repair_does_not_widen_to_language_wide() {
        let python_repair_any = cells_for(LanguageId::Python).any(|cell| {
            cell.family == CapabilityFamily::RepairRouting && cell.framework == FRAMEWORK_ANY
        });
        assert!(!python_repair_any);
        assert!(cells_for(LanguageId::Python).any(|cell| cell.framework == "pytest"));
        assert!(cells_for(LanguageId::Python).any(|cell| cell.framework == "unittest"));
    }

    #[test]
    fn repo_support_can_differ_from_diff_support() {
        let state = |mode| {
            cells_for(LanguageId::TypeScript)
                .find(|cell| cell.mode == mode && cell.family == CapabilityFamily::StaticFacts)
                .map(|cell| cell.state)
        };
        assert_eq!(
            state(AnalysisMode::Diff),
            Some(SupportState::SupportedWithRequiredFallback)
        );
        assert_eq!(
            state(AnalysisMode::Repo),
            Some(SupportState::UnsupportedWithTypedLimit)
        );
    }

    #[test]
    fn duplicate_missing_proof_and_missing_limit_are_rejected() {
        let base = CAPABILITY_CELLS[0];
        let mut no_proof = base;
        no_proof.framework = "other";
        no_proof.proof_refs = &[];
        let mut no_limit = base;
        no_limit.framework = "third";
        no_limit.limit = None;
        let violations = capability_violations(&[base, base, no_proof, no_limit]);
        assert!(
            violations.iter().any(|v| v.contains("duplicate")),
            "{violations:?}"
        );
        assert!(
            violations.iter().any(|v| v.contains("no proof reference")),
            "{violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.contains("no limit or fallback")),
            "{violations:?}"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.contains("has no capability cell")),
            "{violations:?}"
        );
    }

    #[test]
    fn digest_ignores_row_order_and_tracks_semantics() {
        let mut reordered = CAPABILITY_CELLS.to_vec();
        reordered.reverse();
        assert_eq!(
            semantic_digest(CAPABILITY_CELLS),
            semantic_digest(&reordered)
        );
        let mut changed = CAPABILITY_CELLS.to_vec();
        changed[0].state = SupportState::NotProven;
        assert_ne!(semantic_digest(CAPABILITY_CELLS), semantic_digest(&changed));
    }

    #[test]
    fn support_vocabulary_is_closed_and_unique() {
        let mut wires: Vec<_> = SupportState::ALL.iter().map(|s| s.as_str()).collect();
        wires.sort_unstable();
        wires.dedup();
        assert_eq!(wires.len(), 7);
    }
}
