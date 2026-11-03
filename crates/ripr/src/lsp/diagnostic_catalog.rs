//! Governed diagnostic-code catalog for RIPR LSP diagnostics.
//!
//! RIPR-SPEC-0127 (issue #1662 slice A + #4675 / PR C first slice). One
//! registry owns the closed set of stable `code` values every RIPR diagnostic
//! may carry, plus the producer-owned recovery, actionability, and profile
//! metadata for those codes. The message text is presentation; the catalog
//! owns identity and the default machine metadata.
//!
//! Slice A established the typed registry, the current-code inventory, the
//! single-source constructors that build every emitted code, and fail-closed
//! resolution. This first #4675 slice extends each entry with closed
//! recovery / actionability / profile fields and projects them through the
//! shared #1663 `FixInstructionState` vocabulary. Constructors still emit the
//! same code strings; renderers may omit optional presentation but must not
//! invent a recovery or strengthen a limited catalog state.
//!
//! Still out of scope: per-code title/summary/docs and `codeDescription.href`
//! (#1662 PR B); live-code migration and the conformance gate (#4676 / PR D);
//! analyzer classification, severity-policy, or support-tier changes.

use crate::analysis::seams::SeamGripClass;
use crate::domain::{ExposureClass, FixInstructionState};

/// One governed diagnostic code, the deprecated aliases that still resolve
/// to it, and the producer-owned recovery / actionability / profile metadata
/// this code earns by default.
pub(crate) struct CatalogEntry {
    /// The stable, emitted diagnostic code.
    pub(crate) code: &'static str,
    /// Deprecated codes that still resolve to this entry but are no longer
    /// emitted. Empty unless current output already requires an alias.
    pub(crate) aliases: &'static [&'static str],
    /// Default #1663 instruction state this code earns. Instance witness
    /// state may weaken this default (stale / unavailable) but must not
    /// invent a stronger state than the catalog records.
    pub(crate) actionability: FixInstructionState,
    /// Closed recovery kinds this code may advertise. Empty means typed
    /// unavailable — never a guessed command or action.
    pub(crate) recovery_kinds: &'static [CatalogRecoveryKind],
    /// Why recovery is unavailable when `recovery_kinds` is empty.
    pub(crate) unavailable_reason: Option<CatalogUnavailableReason>,
    /// Which diagnostic profiles include this code by default.
    pub(crate) default_visibility: CatalogProfileVisibility,
    /// Where the default LSP severity comes from. Catalog metadata does not
    /// change the live severity policy.
    pub(crate) severity_source: CatalogSeveritySource,
    /// Default severity class this code earns before instance policy applies.
    pub(crate) default_severity: CatalogDefaultSeverity,
    /// Freshness / completed-result preconditions for current recovery.
    pub(crate) preconditions: &'static [CatalogPrecondition],
    /// Honesty bounds that travel with this code.
    pub(crate) non_claims: &'static [CatalogNonClaim],
}

/// Declares a closed diagnostic vocabulary from one list of
/// `Variant => "label"` pairs. The enum, its `as_str` match and the `LABELS`
/// list the well-formedness check reads are all generated from that list, so a
/// variant cannot exist without its label being checked (#12853).
macro_rules! closed_vocabulary {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($variant:ident => $label:literal,)+
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $($variant,)+
        }

        impl $name {
            pub(crate) const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)+
                }
            }

            /// Every wire label of this vocabulary, one per variant.
            pub(crate) const LABELS: &'static [&'static str] = &[$($label,)+];

            /// Every variant, generated from the same list as `LABELS`.
            #[cfg(test)]
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant,)+];
        }
    };
}

closed_vocabulary! {
    /// Closed recovery vocabulary for catalogued diagnostics (#1662 PR C).
    /// Kinds name an inspect / open / retry / refresh / setup / verify / repair
    /// route. Absence of a kind is represented by an empty `recovery_kinds`
    /// list plus [`CatalogUnavailableReason`], not by guessing a command.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogRecoveryKind {
        InspectEvidence => "inspect_evidence",
        InspectFullProfile => "inspect_full_profile",
        OpenFixSite => "open_fix_site",
        RefreshAnalysis => "refresh_analysis",
        Retry => "retry",
        SelectWorkspaceRoot => "select_workspace_root",
        RepairConfiguration => "repair_configuration",
        RetrieveOverflow => "retrieve_overflow",
        RunVerification => "run_verification",
        RecordReceipt => "record_receipt",
    }
}

closed_vocabulary! {
    /// Why a catalog entry has no bounded recovery. Used only when
    /// `recovery_kinds` is empty so missing recovery stays typed.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogUnavailableReason {
        NoBoundedAction => "no_bounded_action",
        MissingProducerEvidence => "missing_producer_evidence",
        UnsafeWithoutRefresh => "unsafe_without_refresh",
    }
}

closed_vocabulary! {
    /// Default profile membership for a catalogued code. Client capability may
    /// hide presentation; it cannot change this membership.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogProfileVisibility {
        ActionableAndFull => "actionable_and_full",
        FullOnly => "full_only",
        Always => "always",
    }
}

closed_vocabulary! {
    /// Authority that owns the default severity. The catalog records the source;
    /// it does not replace RIPR-SPEC-0076 or `SeverityConfig`.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogSeveritySource {
        AdvisoryPolicy => "advisory_policy",
        SeverityConfig => "severity_config",
        DeliveryLimitation => "delivery_limitation",
    }
}

closed_vocabulary! {
    /// Default LSP severity class the code earns before instance policy.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogDefaultSeverity {
        Warning => "warning",
        Information => "information",
        Hint => "hint",
    }
}

closed_vocabulary! {
    /// Freshness or completed-result precondition for catalog recovery.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogPrecondition {
        CurrentSnapshot => "current_snapshot",
        CompletedResult => "completed_result",
    }
}

closed_vocabulary! {
    /// Honesty bounds that a catalogued code must not be read as proving.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(crate) enum CatalogNonClaim {
        RuntimeProof => "not_runtime_proof",
        TestAdequacy => "not_test_adequacy",
        RepairAuthorization => "not_repair_authorization",
        CleanWorkspace => "not_clean_workspace",
        SupportPromotion => "not_support_promotion",
    }
}

/// Client capability that may omit optional presentation. It cannot change
/// catalog actionability, recovery, visibility, or severity source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogClientView {
    pub(crate) supports_diagnostic_data: bool,
}

impl CatalogClientView {
    pub(crate) const FULL: Self = Self {
        supports_diagnostic_data: true,
    };

    pub(crate) const MINIMAL: Self = Self {
        supports_diagnostic_data: false,
    };
}

/// Catalog state projected for a consumer. Optional presentation may be
/// omitted; the underlying catalog fields stay identical.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CatalogProjection {
    pub(crate) code: &'static str,
    pub(crate) actionability: FixInstructionState,
    pub(crate) recovery_kinds: &'static [CatalogRecoveryKind],
    pub(crate) unavailable_reason: Option<CatalogUnavailableReason>,
    pub(crate) default_visibility: CatalogProfileVisibility,
    pub(crate) severity_source: CatalogSeveritySource,
    pub(crate) default_severity: CatalogDefaultSeverity,
    pub(crate) preconditions: &'static [CatalogPrecondition],
    pub(crate) non_claims: &'static [CatalogNonClaim],
    pub(crate) present_optional_fields: bool,
}

const fn project(entry: &CatalogEntry, view: CatalogClientView) -> CatalogProjection {
    CatalogProjection {
        code: entry.code,
        actionability: entry.actionability,
        recovery_kinds: entry.recovery_kinds,
        unavailable_reason: entry.unavailable_reason,
        default_visibility: entry.default_visibility,
        severity_source: entry.severity_source,
        default_severity: entry.default_severity,
        preconditions: entry.preconditions,
        non_claims: entry.non_claims,
        present_optional_fields: view.supports_diagnostic_data,
    }
}

/// Project catalog metadata for a governed code. Unknown codes return
/// nothing — they are not given a guessed recovery.
pub(crate) fn metadata_for(code: &str) -> Option<CatalogProjection> {
    let projection = metadata_for_client(code, CatalogClientView::FULL)?;
    Some(CatalogProjection {
        actionability: join_instruction_state(projection.actionability, projection.actionability),
        ..projection
    })
}

/// Same catalog state with optional presentation omitted. Actionability,
/// recovery, visibility, and severity source stay identical.
pub(crate) fn metadata_for_client(
    code: &str,
    view: CatalogClientView,
) -> Option<CatalogProjection> {
    resolve(code).map(|entry| project(entry, view))
}

fn limitation_rank(state: FixInstructionState) -> u8 {
    match state {
        FixInstructionState::FixSiteReady => 0,
        FixInstructionState::InspectOnly => 1,
        FixInstructionState::StaticLimitation => 2,
        FixInstructionState::Stale => 3,
        FixInstructionState::Unavailable => 4,
    }
}

/// Join catalog default actionability with instance #1663 state. The more
/// limited state wins so a renderer cannot strengthen a limited or stale
/// result, and a catalog default cannot invent `FixSiteReady` over a stale
/// or unavailable witness.
pub(crate) fn join_instruction_state(
    catalog: FixInstructionState,
    instance: FixInstructionState,
) -> FixInstructionState {
    if limitation_rank(instance) > limitation_rank(catalog) {
        instance
    } else {
        catalog
    }
}

/// Allow a recovery kind only when the catalog lists it. A requested kind
/// that is not catalogued stays typed unavailable.
pub(crate) fn allow_recovery(
    entry: &CatalogEntry,
    requested: CatalogRecoveryKind,
) -> Option<CatalogRecoveryKind> {
    entry
        .recovery_kinds
        .contains(&requested)
        .then_some(requested)
}

#[derive(Clone, Copy)]
struct CatalogMeta {
    actionability: FixInstructionState,
    recovery_kinds: &'static [CatalogRecoveryKind],
    unavailable_reason: Option<CatalogUnavailableReason>,
    default_visibility: CatalogProfileVisibility,
    severity_source: CatalogSeveritySource,
    default_severity: CatalogDefaultSeverity,
    preconditions: &'static [CatalogPrecondition],
    non_claims: &'static [CatalogNonClaim],
}

const RECOVERY_OPEN_FIX: &[CatalogRecoveryKind] = &[
    CatalogRecoveryKind::OpenFixSite,
    CatalogRecoveryKind::InspectEvidence,
];
const RECOVERY_INSPECT: &[CatalogRecoveryKind] = &[CatalogRecoveryKind::InspectEvidence];
const RECOVERY_NONE: &[CatalogRecoveryKind] = &[];
const RECOVERY_PENDING: &[CatalogRecoveryKind] = &[
    CatalogRecoveryKind::RefreshAnalysis,
    CatalogRecoveryKind::RepairConfiguration,
    CatalogRecoveryKind::SelectWorkspaceRoot,
];
const RECOVERY_OVERFLOW: &[CatalogRecoveryKind] = &[CatalogRecoveryKind::RetrieveOverflow];
const RECOVERY_WITHHELD: &[CatalogRecoveryKind] = &[
    CatalogRecoveryKind::InspectFullProfile,
    CatalogRecoveryKind::RefreshAnalysis,
];
const RECOVERY_QUARANTINE: &[CatalogRecoveryKind] = &[CatalogRecoveryKind::RefreshAnalysis];

const PRECOND_SNAPSHOT: &[CatalogPrecondition] = &[
    CatalogPrecondition::CurrentSnapshot,
    CatalogPrecondition::CompletedResult,
];
const PRECOND_COMPLETED: &[CatalogPrecondition] = &[CatalogPrecondition::CompletedResult];
const PRECOND_NONE: &[CatalogPrecondition] = &[];

const NON_CLAIMS_STATIC: &[CatalogNonClaim] = &[
    CatalogNonClaim::RuntimeProof,
    CatalogNonClaim::TestAdequacy,
    CatalogNonClaim::RepairAuthorization,
];
const NON_CLAIMS_DELIVERY: &[CatalogNonClaim] = &[
    CatalogNonClaim::RuntimeProof,
    CatalogNonClaim::TestAdequacy,
    CatalogNonClaim::RepairAuthorization,
    CatalogNonClaim::CleanWorkspace,
    CatalogNonClaim::SupportPromotion,
];

const EXPOSURE_ACTIONABLE: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::FixSiteReady,
    recovery_kinds: RECOVERY_OPEN_FIX,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::ActionableAndFull,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Warning,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const EXPOSURE_INSPECT: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_INSPECT,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const EXPOSURE_STATIC: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::StaticLimitation,
    recovery_kinds: RECOVERY_NONE,
    unavailable_reason: Some(CatalogUnavailableReason::NoBoundedAction),
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const SEAM_INSPECT: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_INSPECT,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::SeverityConfig,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const SEAM_STATIC: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::StaticLimitation,
    recovery_kinds: RECOVERY_NONE,
    unavailable_reason: Some(CatalogUnavailableReason::NoBoundedAction),
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::SeverityConfig,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const GAP_ACTIONABLE: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::FixSiteReady,
    recovery_kinds: RECOVERY_OPEN_FIX,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::ActionableAndFull,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Warning,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const GAP_INSPECT: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_INSPECT,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const GAP_STATIC: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::StaticLimitation,
    recovery_kinds: RECOVERY_NONE,
    unavailable_reason: Some(CatalogUnavailableReason::NoBoundedAction),
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const GAP_UNAVAILABLE: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::Unavailable,
    recovery_kinds: RECOVERY_NONE,
    unavailable_reason: Some(CatalogUnavailableReason::MissingProducerEvidence),
    default_visibility: CatalogProfileVisibility::FullOnly,
    severity_source: CatalogSeveritySource::AdvisoryPolicy,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_SNAPSHOT,
    non_claims: NON_CLAIMS_STATIC,
};

const DELIVERY_PENDING: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_PENDING,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::Always,
    severity_source: CatalogSeveritySource::DeliveryLimitation,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_NONE,
    non_claims: NON_CLAIMS_DELIVERY,
};

const DELIVERY_OVERFLOW: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_OVERFLOW,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::Always,
    severity_source: CatalogSeveritySource::DeliveryLimitation,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_COMPLETED,
    non_claims: NON_CLAIMS_DELIVERY,
};

const DELIVERY_QUARANTINE: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_QUARANTINE,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::Always,
    severity_source: CatalogSeveritySource::DeliveryLimitation,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_NONE,
    non_claims: NON_CLAIMS_DELIVERY,
};

const DELIVERY_UNSUPPORTED: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_NONE,
    unavailable_reason: Some(CatalogUnavailableReason::NoBoundedAction),
    default_visibility: CatalogProfileVisibility::Always,
    severity_source: CatalogSeveritySource::DeliveryLimitation,
    default_severity: CatalogDefaultSeverity::Information,
    preconditions: PRECOND_NONE,
    non_claims: NON_CLAIMS_DELIVERY,
};

const DELIVERY_WITHHELD: CatalogMeta = CatalogMeta {
    actionability: FixInstructionState::InspectOnly,
    recovery_kinds: RECOVERY_WITHHELD,
    unavailable_reason: None,
    default_visibility: CatalogProfileVisibility::Always,
    severity_source: CatalogSeveritySource::DeliveryLimitation,
    default_severity: CatalogDefaultSeverity::Hint,
    preconditions: PRECOND_COMPLETED,
    non_claims: NON_CLAIMS_DELIVERY,
};

const fn entry(code: &'static str, meta: CatalogMeta) -> CatalogEntry {
    CatalogEntry {
        code,
        aliases: &[],
        actionability: meta.actionability,
        recovery_kinds: meta.recovery_kinds,
        unavailable_reason: meta.unavailable_reason,
        default_visibility: meta.default_visibility,
        severity_source: meta.severity_source,
        default_severity: meta.default_severity,
        preconditions: meta.preconditions,
        non_claims: meta.non_claims,
    }
}

/// The governed catalog: exactly one entry per emitted code.
///
/// Finding codes are the verbatim `ExposureClass` labels; seam codes are
/// `ripr-seam-{SeamGripClass}` for every grip class; gap codes are
/// `ripr-gap-{GapRecord.kind}` for every known kind. `MissingArtifact` is a
/// value contract an external gap-decision-ledger producer may supply and is
/// catalogued so it never emits an unregistered code.
const CATALOG: &[CatalogEntry] = &[
    // Finding / exposure codes (verbatim ExposureClass labels).
    entry("exposed", EXPOSURE_INSPECT),
    entry("weakly_exposed", EXPOSURE_ACTIONABLE),
    entry("reachable_unrevealed", EXPOSURE_ACTIONABLE),
    entry("no_static_path", EXPOSURE_ACTIONABLE),
    entry("infection_unknown", EXPOSURE_STATIC),
    entry("propagation_unknown", EXPOSURE_STATIC),
    entry("static_unknown", EXPOSURE_STATIC),
    // Seam grip codes (ripr-seam-{SeamGripClass}).
    entry("ripr-seam-strongly-gripped", SEAM_INSPECT),
    entry("ripr-seam-weakly-gripped", SEAM_INSPECT),
    entry("ripr-seam-ungripped", SEAM_INSPECT),
    entry("ripr-seam-reachable-unrevealed", SEAM_INSPECT),
    entry("ripr-seam-activation-unknown", SEAM_STATIC),
    entry("ripr-seam-propagation-unknown", SEAM_STATIC),
    entry("ripr-seam-observation-unknown", SEAM_STATIC),
    entry("ripr-seam-discrimination-unknown", SEAM_STATIC),
    entry("ripr-seam-opaque", SEAM_STATIC),
    entry("ripr-seam-intentional", SEAM_INSPECT),
    entry("ripr-seam-suppressed", SEAM_INSPECT),
    // Gap decision codes (ripr-gap-{GapRecord.kind}).
    entry("ripr-gap-NoActionAlreadyObserved", GAP_INSPECT),
    entry("ripr-gap-NoActionInternal", GAP_INSPECT),
    entry("ripr-gap-NoActionNoRelatedTest", GAP_INSPECT),
    entry("ripr-gap-NoActionHeuristicOnly", GAP_INSPECT),
    entry("ripr-gap-StaticLimitation", GAP_STATIC),
    entry("ripr-gap-MissingOutputContract", GAP_ACTIONABLE),
    entry("ripr-gap-MissingBoundaryAssertion", GAP_ACTIONABLE),
    entry("ripr-gap-MissingErrorDiscriminator", GAP_ACTIONABLE),
    entry("ripr-gap-MissingValueAssertion", GAP_ACTIONABLE),
    entry("ripr-gap-MissingSideEffectObserver", GAP_ACTIONABLE),
    entry("ripr-gap-Unknown", GAP_UNAVAILABLE),
    entry("ripr-gap-MissingArtifact", GAP_UNAVAILABLE),
    // Scope-limitation codes (workspace-scoped guard disclosures).
    entry("ripr-scope-diff-oversized", DELIVERY_OVERFLOW),
    // Passive delivery limitations; never analyzer findings or repair routes.
    entry("ripr-diagnostic-budget-omitted", DELIVERY_OVERFLOW),
    entry("ripr-diagnostic-quarantine-withdrawn", DELIVERY_QUARANTINE),
    entry("ripr-diagnostic-analysis-pending", DELIVERY_PENDING),
    entry(
        "ripr-diagnostic-language-not-analyzed",
        DELIVERY_UNSUPPORTED,
    ),
    entry("ripr-diagnostic-preview-withheld", DELIVERY_WITHHELD),
];

/// The workspace-scoped warning code emitted when the fail-closed
/// diff-scope guard converts to a limited snapshot (#2299).
pub(crate) const DIFF_SCOPE_OVERSIZED_CODE: &str = "ripr-scope-diff-oversized";
pub(crate) const DIAGNOSTIC_BUDGET_OMITTED_CODE: &str = "ripr-diagnostic-budget-omitted";
pub(crate) const DIAGNOSTIC_QUARANTINE_WITHDRAWN_CODE: &str =
    "ripr-diagnostic-quarantine-withdrawn";
/// The Information marker served on both pull paths while no analysis
/// snapshot exists yet (#7774): an empty report would claim a clean
/// workspace, which the server cannot support.
pub(crate) const DIAGNOSTIC_ANALYSIS_PENDING_CODE: &str = "ripr-diagnostic-analysis-pending";
/// The Information marker on a changed file whose language adapter is not
/// enabled (#10715): the file was not analyzed, so an empty list for it is
/// not a clean result. Mirrors the typed `language_adapter_unavailable`
/// limitation that `check` and MCP carry.
pub(crate) const DIAGNOSTIC_LANGUAGE_NOT_ANALYZED_CODE: &str =
    "ripr-diagnostic-language-not-analyzed";

/// The Hint marker on a changed file whose preview-language findings the
/// actionable profile withheld (#11296): an empty list for that file would
/// read as clean. It carries no finding identity, repair data, or
/// actionability, and sits outside the delivery budget.
pub(crate) const DIAGNOSTIC_PREVIEW_WITHHELD_CODE: &str = "ripr-diagnostic-preview-withheld";

/// A closed vocabulary is usable on the wire only when every label is
/// non-empty snake_case and no two labels collide. Takes the label list so a
/// negative test can feed a violating vocabulary.
const fn labels_are_distinct_snake_case(labels: &[&str]) -> bool {
    let mut i = 0;
    while i < labels.len() {
        let bytes = labels[i].as_bytes();
        if bytes.is_empty() {
            return false;
        }
        let mut k = 0;
        while k < bytes.len() {
            let b = bytes[k];
            if !(b.is_ascii_lowercase() || b == b'_') {
                return false;
            }
            // No leading, trailing or doubled underscore.
            if b == b'_' && (k == 0 || k + 1 == bytes.len() || bytes[k + 1] == b'_') {
                return false;
            }
            k += 1;
        }
        let mut j = i + 1;
        while j < labels.len() {
            if bytes_equal(bytes, labels[j].as_bytes()) {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

const fn bytes_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Every closed vocabulary has distinct, non-empty snake_case labels and the
/// client views still differ on optional presentation. Evaluated at compile
/// time by [`catalog`]; a collapsed or duplicated label fails the build.
const fn closed_vocabulary_is_well_formed() -> bool {
    labels_are_distinct_snake_case(CatalogRecoveryKind::LABELS)
        && labels_are_distinct_snake_case(CatalogUnavailableReason::LABELS)
        && labels_are_distinct_snake_case(CatalogProfileVisibility::LABELS)
        && labels_are_distinct_snake_case(CatalogSeveritySource::LABELS)
        && labels_are_distinct_snake_case(CatalogDefaultSeverity::LABELS)
        && labels_are_distinct_snake_case(CatalogPrecondition::LABELS)
        && labels_are_distinct_snake_case(CatalogNonClaim::LABELS)
        && CatalogClientView::FULL.supports_diagnostic_data
        && !CatalogClientView::MINIMAL.supports_diagnostic_data
}

/// An entry's recovery declaration is coherent when every listed kind is
/// allowed, no kind repeats, and an empty list carries a typed unavailable
/// reason while a non-empty list carries none.
pub(crate) fn recovery_is_coherent(entry: &CatalogEntry) -> bool {
    let kinds = entry.recovery_kinds;
    for (index, kind) in kinds.iter().enumerate() {
        if allow_recovery(entry, *kind).is_none() || kinds[..index].contains(kind) {
            return false;
        }
    }
    kinds.is_empty() == entry.unavailable_reason.is_some()
}

/// The governed catalog of diagnostic codes.
pub(crate) fn catalog() -> &'static [CatalogEntry] {
    const {
        assert!(closed_vocabulary_is_well_formed());
    }
    CATALOG
}

/// Resolve a code (or a deprecated alias) to its catalog entry, if any. An
/// unknown code resolves to `None` and must not be emitted as a diagnostic.
pub(crate) fn resolve(code: &str) -> Option<&'static CatalogEntry> {
    catalog()
        .iter()
        .find(|entry| entry.code == code || entry.aliases.contains(&code))
}

fn emit_catalog_code(code: &str) -> Option<String> {
    let entry = resolve(code)?;
    let projection = metadata_for(code)?;
    // Fail closed if the entry's recovery declaration is incoherent: the
    // constructor must not emit a code whose catalog recovery is malformed.
    if !recovery_is_coherent(entry) {
        return None;
    }
    Some(projection.code.to_string())
}

/// Build the catalog-backed diagnostic code for a finding exposure class.
///
/// `ExposureClass` is a closed enum whose every label is a governed code, so
/// this is infallible. The emitted string is the catalog code; recovery and
/// actionability ride on the entry and do not rewrite the wire identity.
pub(crate) fn finding_code(class: &ExposureClass) -> String {
    let code = class.as_str();
    emit_catalog_code(code).unwrap_or_else(|| code.to_string())
}

/// Build the catalog-backed diagnostic code for a seam grip class.
///
/// `SeamGripClass` is a closed enum whose every label is a governed code, so
/// this is infallible.
pub(crate) fn seam_code(class: SeamGripClass) -> String {
    // user-text-exempt: machine diagnostic code, not prose
    let code = format!("ripr-seam-{}", class.as_str().replace('_', "-"));
    emit_catalog_code(&code).unwrap_or(code)
}

/// Build the catalog-backed diagnostic code for a gap-decision record kind.
///
/// `GapRecord.kind` is an open string that may come from an external ledger, so
/// this fails closed: it returns `Some` only when the resulting code is a
/// governed catalog entry, and `None` for an unregistered kind so the caller
/// under-emits rather than surfacing an unknown code.
pub(crate) fn gap_code(kind: &str) -> Option<String> {
    let code = format!("ripr-gap-{}", kind.replace('_', "-"));
    emit_catalog_code(&code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPOSURE_CLASSES: [ExposureClass; 7] = [
        ExposureClass::Exposed,
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::InfectionUnknown,
        ExposureClass::PropagationUnknown,
        ExposureClass::StaticUnknown,
    ];

    // Every `GapRecord.kind` this crate produces plus the external-producer
    // `MissingArtifact` contract; see crates/ripr/src/output/gap_decision_ledger.rs.
    const KNOWN_GAP_KINDS: [&str; 12] = [
        "NoActionAlreadyObserved",
        "NoActionInternal",
        "NoActionNoRelatedTest",
        "NoActionHeuristicOnly",
        "StaticLimitation",
        "MissingOutputContract",
        "MissingBoundaryAssertion",
        "MissingErrorDiscriminator",
        "MissingValueAssertion",
        "MissingSideEffectObserver",
        "Unknown",
        "MissingArtifact",
    ];

    fn require(code: &str) -> Result<&'static CatalogEntry, String> {
        resolve(code).ok_or_else(|| format!("expected catalogued code {code}"))
    }

    #[test]
    fn every_entry_has_a_nonempty_code() {
        for entry in catalog() {
            assert!(!entry.code.is_empty(), "catalog entry has an empty code");
        }
    }

    #[test]
    fn codes_and_aliases_are_globally_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in catalog() {
            assert!(seen.insert(entry.code), "duplicate code {}", entry.code);
            for alias in entry.aliases {
                assert!(
                    seen.insert(alias),
                    "alias {alias} collides with a code or alias"
                );
            }
        }
    }

    #[test]
    fn every_code_resolves_to_exactly_one_entry() {
        for entry in catalog() {
            let mut matches = catalog()
                .iter()
                .filter(|candidate| candidate.code == entry.code);
            assert!(matches.next().is_some(), "{} does not resolve", entry.code);
            assert!(
                matches.next().is_none(),
                "{} resolves to more than one entry",
                entry.code
            );
        }
    }

    #[test]
    fn every_emitted_finding_code_is_catalogued() {
        for class in EXPOSURE_CLASSES {
            let code = finding_code(&class);
            assert!(
                resolve(&code).is_some(),
                "finding code {code} is not in the catalog"
            );
            assert_eq!(
                code,
                class.as_str(),
                "finding constructor must stay verbatim"
            );
        }
    }

    #[test]
    fn every_emitted_seam_code_is_catalogued() {
        for class in SeamGripClass::ALL {
            let code = seam_code(class);
            assert!(
                resolve(&code).is_some(),
                "seam code {code} is not in the catalog"
            );
            assert_eq!(
                code,
                format!("ripr-seam-{}", class.as_str().replace('_', "-")),
                "seam constructor must stay kebab-case"
            );
        }
    }

    #[test]
    fn every_emitted_gap_code_is_catalogued() {
        for kind in KNOWN_GAP_KINDS {
            let code = gap_code(kind);
            assert!(
                code.is_some(),
                "gap kind {kind} did not produce a catalogued code"
            );
        }
    }

    #[test]
    fn gap_code_fails_closed_for_unknown_kind() {
        assert!(gap_code("TotallyUnregisteredKind").is_none());
        assert!(gap_code("").is_none());
    }

    #[test]
    fn unknown_code_does_not_resolve() {
        assert!(resolve("ripr-bogus-code").is_none());
        assert!(resolve("").is_none());
        assert!(resolve("ripr-gap-NotAGapKind").is_none());
        assert!(
            metadata_for("ripr-bogus-code").is_none(),
            "unknown codes must not receive guessed catalog metadata"
        );
    }

    #[test]
    fn vocabulary_check_rejects_empty_duplicate_and_non_snake_labels() {
        assert!(labels_are_distinct_snake_case(&["a_b", "c"]));
        assert!(!labels_are_distinct_snake_case(&["a", ""]));
        assert!(!labels_are_distinct_snake_case(&["a", "b", "a"]));
        assert!(!labels_are_distinct_snake_case(&["Upper"]));
        assert!(!labels_are_distinct_snake_case(&["has-dash"]));
        assert!(!labels_are_distinct_snake_case(&["_lead"]));
        assert!(!labels_are_distinct_snake_case(&["trail_"]));
        assert!(!labels_are_distinct_snake_case(&["dou__ble"]));
        assert!(closed_vocabulary_is_well_formed());
    }

    #[test]
    fn closed_vocabulary_labels_cover_every_variant() {
        fn check<T: Copy>(all: &[T], labels: &[&str], as_str: fn(T) -> &'static str) {
            let rendered: Vec<&str> = all.iter().map(|variant| as_str(*variant)).collect();
            assert_eq!(rendered, labels);
        }
        check(
            CatalogRecoveryKind::ALL,
            CatalogRecoveryKind::LABELS,
            CatalogRecoveryKind::as_str,
        );
        check(
            CatalogUnavailableReason::ALL,
            CatalogUnavailableReason::LABELS,
            CatalogUnavailableReason::as_str,
        );
        check(
            CatalogProfileVisibility::ALL,
            CatalogProfileVisibility::LABELS,
            CatalogProfileVisibility::as_str,
        );
        check(
            CatalogSeveritySource::ALL,
            CatalogSeveritySource::LABELS,
            CatalogSeveritySource::as_str,
        );
        check(
            CatalogDefaultSeverity::ALL,
            CatalogDefaultSeverity::LABELS,
            CatalogDefaultSeverity::as_str,
        );
        check(
            CatalogPrecondition::ALL,
            CatalogPrecondition::LABELS,
            CatalogPrecondition::as_str,
        );
        check(
            CatalogNonClaim::ALL,
            CatalogNonClaim::LABELS,
            CatalogNonClaim::as_str,
        );
    }

    #[test]
    fn recovery_check_rejects_incoherent_entries() {
        let meta = |kinds: &'static [CatalogRecoveryKind],
                    reason: Option<CatalogUnavailableReason>| CatalogMeta {
            actionability: FixInstructionState::InspectOnly,
            recovery_kinds: kinds,
            unavailable_reason: reason,
            default_visibility: CatalogProfileVisibility::Always,
            severity_source: CatalogSeveritySource::AdvisoryPolicy,
            default_severity: CatalogDefaultSeverity::Hint,
            preconditions: &[],
            non_claims: &[],
        };
        let ok = entry("x", meta(&[CatalogRecoveryKind::Retry], None));
        assert!(recovery_is_coherent(&ok));
        let none_typed = entry(
            "x",
            meta(&[], Some(CatalogUnavailableReason::NoBoundedAction)),
        );
        assert!(recovery_is_coherent(&none_typed));
        let none_untyped = entry("x", meta(&[], None));
        assert!(!recovery_is_coherent(&none_untyped));
        let both = entry(
            "x",
            meta(
                &[CatalogRecoveryKind::Retry],
                Some(CatalogUnavailableReason::NoBoundedAction),
            ),
        );
        assert!(!recovery_is_coherent(&both));
        let duplicate = entry(
            "x",
            meta(
                &[CatalogRecoveryKind::Retry, CatalogRecoveryKind::Retry],
                None,
            ),
        );
        assert!(!recovery_is_coherent(&duplicate));
        for catalogued in catalog() {
            assert!(recovery_is_coherent(catalogued), "{}", catalogued.code);
        }
    }

    #[test]
    fn aliases_remain_empty_unless_current_output_requires_them() {
        for entry in catalog() {
            assert!(
                entry.aliases.is_empty(),
                "{} unexpectedly grew an alias",
                entry.code
            );
        }
    }

    #[test]
    fn every_entry_has_closed_recovery_and_actionability() {
        for entry in catalog() {
            assert_ne!(
                entry.actionability,
                FixInstructionState::Stale,
                "{} must not default to stale; staleness is instance-only",
                entry.code
            );
            if entry.recovery_kinds.is_empty() {
                assert!(
                    entry.unavailable_reason.is_some(),
                    "{} has no recovery kinds but no typed unavailable reason",
                    entry.code
                );
            } else {
                assert!(
                    entry.unavailable_reason.is_none(),
                    "{} lists recovery kinds and must not also claim unavailable",
                    entry.code
                );
            }
            assert!(
                !entry.non_claims.is_empty(),
                "{} must carry honesty non-claims",
                entry.code
            );
        }
    }

    #[test]
    fn named_fixture_families_earn_catalog_metadata() -> Result<(), String> {
        let actionable = [
            "weakly_exposed",
            "reachable_unrevealed",
            "no_static_path",
            "ripr-gap-MissingBoundaryAssertion",
            "ripr-gap-MissingValueAssertion",
            "ripr-gap-MissingErrorDiscriminator",
            "ripr-gap-MissingOutputContract",
            "ripr-gap-MissingSideEffectObserver",
        ];
        for code in actionable {
            let entry = require(code)?;
            if entry.actionability != FixInstructionState::FixSiteReady {
                return Err(format!("{code} should be the actionable family"));
            }
            if entry.default_visibility != CatalogProfileVisibility::ActionableAndFull {
                return Err(format!(
                    "{code} should be visible in the actionable profile"
                ));
            }
            if allow_recovery(entry, CatalogRecoveryKind::OpenFixSite).is_none() {
                return Err(format!("{code} must allow open_fix_site"));
            }
        }

        let inspect_only = [
            "exposed",
            "ripr-seam-strongly-gripped",
            "ripr-gap-NoActionAlreadyObserved",
        ];
        for code in inspect_only {
            let entry = require(code)?;
            if entry.actionability != FixInstructionState::InspectOnly {
                return Err(format!("{code} should be inspect-only"));
            }
            if allow_recovery(entry, CatalogRecoveryKind::InspectEvidence).is_none() {
                return Err(format!("{code} must allow inspect_evidence"));
            }
            if allow_recovery(entry, CatalogRecoveryKind::OpenFixSite).is_some() {
                return Err(format!("{code} must not advertise an open/repair recovery"));
            }
        }

        let static_limited = [
            "static_unknown",
            "infection_unknown",
            "propagation_unknown",
            "ripr-gap-StaticLimitation",
            "ripr-seam-opaque",
        ];
        for code in static_limited {
            let entry = require(code)?;
            if entry.actionability != FixInstructionState::StaticLimitation {
                return Err(format!("{code} should be static-limited"));
            }
            if !entry.recovery_kinds.is_empty()
                || entry.unavailable_reason != Some(CatalogUnavailableReason::NoBoundedAction)
            {
                return Err(format!("{code} must stay typed no_bounded_action"));
            }
        }

        let pending = require(DIAGNOSTIC_ANALYSIS_PENDING_CODE)?;
        if pending.actionability != FixInstructionState::InspectOnly {
            return Err("pending should be inspect-only".to_string());
        }
        if !pending.preconditions.is_empty() {
            return Err("no-snapshot has no completed-result precondition".to_string());
        }
        if allow_recovery(pending, CatalogRecoveryKind::RefreshAnalysis).is_none()
            || allow_recovery(pending, CatalogRecoveryKind::SelectWorkspaceRoot).is_none()
            || allow_recovery(pending, CatalogRecoveryKind::RepairConfiguration).is_none()
        {
            return Err(
                "pending must allow refresh, wrong-root, and configuration recovery".to_string(),
            );
        }

        let unsupported = require(DIAGNOSTIC_LANGUAGE_NOT_ANALYZED_CODE)?;
        if unsupported.actionability != FixInstructionState::InspectOnly
            || !unsupported.recovery_kinds.is_empty()
            || unsupported.unavailable_reason != Some(CatalogUnavailableReason::NoBoundedAction)
        {
            return Err("unsupported must stay typed no_bounded_action".to_string());
        }

        for code in [DIFF_SCOPE_OVERSIZED_CODE, DIAGNOSTIC_BUDGET_OMITTED_CODE] {
            let entry = require(code)?;
            if entry.actionability != FixInstructionState::InspectOnly
                || allow_recovery(entry, CatalogRecoveryKind::RetrieveOverflow)
                    != Some(CatalogRecoveryKind::RetrieveOverflow)
                || !entry
                    .preconditions
                    .contains(&CatalogPrecondition::CompletedResult)
            {
                return Err(format!("{code} should be the overflowed family"));
            }
        }
        Ok(())
    }

    #[test]
    fn stale_is_instance_join_not_a_catalog_default() -> Result<(), String> {
        let actionable = require("weakly_exposed")?;
        if !actionable
            .preconditions
            .contains(&CatalogPrecondition::CurrentSnapshot)
        {
            return Err("actionable codes require a current snapshot".to_string());
        }
        if join_instruction_state(actionable.actionability, FixInstructionState::Stale)
            != FixInstructionState::Stale
        {
            return Err("stale instance state must win the join".to_string());
        }
        if join_instruction_state(
            FixInstructionState::StaticLimitation,
            FixInstructionState::FixSiteReady,
        ) != FixInstructionState::StaticLimitation
        {
            return Err("join must not strengthen a static-limited catalog default".to_string());
        }
        if join_instruction_state(
            FixInstructionState::InspectOnly,
            FixInstructionState::FixSiteReady,
        ) != FixInstructionState::InspectOnly
        {
            return Err("join must not promote inspect-only to fix_site_ready".to_string());
        }
        Ok(())
    }

    #[test]
    fn missing_recovery_stays_typed_unavailable() -> Result<(), String> {
        let entry = require("ripr-gap-Unknown")?;
        if allow_recovery(entry, CatalogRecoveryKind::RefreshAnalysis).is_some()
            || allow_recovery(entry, CatalogRecoveryKind::OpenFixSite).is_some()
        {
            return Err("unknown gap must not guess a recovery".to_string());
        }
        if entry.unavailable_reason != Some(CatalogUnavailableReason::MissingProducerEvidence)
            || entry
                .unavailable_reason
                .map(CatalogUnavailableReason::as_str)
                != Some("missing_producer_evidence")
        {
            return Err("unknown gap must stay typed missing_producer_evidence".to_string());
        }
        Ok(())
    }

    #[test]
    fn client_capability_cannot_change_catalog_state() -> Result<(), String> {
        let full = metadata_for_client("weakly_exposed", CatalogClientView::FULL)
            .ok_or_else(|| "weakly_exposed is catalogued".to_string())?;
        let minimal = metadata_for_client("weakly_exposed", CatalogClientView::MINIMAL)
            .ok_or_else(|| "weakly_exposed is catalogued".to_string())?;
        if !full.present_optional_fields || minimal.present_optional_fields {
            return Err("capability should only toggle optional presentation".to_string());
        }
        if full.actionability != minimal.actionability
            || full.recovery_kinds != minimal.recovery_kinds
            || full.unavailable_reason != minimal.unavailable_reason
            || full.default_visibility != minimal.default_visibility
            || full.severity_source != minimal.severity_source
            || full.default_severity != minimal.default_severity
            || full.preconditions != minimal.preconditions
            || full.non_claims != minimal.non_claims
            || full.code != "weakly_exposed"
        {
            return Err("capability must not change catalog state".to_string());
        }
        Ok(())
    }

    #[test]
    fn recovery_kind_names_are_stable() -> Result<(), String> {
        let recovery = [
            (CatalogRecoveryKind::InspectEvidence, "inspect_evidence"),
            (
                CatalogRecoveryKind::InspectFullProfile,
                "inspect_full_profile",
            ),
            (CatalogRecoveryKind::OpenFixSite, "open_fix_site"),
            (CatalogRecoveryKind::RefreshAnalysis, "refresh_analysis"),
            (CatalogRecoveryKind::Retry, "retry"),
            (
                CatalogRecoveryKind::SelectWorkspaceRoot,
                "select_workspace_root",
            ),
            (
                CatalogRecoveryKind::RepairConfiguration,
                "repair_configuration",
            ),
            (CatalogRecoveryKind::RetrieveOverflow, "retrieve_overflow"),
            (CatalogRecoveryKind::RunVerification, "run_verification"),
            (CatalogRecoveryKind::RecordReceipt, "record_receipt"),
        ];
        for (kind, name) in recovery {
            if kind.as_str() != name {
                return Err(format!("recovery kind name drifted: {}", kind.as_str()));
            }
        }
        let unavailable = [
            (
                CatalogUnavailableReason::NoBoundedAction,
                "no_bounded_action",
            ),
            (
                CatalogUnavailableReason::MissingProducerEvidence,
                "missing_producer_evidence",
            ),
            (
                CatalogUnavailableReason::UnsafeWithoutRefresh,
                "unsafe_without_refresh",
            ),
        ];
        for (reason, name) in unavailable {
            if reason.as_str() != name {
                return Err(format!(
                    "unavailable reason name drifted: {}",
                    reason.as_str()
                ));
            }
        }
        if CatalogProfileVisibility::ActionableAndFull.as_str() != "actionable_and_full"
            || CatalogProfileVisibility::FullOnly.as_str() != "full_only"
            || CatalogProfileVisibility::Always.as_str() != "always"
            || CatalogSeveritySource::AdvisoryPolicy.as_str() != "advisory_policy"
            || CatalogSeveritySource::SeverityConfig.as_str() != "severity_config"
            || CatalogSeveritySource::DeliveryLimitation.as_str() != "delivery_limitation"
            || CatalogDefaultSeverity::Warning.as_str() != "warning"
            || CatalogDefaultSeverity::Information.as_str() != "information"
            || CatalogDefaultSeverity::Hint.as_str() != "hint"
            || CatalogPrecondition::CurrentSnapshot.as_str() != "current_snapshot"
            || CatalogPrecondition::CompletedResult.as_str() != "completed_result"
            || CatalogNonClaim::RuntimeProof.as_str() != "not_runtime_proof"
            || CatalogNonClaim::TestAdequacy.as_str() != "not_test_adequacy"
            || CatalogNonClaim::RepairAuthorization.as_str() != "not_repair_authorization"
            || CatalogNonClaim::CleanWorkspace.as_str() != "not_clean_workspace"
            || CatalogNonClaim::SupportPromotion.as_str() != "not_support_promotion"
        {
            return Err("closed catalog vocabulary names drifted".to_string());
        }
        Ok(())
    }
}
