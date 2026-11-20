use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{
    ExposureClass, OracleKind, OracleStrength, ProbeId, RiprEvidence, SourceLocation, SymbolId,
};

/// The shape of changed behavior a probe stands for. Serialized as the
/// snake_case `family` token in JSON output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeFamily {
    /// `predicate`: a changed condition or comparison that decides a branch.
    Predicate,
    /// `return_value`: a changed value the function returns or computes.
    ReturnValue,
    /// `error_path`: a changed error path, such as an error return or raise.
    ErrorPath,
    /// `call_deletion`: a call site the diff removed.
    CallDeletion,
    /// `field_construction`: a changed field value in a constructed struct
    /// or object.
    FieldConstruction,
    /// `side_effect`: a changed effect outside the return value, such as a
    /// call made for its effect or a process exit code.
    SideEffect,
    /// `match_arm`: a changed match arm.
    MatchArm,
    /// `static_unknown`: a changed line whose behavior shape `ripr` could not
    /// classify.
    StaticUnknown,
}

impl ProbeFamily {
    /// Families `ripr check` resolves a stub route for: the stub producer
    /// either writes a test or names why it cannot. `static_unknown` has no
    /// seam to resolve.
    pub fn offers_stub_route(&self) -> bool {
        !matches!(self, ProbeFamily::StaticUnknown)
    }

    /// Inverse of [`ProbeFamily::as_str`] for projections that hold only the
    /// wire token (`ContextPacketProbe::family`). `None` for an unknown token.
    pub fn from_wire(token: &str) -> Option<Self> {
        Some(match token {
            "predicate" => ProbeFamily::Predicate,
            "return_value" => ProbeFamily::ReturnValue,
            "error_path" => ProbeFamily::ErrorPath,
            "call_deletion" => ProbeFamily::CallDeletion,
            "field_construction" => ProbeFamily::FieldConstruction,
            "side_effect" => ProbeFamily::SideEffect,
            "match_arm" => ProbeFamily::MatchArm,
            "static_unknown" => ProbeFamily::StaticUnknown,
            _ => return None,
        })
    }

    /// The snake_case wire token, as serialized in JSON output.
    pub fn as_str(&self) -> &'static str {
        match self {
            ProbeFamily::Predicate => "predicate",
            ProbeFamily::ReturnValue => "return_value",
            ProbeFamily::ErrorPath => "error_path",
            ProbeFamily::CallDeletion => "call_deletion",
            ProbeFamily::FieldConstruction => "field_construction",
            ProbeFamily::SideEffect => "side_effect",
            ProbeFamily::MatchArm => "match_arm",
            ProbeFamily::StaticUnknown => "static_unknown",
        }
    }
}

/// What kind of difference a probe's change makes. Serialized as the
/// snake_case `delta` token in JSON output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeltaKind {
    /// `value`: a different value is produced.
    Value,
    /// `control`: a different branch or path is taken.
    Control,
    /// `effect`: a different effect happens outside the returned value.
    Effect,
    /// `unknown`: the kind of difference is not known.
    Unknown,
}

impl DeltaKind {
    /// The snake_case wire token, as serialized in JSON output.
    pub fn as_str(&self) -> &'static str {
        match self {
            DeltaKind::Value => "value",
            DeltaKind::Control => "control",
            DeltaKind::Effect => "effect",
            DeltaKind::Unknown => "unknown",
        }
    }
}

/// Why static analysis stopped short for a finding. Serialized as snake_case
/// tokens in `stop_reasons`. RIPR-SPEC-0411 lists each value's role and
/// current producer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// `max_depth_reached`: reserved wire value; not emitted today.
    MaxDepthReached,
    /// `external_crate_boundary`: reserved wire value; not emitted today.
    ExternalCrateBoundary,
    /// `dynamic_dispatch_unresolved`: the path goes through a dynamic call
    /// `ripr` cannot resolve statically.
    DynamicDispatchUnresolved,
    /// `proc_macro_opaque`: heuristic annotation. The changed expression
    /// contains a macro call (`name!(`) whose expansion `ripr` does not see.
    ProcMacroOpaque,
    /// `fixture_opaque`: heuristic annotation. A related test mentions a
    /// fixture, builder, or generator, so its inputs may come from values
    /// `ripr` does not evaluate.
    FixtureOpaque,
    /// `feature_unknown`: reserved wire value; not emitted today.
    FeatureUnknown,
    /// `async_boundary_opaque`: heuristic annotation. The changed expression
    /// names `async`, `spawn`, or `await`, so the path may cross a task
    /// boundary `ripr` does not follow. A name match alone does not mean a
    /// boundary is crossed.
    AsyncBoundaryOpaque,
    /// `no_changed_rust_line`: heuristic annotation. `ripr` could not place
    /// the changed line inside a Rust function it analyzes.
    NoChangedRustLine,
    /// `infection_evidence_unknown`: added when a finding is
    /// `infection_unknown` and has no more specific reason.
    InfectionEvidenceUnknown,
    /// `propagation_evidence_unknown`: added when a finding is
    /// `propagation_unknown` and has no more specific reason.
    PropagationEvidenceUnknown,
    /// `static_probe_unknown`: added when a finding is `static_unknown` and
    /// has no more specific reason.
    StaticProbeUnknown,
    /// Reach through internal calls is unresolved. A bounded walk may retain
    /// a candidate path (RIPR-SPEC-0114), but an unmodeled private-member
    /// boundary establishes no path either way (RIPR-SPEC-0243).
    TransitiveReachUnresolved,
    /// A Rust reach walk found a candidate test entry path that stops at a
    /// same-repo macro invocation whose definition lexically names the changed
    /// owner. ripr does not expand the macro; classification stays
    /// `no_static_path`. See RIPR-SPEC-0117.
    MacroReachUnresolved,
    /// ripr could not establish the evidence a gap claim needs (today, that
    /// the refused related assertions do not run), so the finding is an
    /// unknown rather than a gap. A `static_limit_kind` names the missing
    /// link. See RIPR-SPEC-0240.
    GapEvidenceUnresolved,
    /// A helper-chain uniqueness stop that tests enter: ripr saw a call of
    /// the ambiguous name but cannot bind it to one workspace function.
    /// Classification is `static_unknown`. See RIPR-SPEC-0159 / #7272.
    HelperIdentityUnresolved,
    /// The changed line is a compile-time assertion (`const _: () = assert!(..)`,
    /// `static_assertions::const_assert!`). A violating edit fails to build; it
    /// is not a test-kill question, so ripr states that instead of placing the
    /// line in a function. Classification stays `static_unknown`, never
    /// `exposed` (#11547).
    CompileTimeInvariant,
    /// The changed code states an unsafe contract the compiler does not
    /// check: an `unsafe fn` (callers now carry a safety obligation) or an
    /// unchecked assumption (`unreachable_unchecked`, `assert_unchecked`).
    /// A safe-code test cannot discharge it. Classification stays
    /// `static_unknown`, never `exposed` (#11549).
    UnsafeObligation,
}

impl StopReason {
    /// The snake_case wire token, as serialized in JSON output.
    pub fn as_str(&self) -> &'static str {
        match self {
            StopReason::MaxDepthReached => "max_depth_reached",
            StopReason::ExternalCrateBoundary => "external_crate_boundary",
            StopReason::DynamicDispatchUnresolved => "dynamic_dispatch_unresolved",
            StopReason::ProcMacroOpaque => "proc_macro_opaque",
            StopReason::FixtureOpaque => "fixture_opaque",
            StopReason::FeatureUnknown => "feature_unknown",
            StopReason::AsyncBoundaryOpaque => "async_boundary_opaque",
            StopReason::NoChangedRustLine => "no_changed_rust_line",
            StopReason::InfectionEvidenceUnknown => "infection_evidence_unknown",
            StopReason::PropagationEvidenceUnknown => "propagation_evidence_unknown",
            StopReason::StaticProbeUnknown => "static_probe_unknown",
            StopReason::TransitiveReachUnresolved => "transitive_reach_unresolved",
            StopReason::MacroReachUnresolved => "macro_reach_unresolved",
            StopReason::GapEvidenceUnresolved => "gap_evidence_unresolved",
            StopReason::HelperIdentityUnresolved => "helper_identity_unresolved",
            StopReason::CompileTimeInvariant => "compile_time_invariant",
            StopReason::UnsafeObligation => "unsafe_obligation",
        }
    }

    /// Plain-English gloss for human output (#4323). The snake_case token
    /// stays the machine identity; this says why the static path stopped. Each
    /// gloss names a limit of ripr's model, never a coverage or runtime claim.
    /// Reasons produced by a lexical heuristic (`FixtureOpaque`,
    /// `AsyncBoundaryOpaque`) say "mentions ... may", not a confirmed fact.
    pub fn describe(&self) -> &'static str {
        match self {
            StopReason::MaxDepthReached => {
                "the bounded call walk reached its depth limit before finding a test"
            }
            StopReason::ExternalCrateBoundary => {
                "the path crosses into another crate, which ripr does not analyze"
            }
            StopReason::DynamicDispatchUnresolved => {
                "the path goes through a dynamic call ripr cannot resolve statically"
            }
            StopReason::ProcMacroOpaque => {
                "the changed expression contains a macro call whose expansion ripr does not see"
            }
            StopReason::FixtureOpaque => {
                "a related test mentions a fixture, builder, or generator, so its inputs may \
                 come from values ripr does not evaluate"
            }
            StopReason::FeatureUnknown => {
                "the code depends on a Cargo feature or cfg ripr could not resolve"
            }
            StopReason::AsyncBoundaryOpaque => {
                "the changed expression mentions async, spawn, or await, so the path may cross \
                 a task boundary ripr does not follow"
            }
            StopReason::NoChangedRustLine => {
                "ripr could not place the changed line inside a Rust function it analyzes"
            }
            StopReason::InfectionEvidenceUnknown => {
                "ripr could not tell whether a test input reaches a value that makes the \
                 change matter"
            }
            StopReason::PropagationEvidenceUnknown => {
                "ripr could not tell whether the changed value flows to something a test \
                 observes"
            }
            StopReason::StaticProbeUnknown => {
                "ripr could not model this change well enough to classify it"
            }
            StopReason::TransitiveReachUnresolved => {
                "ripr could not establish whether a test reaches this change through internal calls"
            }
            StopReason::MacroReachUnresolved => {
                "a candidate test path stops at a same-repo macro ripr does not expand"
            }
            StopReason::GapEvidenceUnresolved => {
                "ripr could not establish the evidence a gap needs for this change, so it does \
                 not claim one"
            }
            StopReason::HelperIdentityUnresolved => {
                "ripr cannot tell which workspace function a test call names because that name \
                 is not unique"
            }
            StopReason::CompileTimeInvariant => {
                "the changed line is a compile-time assertion, so a violating edit fails to \
                 build instead of failing a test"
            }
            StopReason::UnsafeObligation => {
                "the change adds or alters an unsafe contract (an unsafe function or an \
                 unchecked assumption) that safe test code cannot discharge"
            }
        }
    }

    /// The generic stop reason for an unknown class: `infection_evidence_unknown`,
    /// `propagation_evidence_unknown` or `static_probe_unknown` for the
    /// matching `*_unknown` class, and `None` for every other class. The
    /// classifier adds it when an unknown finding has no more specific stop
    /// reason.
    pub fn for_unknown_class(class: &ExposureClass) -> Option<Self> {
        match class {
            ExposureClass::InfectionUnknown => Some(StopReason::InfectionEvidenceUnknown),
            ExposureClass::PropagationUnknown => Some(StopReason::PropagationEvidenceUnknown),
            ExposureClass::StaticUnknown => Some(StopReason::StaticProbeUnknown),
            _ => None,
        }
    }
}

/// One changed behavior that ripr checks the tests against: a single changed
/// line, the family of change it is, and what a test would need to observe
/// to notice the change.
///
/// A probe is static evidence about the diff. It names where the change is
/// and what kind of oracle could discriminate it; it does not claim that any
/// mutant of the line survives or is killed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Probe {
    /// Identifier for this probe within one run. A [`Finding`] built from the
    /// probe reuses it as the finding id.
    pub id: ProbeId,
    /// Where the changed line is. For a diff probe the line is the new-file
    /// (head-side) coordinate, also for a removed line.
    pub location: SourceLocation,
    /// The function or other owner that contains the changed line, when one
    /// was found. `None` for a change outside any owner and for a line of a
    /// function the diff removed entirely.
    pub owner: Option<SymbolId>,
    /// What kind of change this is.
    pub family: ProbeFamily,
    /// What kind of program delta the family implies.
    pub delta: DeltaKind,
    /// Base-side text of the changed line, when the diff removed or replaced
    /// one. `None` for a purely added line.
    pub before: Option<String>,
    /// Head-side text of the changed line, when the diff added or replaced
    /// one. `None` for a removed-only line.
    pub after: Option<String>,
    /// The changed source text, trimmed, as the analysis read it.
    pub expression: String,
    /// Places a test could observe this change, such as `return value` or
    /// `field:<name>`. Sorted and deduplicated; empty when none are named.
    pub expected_sinks: Vec<String>,
    /// Oracle shapes that would discriminate this change, such as
    /// `exact error variant assertion`. Sorted and deduplicated; empty when
    /// none are named.
    pub required_oracles: Vec<String>,
}

impl Probe {
    /// The name of the `const` or `static` item this probe's changed line
    /// declares, for a `static_unknown` probe. A changed constant has no seam
    /// of its own, so `ripr check` routes it to the functions that consume it.
    pub fn changed_const_name(&self) -> Option<&str> {
        if self.family != ProbeFamily::StaticUnknown {
            return None;
        }
        declared_const_name(&self.expression)
    }

    /// Whether `ripr check` resolves a stub route for this probe.
    pub fn offers_stub_route(&self) -> bool {
        self.family.offers_stub_route() || self.changed_const_name().is_some()
    }
}

/// The name of the `const` or `static` item a changed line declares
/// (`pub const GREETING: &str = "hi";` names `GREETING`), when the line holds
/// its initializer. Shared by the static-unknown const probe and the #11400
/// reader retarget, so both read one declaration rule.
pub fn declared_const_name(expression: &str) -> Option<&str> {
    let mut rest = expression.trim_start();
    if let Some(after_pub) = rest.strip_prefix("pub") {
        rest = after_pub.trim_start();
        if rest.starts_with('(') {
            rest = rest[rest.find(')')? + 1..].trim_start();
        }
    }
    rest = rest
        .strip_prefix("const ")
        .or_else(|| rest.strip_prefix("static "))?
        .trim_start();
    rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    let declaration = rest[end..].trim_start();
    // An initializer on the line separates an item from a generics or
    // parameter continuation line such as `const N: usize,`.
    let named_item = !name.is_empty()
        && name != "_"
        && declaration.starts_with(':')
        && declaration.contains('=');
    named_item.then_some(name)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowSinkKind {
    ReturnValue,
    ErrorVariant,
    StructField,
    EventCall,
    StateWrite,
    Persistence,
    LogMessage,
    ConfigChange,
    CallEffect,
    MatchArm,
    Unknown,
}

impl FlowSinkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FlowSinkKind::ReturnValue => "return_value",
            FlowSinkKind::ErrorVariant => "error_variant",
            FlowSinkKind::StructField => "struct_field",
            FlowSinkKind::EventCall => "event_call",
            FlowSinkKind::StateWrite => "state_write",
            FlowSinkKind::Persistence => "persistence",
            FlowSinkKind::LogMessage => "log_message",
            FlowSinkKind::ConfigChange => "config_change",
            FlowSinkKind::CallEffect => "call_effect",
            FlowSinkKind::MatchArm => "match_arm",
            FlowSinkKind::Unknown => "unknown",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FlowSinkKind::ReturnValue => "returned value",
            FlowSinkKind::ErrorVariant => "error variant",
            FlowSinkKind::StructField => "constructed field",
            FlowSinkKind::EventCall => "event or outbound call",
            FlowSinkKind::StateWrite => "state write",
            FlowSinkKind::Persistence => "persistence write",
            FlowSinkKind::LogMessage => "log message",
            FlowSinkKind::ConfigChange => "configuration change",
            FlowSinkKind::CallEffect => "call effect",
            FlowSinkKind::MatchArm => "match arm result",
            FlowSinkKind::Unknown => "unknown sink",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowSinkFact {
    pub kind: FlowSinkKind,
    pub text: String,
    pub line: usize,
    pub owner: Option<SymbolId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueContext {
    FunctionArgument,
    AssertionArgument,
    BuilderMethod,
    TableRow,
    EnumVariant,
    /// A qualified path whose spelling establishes a constant (`u64::MAX`,
    /// `crate::KIB`), not an enum variant (#5357).
    Constant,
    ReturnValue,
    Unknown,
}

impl ValueContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValueContext::FunctionArgument => "function_argument",
            ValueContext::AssertionArgument => "assertion_argument",
            ValueContext::BuilderMethod => "builder_method",
            ValueContext::TableRow => "table_row",
            ValueContext::EnumVariant => "enum_variant",
            ValueContext::Constant => "constant",
            ValueContext::ReturnValue => "return_value",
            ValueContext::Unknown => "unknown",
        }
    }
}

/// A value found or statically derived from test source.
///
/// This fact does not establish assertion execution, oracle admission or
/// observation of the changed behavior. It can retain values from refused
/// assertions and bounded static value-transfer expressions. `context` records
/// the source/value origin, not an execution state.
///
/// The `text` field holds retained source or value-transfer provenance. The
/// JSON renderer deduplicates shared text into the finding-level
/// `assertion_texts` map keyed by line number. Per-value objects retain optional
/// `provenance` when their text differs from that shared entry; otherwise
/// consumers recover it via `finding.assertion_texts[line.to_string()]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFact {
    pub line: usize,
    pub text: String,
    pub value: String,
    pub context: ValueContext,
}

/// Whether a missing-discriminator recommendation names an equality input
/// boundary, the activating domain of a guard, or some other shape. The
/// consumers that used to string-match `==` in [`MissingDiscriminatorFact`]
/// value (the predicate decision suppression, the predicate infection
/// summary, [`input_boundary_fact`]) read this instead, so a new fact shape
/// cannot silently fall into the equality reading (#12414).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscriminatorShape {
    /// The equality input boundary a changed comparison sits on:
    /// `left == right`, the input no related test supplies
    /// (`classify::activation::missing_boundary_discriminator`).
    Boundary,
    /// The activating domain of a guard: `if <parameter> <op> <bound>`, the
    /// region no related test's input enters (RIPR-SPEC-0263, and the
    /// inserted-guard route of #12414).
    GuardDomain,
    /// Any other recommendation (an unselected arm, an exact assertion, a
    /// named value). No consumer selects it by equality or guard shape.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingDiscriminatorFact {
    pub value: String,
    pub reason: String,
    pub flow_sink: Option<FlowSinkFact>,
    /// The concrete drafted boundary call for the equality boundary this
    /// fact names, when the analyzer's static model can pin the boundary
    /// input from the related tests' own call rows. `expected` stays `None`
    /// when the owner's returned expression does not fold at that input;
    /// the rendered assertion then keeps its `/* expected */` placeholder.
    /// This is a static draft: ripr has not run or verified anything, and
    /// the developer confirms the expected value before relying on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drafted_boundary: Option<DraftedBoundaryAssertion>,
    /// What the recommendation names. No serde default: a cached fact or
    /// persisted evidence written before the field existed must fail to
    /// decode rather than silently re-read as the wrong shape, mirroring the
    /// `statically_contradicted_related_tests` precedent (#9110); the seam
    /// cache schema versions bump with this field.
    pub shape: DiscriminatorShape,
}

/// The owner-call argument texts a drafted boundary assertion passes, plus
/// the statically folded expected value when the analyzer's bounded integer
/// fold derives one. Argument texts are plain integer literals, so the
/// drafted call is paste-ready; the expected value is a fold of the current
/// behavior, not an observed or verified result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftedBoundaryAssertion {
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

impl MissingDiscriminatorFact {
    /// The fact for an emitters whose recommendation is spelled
    /// `left == right` only sometimes (the syntax-first language previews,
    /// the bun bridge, the Perl preview): the value keeps the equality
    /// reading exactly when it is equality-spelled, the same rule the
    /// `==`-string readers applied before the typed shape existed (#12414).
    /// Emitters whose value format is fixed use the shape field directly.
    pub fn equality_spelled(
        value: String,
        reason: String,
        flow_sink: Option<FlowSinkFact>,
    ) -> Self {
        let shape = if value.contains(" == ") {
            DiscriminatorShape::Boundary
        } else {
            DiscriminatorShape::Other
        };
        Self {
            value,
            reason,
            flow_sink,
            shape,
            drafted_boundary: None,
        }
    }
}

/// Opens the reason of a match-arm missing discriminator that names an arm
/// no related test's input selects (RIPR-SPEC-0229). Shared so the analyzer
/// that writes it and the miss renderer that reads it agree.
pub const ARM_UNSELECTED_REASON_PREFIX: &str = "No related test call selects arm";

/// Opens the reason of a side-effect, call-deletion, error-path or
/// error-variant return-value missing discriminator that names an enclosing `if` no related test's input enters
/// (RIPR-SPEC-0263). Shared like [`ARM_UNSELECTED_REASON_PREFIX`].
pub const GUARD_UNENTERED_REASON_PREFIX: &str = "No related test call enters guard";

/// Whether a missing-discriminator reason names an input no related test
/// supplies at all: an arm no input selects (RIPR-SPEC-0229) or a guard no
/// input enters (RIPR-SPEC-0263). Such a probe never runs under the tests.
pub fn is_unselected_input_reason(reason: &str) -> bool {
    reason.starts_with(ARM_UNSELECTED_REASON_PREFIX)
        || reason.starts_with(GUARD_UNENTERED_REASON_PREFIX)
}

/// The missing discriminator that names an input no test supplies: for a
/// predicate probe the boundary `left == right` it never reaches
/// (`classify::activation::missing_boundary_discriminator`), for a match-arm
/// probe the arm no test input selects, and for a side-effect,
/// call-deletion, error-path or error-variant return-value probe the
/// enclosing `if` no test input enters. Error-variant and field facts name
/// an assertion that is missing instead.
pub fn input_boundary_fact<'a>(
    facts: &'a [MissingDiscriminatorFact],
    family: &ProbeFamily,
) -> Option<&'a MissingDiscriminatorFact> {
    let boundary = |facts: &'a [MissingDiscriminatorFact]| {
        facts
            .iter()
            .find(|fact| fact.shape == DiscriminatorShape::Boundary)
    };
    let guard = |facts: &'a [MissingDiscriminatorFact]| {
        facts
            .iter()
            .find(|fact| fact.shape == DiscriminatorShape::GuardDomain)
    };
    match family {
        // A returned bare comparison reads the same boundary (#9094); no
        // other return-value fact is spelled `left == right`.
        // An inserted predicate names its guard domain instead (#12414).
        ProbeFamily::Predicate => boundary(facts).or_else(|| guard(facts)),
        // A returned error variant under an unentered `if` reads the guard
        // (#11147), after the bare-comparison reading above.
        ProbeFamily::ReturnValue => boundary(facts).or_else(|| guard(facts)),
        ProbeFamily::MatchArm => facts
            .iter()
            .find(|fact| fact.reason.starts_with(ARM_UNSELECTED_REASON_PREFIX)),
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion | ProbeFamily::ErrorPath => facts
            .iter()
            .find(|fact| fact.reason.starts_with(GUARD_UNENTERED_REASON_PREFIX)),
        _ => None,
    }
}

/// Row-local exact-discriminator support for #5575: typed value/field/error/effect
/// facts only, never input-activation gaps (guard unentered, unselected arm).
pub fn row_exact_discriminator_fact<'a>(
    facts: &'a [MissingDiscriminatorFact],
    family: &ProbeFamily,
) -> Option<&'a MissingDiscriminatorFact> {
    let boundary = input_boundary_fact(facts, family);
    facts.iter().find(|fact| {
        !is_unselected_input_reason(&fact.reason)
            && boundary.is_none_or(|boundary| !std::ptr::eq(*fact, boundary))
    })
}

/// The missing discriminator that names an exact assertion no test makes
/// (an error variant or a constructed field): the first fact that is not the
/// predicate input boundary.
pub fn exact_assertion_fact<'a>(
    facts: &'a [MissingDiscriminatorFact],
    family: &ProbeFamily,
) -> Option<&'a MissingDiscriminatorFact> {
    let boundary = input_boundary_fact(facts, family);
    facts
        .iter()
        .find(|fact| boundary.is_none_or(|boundary| !std::ptr::eq(*fact, boundary)))
}

/// Prefix the classifier puts on the value-shaped entries of `Finding.missing`.
///
/// The field mixes value-shaped entries (`Missing check value: X`,
/// built from a [`MissingDiscriminatorFact`]) with prose entries ("No strong
/// discriminator was detected"), so consumers that already print a
/// "Missing discriminator" label must strip this prefix to avoid restating it.
/// Shared here because the classifier writes it and the renderers read it.
///
/// Crate-private: this is an internal formatting convention, not a contract
/// embedders may bind to. `lib.rs` re-exports `pub mod domain`, so a `pub`
/// constant here would publish the exact wording as library API.
pub const MISSING_DISCRIMINATOR_VALUE_PREFIX: &str = "Missing check value: ";

/// Reach-summary suffix for an owner no source, test or doc in the workspace
/// names (RIPR-SPEC-0219). The verdict corpus keys on this wording, and
/// [`Finding::is_unreferenced_no_static_path`] reads it back (#14820).
pub const UNREFERENCED_IN_WORKSPACE: &str = "(unreferenced in workspace)";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingCanonicalGap {
    pub id: String,
    pub language: String,
    pub file: String,
    pub owner: String,
    pub behavior_kind: String,
    pub probe_kind: String,
    pub normalized_discriminator: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationEvidence {
    /// Historical field name for source values; see ValueFact's non-execution
    /// contract. Oracle observation remains a separate admitted stage.
    pub observed_values: Vec<ValueFact>,
    pub missing_discriminators: Vec<MissingDiscriminatorFact>,
}

/// Next-step text for a finding whose related test has a strong check that
/// ripr cannot tie to the changed line. The TypeScript classifier writes it
/// and the preview card reads it, so both stay in step (#12463).
pub const STRONG_CHECK_UNTIED_NEXT_STEP: &str = "TypeScript preview advisory: the related test's check is already strong, but ripr cannot tie it to the changed line; exercise the changed branch with an input that flips it. No actionable repair packet is emitted until the target shape is explicit.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedTest {
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
    pub oracle: Option<String>,
    pub oracle_kind: OracleKind,
    pub oracle_strength: OracleStrength,
    /// Why this test was related to the probe. `None` when the match origin
    /// is unknown (legacy callers). `Some` for all diff-check findings that
    /// go through `classify/related_tests.rs`.
    pub relation_reason: Option<crate::domain::RelationReason>,
    /// Confidence that this test grips the changed behavior. `None` when
    /// `relation_reason` is `None`. Derived from `relation_reason` when set.
    pub relation_confidence: Option<crate::domain::RelationConfidence>,
    /// Why this test does not catch the changed behavior, when ripr can say.
    /// `None` for a test whose oracle supports the finding's class, and for
    /// findings from producers that do not record it. Explanation only: no
    /// decision helper reads this field except the refused-assertion exception
    /// of the opaque assertion-macro limit (`AssertionNotCredited`, RIPR-SPEC-0120,
    /// #9644); row eligibility is [`Self::role`] (#5563). Display pickers read
    /// it through [`Self::observation_tier`] to choose which row to name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miss: Option<RelatedTestMiss>,
    /// Whether this row belonged to the legacy decision set or was retained
    /// only so an examined test can be listed (#5563). Independent of `miss`:
    /// any miss value may be attached to a row of either role without
    /// changing which decision paths consume it. Absent in older artifacts,
    /// which read as `contributing` (every row they carry was part of that
    /// artifact's decision set).
    #[serde(default, skip_serializing_if = "RelatedTestRole::is_contributing")]
    pub role: RelatedTestRole,
    /// The one typed fact the producer established for **this row** that
    /// backs a `missing_input` or `missing_exact_assertion` miss (#5564).
    /// `None` when the producer could not establish row ownership; renderers
    /// then use generic wording and never borrow a finding-wide
    /// `missing_discriminators` entry. Evidence only: no decision reads it
    /// and it is not part of finding identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miss_support: Option<RelatedTestMissSupport>,
}

/// Row-owned supporting fact for a discriminator-backed [`RelatedTestMiss`]
/// (#5564). Semantic data, not prose: the shared renderer chooses the wording
/// from the miss, the finding's probe family and this record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedTestMissSupport {
    /// The value or boundary the row misses (`amount == threshold`).
    pub value: String,
    /// The sink the fact was established against, when the producer knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_sink: Option<FlowSinkFact>,
}

impl RelatedTestMissSupport {
    /// Support carrying the value and sink of a discriminator fact the
    /// producer has established applies to the row it attaches this to.
    pub fn from_fact(fact: &MissingDiscriminatorFact) -> Self {
        Self {
            value: fact.value.clone(),
            flow_sink: fact.flow_sink.clone(),
        }
    }
}

/// The decision role of one related-test row, separate from why it is listed
/// (`relation_reason`) and why it misses the changed behavior (`miss`).
///
/// Registered in `policy/output_contracts.txt` and documented in
/// `docs/OUTPUT_SCHEMA.md`. Not runtime execution evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelatedTestRole {
    /// The row is part of the decision set gates, fix-site selection and
    /// repair routing read. The compatibility default.
    #[default]
    Contributing,
    /// The row was retained only to show an examined test whose assertions
    /// matched nothing; decision paths skip it.
    ExaminedOnly,
}

impl RelatedTestRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contributing => "contributing",
            Self::ExaminedOnly => "examined_only",
        }
    }

    /// `serde` skip predicate: the default role is not serialized.
    pub fn is_contributing(&self) -> bool {
        matches!(self, Self::Contributing)
    }
}

impl RelatedTest {
    /// True for a row in the decision set. Reads the explicit [`RelatedTestRole`],
    /// never `miss`.
    pub fn is_contributing(&self) -> bool {
        self.role.is_contributing()
    }

    /// True for a test ripr examined whose assertions it matched against the
    /// changed behavior and found none that apply. Such a test is listed as
    /// evidence, but it supplied no oracle row, so gates that ask "did any
    /// related test survive oracle matching" must skip it. Reads the explicit
    /// role, never `miss`.
    pub fn is_examined_only(&self) -> bool {
        !self.is_contributing()
    }

    /// How well this row can observe the changed behavior, for every picker
    /// that names one related test to open or edit (#14454, #14483). It reads
    /// only the producer's recorded miss, the one owner of whether a row
    /// reaches the change (a proximity relation can still reach the owner
    /// through a helper and credit exposure). Higher is better:
    /// - `0`: [`RelatedTestMiss::NoCallPath`], no established path to the
    ///   change;
    /// - `1`: [`RelatedTestMiss::AssertionNotObserving`], the test runs the
    ///   change but its assertion pins something the change cannot move;
    /// - `2`: every other row. That includes `no_assertion`, whose contract
    ///   (RIPR-SPEC-0224) requires an established path, so the row is where
    ///   the missing assertion goes; its `none` strength already ranks it
    ///   last where strength decides. A producer that sets it on a row with
    ///   no established path (#14808) lets that row lead first-row pickers.
    ///   Also in `2` are the strength
    ///   misses (`weak_assertion`, `missing_input`, `missing_exact_assertion`)
    ///   and the unknown-edge misses (`observation_unconfirmed`,
    ///   `assertion_not_credited`, `assertion_inert`,
    ///   `assertion_admission_unproven`), which record an observation ripr
    ///   could not establish, not one that is absent (#5508).
    ///
    /// Display row selection only: no class, gate or credit reads it.
    pub fn observation_tier(&self) -> u8 {
        match self.miss {
            Some(RelatedTestMiss::NoCallPath) => 0,
            Some(RelatedTestMiss::AssertionNotObserving) => 1,
            _ => 2,
        }
    }
}

/// The first row of the best [`RelatedTest::observation_tier`], keeping the
/// listed order within that tier. The one observing-first rule for pickers
/// that otherwise take the first listed row (#14483).
pub fn first_observing_related_test(tests: &[RelatedTest]) -> Option<&RelatedTest> {
    // `min_by_key` keeps the first of equal keys.
    tests
        .iter()
        .min_by_key(|test| std::cmp::Reverse(test.observation_tier()))
}

/// Why one examined test would not notice the changed behavior being wrong.
///
/// Each value names a concrete, checkable fact about the test so a reader can
/// open it and agree or disagree. Registered in `policy/output_contracts.txt`
/// and documented in `docs/OUTPUT_SCHEMA.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelatedTestMiss {
    /// Linked by name or file location only; no call from the test to the
    /// changed code was found.
    NoCallPath,
    /// The test contains no assertion ripr recognizes.
    NoAssertion,
    /// The test asserts, but none of its assertions observe the changed value,
    /// error, field, or effect.
    AssertionNotObserving,
    /// The test has an assertion ripr declined to credit because it could not
    /// establish that the assertion runs or what the macro binds to.
    /// Legacy token; producers split this into [`Self::AssertionInert`] and
    /// [`Self::AssertionAdmissionUnproven`] (#5446). Retained for older artifacts.
    AssertionNotCredited,
    /// The test's assertion has a shape that can keep it from running as a
    /// standard check here (for example a shadowed macro, an opaque macro
    /// that may exit early, or a path that may be inactive). A real-gap class
    /// under RIPR-SPEC-0240, not proof that the assertion never runs.
    AssertionInert,
    /// ripr could not establish whether the assertion runs as a standard check;
    /// an analyzer limit, not a confirmed gap (RIPR-SPEC-0221).
    AssertionAdmissionUnproven,
    /// The assertion observes the behavior only weakly (for example
    /// `is_ok()` or a smoke check), so it cannot tell the old value from the
    /// new one.
    WeakAssertion,
    /// The assertion observes the behavior, but no test input reaches the
    /// value that separates the old behavior from the new one.
    MissingInput,
    /// The assertion observes the behavior, but none pins the exact value the
    /// change alters (an error variant or a constructed field), so a wrong
    /// value of the same shape still passes.
    MissingExactAssertion,
    /// The assertion has the right shape, but ripr could not confirm that it
    /// observes this change rather than a sibling value (Rust: its text never
    /// names the changed expression; Perl: the packet establishes no sink
    /// alignment). An unknown, not an established miss.
    ObservationUnconfirmed,
}

impl RelatedTestMiss {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoCallPath => "no_call_path",
            Self::NoAssertion => "no_assertion",
            Self::AssertionNotObserving => "assertion_not_observing",
            Self::AssertionNotCredited => "assertion_not_credited",
            Self::AssertionInert => "assertion_inert",
            Self::AssertionAdmissionUnproven => "assertion_admission_unproven",
            Self::WeakAssertion => "weak_assertion",
            Self::MissingInput => "missing_input",
            Self::MissingExactAssertion => "missing_exact_assertion",
            Self::ObservationUnconfirmed => "observation_unconfirmed",
        }
    }

    /// Whether this miss means ripr declined to credit an assertion (#5446).
    pub fn is_assertion_refusal_miss(self) -> bool {
        matches!(
            self,
            Self::AssertionNotCredited | Self::AssertionInert | Self::AssertionAdmissionUnproven
        )
    }
}

/// ripr's static verdict on one [`Probe`]: whether the related tests appear
/// to contain a discriminator that would notice the changed behavior, with
/// the evidence and gaps behind that verdict.
///
/// The class is static exposure evidence. It is not a runtime mutation
/// outcome and does not establish that the tests are sufficient.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Finding identifier. The classifiers set it to the probe id.
    pub id: String,
    /// Identity parts of the canonical gap this finding belongs to, when a
    /// producer assigned one. `None` means no canonical gap identity was
    /// assigned.
    pub canonical_gap: Option<FindingCanonicalGap>,
    /// The changed behavior this finding is about.
    pub probe: Probe,
    /// The exposure classification, such as `exposed` or
    /// `reachable_unrevealed`.
    pub class: ExposureClass,
    /// Per-stage evidence for reach, infect, propagate and reveal.
    pub ripr: RiprEvidence,
    /// Headline confidence in the classification, between 0 and 1. For Rust
    /// it is derived from the stage evidence and capped by the weakest stage;
    /// preview adapters use a per-class constant that is not on the same
    /// scale (see `confidence_basis`).
    pub confidence: f32,
    /// Why `confidence` is held at its value, for preview-adapter findings
    /// (#10700). Empty for Rust, whose score is not a per-class constant.
    /// Explains the number only; no decision helper reads it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub confidence_basis: Vec<crate::domain::ConfidenceCap>,
    /// Human-readable lines naming the evidence the analysis found.
    pub evidence: Vec<String>,
    /// Human-readable lines naming evidence the analysis looked for and did
    /// not find.
    pub missing: Vec<String>,
    /// Where the changed value appears to flow, such as a return value.
    pub flow_sinks: Vec<FlowSinkFact>,
    /// Values related tests were seen to use, and discriminating values no
    /// related test was seen to use.
    pub activation: ActivationEvidence,
    /// Why the static analysis stopped short of a firmer verdict. Empty when
    /// nothing stopped it.
    pub stop_reasons: Vec<StopReason>,
    /// Number of matched related-test/oracle rows after the existing dedup,
    /// before bounded unique-first packing. Metadata only: the retained
    /// related_tests vector remains the semantic/selection input.
    /// Older producers and artifacts omit this field and retain their known
    /// vector count; absence cannot establish how many rows were discarded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_tests_matched_total: Option<usize>,
    /// Tests ripr related to this change, with the oracle each one uses.
    /// Bounded: see `related_tests_matched_total` for the count before
    /// packing.
    pub related_tests: Vec<RelatedTest>,
    /// One suggested next action for a person or agent, when the producer
    /// named one.
    pub recommended_next_step: Option<String>,
    /// Source language the adapter that produced this finding identifies as.
    /// Additive optional per RIPR-SPEC-0026; populated by the per-language
    /// adapter (e.g. `RustAdapter` sets `Some(LanguageId::Rust)`).
    pub language: Option<crate::domain::LanguageId>,
    /// Adapter status for the source language. Omitted for `Rust` per the
    /// spec; preview adapters (TypeScript, Python) will set
    /// `Some(LanguageStatus::Preview)` when they land.
    pub language_status: Option<crate::domain::LanguageStatus>,
    /// Syntactic owner kind identified by preview language adapters.
    /// Additive optional per RIPR-SPEC-0026; omitted when the adapter does
    /// not yet have a bounded owner kind for the changed line.
    pub owner_kind: Option<crate::domain::OwnerKind>,
    /// Structured static limitation kind for preview evidence when the
    /// adapter can name one. Omitted when no static limit is known.
    pub static_limit_kind: Option<crate::domain::StaticLimitKind>,
    /// Significant tokens of the changed line — the *changed sink* the change
    /// touches. Additive optional per RIPR-SPEC-0028; populated by the Python
    /// preview adapter so a consumer can see what an oracle would need to
    /// observe. Omitted by adapters that do not compute it.
    pub changed_sink: Option<String>,
    /// Assertion text of the strongest related-test oracle the adapter
    /// inspected when deciding sink alignment. Additive optional per
    /// RIPR-SPEC-0028; omitted when no strong oracle was observed.
    pub observed_sink: Option<String>,
    /// Which token category the strongest oracle matched, surfacing *why* a
    /// strong oracle did or did not credit `exposed`. One of
    /// [`ORACLE_ALIGNMENT_VALUES`]. Additive optional per RIPR-SPEC-0028;
    /// omitted by adapters that do not compute sink alignment.
    pub oracle_alignment: Option<String>,
    /// Stable snake_case reason token explaining the `oracle_alignment` value.
    /// Additive optional per RIPR-SPEC-0028.
    pub alignment_reason: Option<String>,
    /// Producer-owned resolution of this finding's source against the
    /// candidate (head-side) revision (#3212 / #3280). Set by the producer
    /// that observed the diff evidence; `UnresolvedSubject` is the explicit
    /// unknown for surfaces that do not resolve it and the
    /// backward-compatibility default for artifacts written before the
    /// field existed. A `BaseDeleted` finding is base-side evidence; a
    /// `MovedOrRenamed` finding carries unresolved movement evidence.
    /// Neither is a candidate edit target.
    #[serde(default)]
    pub source_currentness: SourceCurrentness,
    /// Typed analysis scope of the run that produced this finding (#5531).
    /// Required in every construction (no `Default` is applied by struct
    /// literals), so a producer must state its scope; repo-mode producers
    /// set it through [`Finding::mark_repo_scoped`]. Consumers read it via
    /// `Finding::analysis_mode`, never from the free-text evidence line.
    /// Omitted from serialized `Finding` when `diff`; a missing field in an
    /// older artifact deserializes as `diff`, which `analysis_mode` still
    /// refuses if the legacy evidence line says otherwise.
    #[serde(default, skip_serializing_if = "AnalysisScope::is_diff")]
    pub analysis_scope: AnalysisScope,
}

/// Analysis scope a [`Finding`] was produced under (#5531). Repo-mode
/// capability cells are looked up from this, never defaulted from a neighbor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisScope {
    /// Produced from a diff (the only scope Rust and the diff adapters use).
    #[default]
    Diff,
    /// Produced by a repo-mode run (#9896 TypeScript/JavaScript, #9923 Python).
    Repo,
}

impl AnalysisScope {
    fn is_diff(&self) -> bool {
        matches!(self, Self::Diff)
    }
}

/// Typed source-currentness disposition for a [`Finding`] (#3212 / #3280).
///
/// The vocabulary is deliberately conservative: the producer states which
/// revision owns the actionable source, or states that it could not tell.
/// It never claims a deleted-side record is current.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCurrentness {
    /// The finding's source expression is present in the candidate source
    /// at the recorded location; the location is a candidate edit target.
    CandidateCurrent,
    /// The expression was removed on the candidate side. The retained
    /// evidence is base-side and is not a candidate edit target. In the C1
    /// producer slice, the recorded probe coordinate remains the projected
    /// new-side coordinate; consumer re-coordination is owned by #3281.
    BaseDeleted,
    /// Movement evidence exists (the same expression re-appears elsewhere
    /// in the candidate file), but the producer cannot prove the candidate
    /// identity of the exact source. Not a candidate edit target.
    MovedOrRenamed,
    /// The producing surface does not resolve source currentness; the
    /// disposition is explicitly unknown, never fabricated. Also the
    /// deserialize default for pre-#3280 artifacts.
    #[default]
    UnresolvedSubject,
}

impl SourceCurrentness {
    /// Stable wire label (matches the serde form).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CandidateCurrent => "candidate_current",
            Self::BaseDeleted => "base_deleted",
            Self::MovedOrRenamed => "moved_or_renamed",
            Self::UnresolvedSubject => "unresolved_subject",
        }
    }

    /// Delta-only resolution for producers that seed probes from head-side
    /// lines and never produce removed-only probes (#3281). A probe whose
    /// expression is candidate-side code (`after` present) is
    /// `CandidateCurrent`; a producer with no delta evidence at all stays
    /// the explicit unknown. Removed-only
    /// probes need the diff-level resolver in
    /// `analysis::probes` (movement evidence), not this helper.
    pub fn from_probe_delta(before: Option<&str>, after: Option<&str>) -> Self {
        match (before, after) {
            (_, Some(_)) => Self::CandidateCurrent,
            (None, None) => Self::UnresolvedSubject,
            // A before-only probe from a delta-only producer carries no
            // movement evidence; the diff-level resolver owns that case.
            (Some(_), None) => Self::UnresolvedSubject,
        }
    }

    /// Whether this disposition may drive current candidate-side
    /// obligations (repair routes, gate candidates, diagnostics, agent
    /// actions). Only established candidate currentness qualifies: base-side
    /// evidence and the explicit unknown are never silently upgraded
    /// because classification or severity suggests action (#3281).
    pub fn permits_candidate_action(&self) -> bool {
        matches!(self, Self::CandidateCurrent)
    }

    /// Whether the producer established that the source is base-side: the
    /// expression was deleted or moved on the candidate side. Narrower than
    /// `!permits_candidate_action()`, which also covers the explicit unknown
    /// of legacy and packet-driven findings.
    pub fn is_base_side(&self) -> bool {
        matches!(self, Self::BaseDeleted | Self::MovedOrRenamed)
    }
}

/// Controlled enum values for [`Finding::source_currentness`]. Registered in
/// `policy/output_contracts.txt` and documented in `docs/OUTPUT_SCHEMA.md`.
pub const SOURCE_CURRENTNESS_VALUES: [&str; 4] = [
    "candidate_current",
    "base_deleted",
    "moved_or_renamed",
    "unresolved_subject",
];

/// This producer reason requires a fresh physical source observation and cannot
/// be safely reused from a check artifact's diff/config-only identity.
pub const LIVE_PYTHON_TRUTHINESS_ALIGNMENT: &str = "strong_oracle_observes_dunder_truthiness";

/// Controlled enum values for [`Finding::oracle_alignment`]. Registered in
/// `policy/output_contracts.txt` and documented in `docs/OUTPUT_SCHEMA.md`.
pub const ORACLE_ALIGNMENT_VALUES: [&str; 5] = [
    "direct",
    "alias",
    "changed_sink_token",
    "orthogonal",
    "unknown",
];

/// The one gate for "a related test already reaches the boundary input and
/// only its oracle is weak" (#10622), shared by the Next-step producer and the
/// check advisory. It holds only when the finding's own evidence is a genuinely
/// weak credited oracle on the boundary test: infection detected the boundary
/// input, the discriminator stage is weak because the strongest oracle found is
/// weak (not a pairing, refusal or unverified-observation limitation), no
/// related row credits a strong or medium oracle (an exact assertion that is
/// not paired with the boundary call is a different gap), and no stop reason
/// says the call may not run.
pub fn boundary_input_observed_with_weak_oracle(
    class: &ExposureClass,
    family: &ProbeFamily,
    infect: &crate::domain::StageEvidence,
    discriminate: &crate::domain::StageEvidence,
    related_tests: &[RelatedTest],
    stop_reasons: &[StopReason],
) -> bool {
    *class == ExposureClass::WeaklyExposed
        && *family == ProbeFamily::Predicate
        && infect.state == crate::domain::StageState::Yes
        && infect.summary == crate::domain::BOUNDARY_INPUT_OBSERVED_SUMMARY
        && discriminate.state == crate::domain::StageState::Weak
        && discriminate.summary.starts_with("Only ")
        && discriminate.summary.contains(" oracle found")
        && stop_reasons.is_empty()
        && related_tests
            .iter()
            .any(|test| test.oracle_strength == crate::domain::OracleStrength::Weak)
        && !related_tests.iter().any(|test| {
            matches!(
                test.oracle_strength,
                crate::domain::OracleStrength::Strong | crate::domain::OracleStrength::Medium
            )
        })
}

impl Finding {
    /// The (file, line, expression) a finding speaks about. Probes of
    /// different families can sit on one expression (#11554); they share this
    /// key, so counts and selection can treat them as one verdict.
    pub fn expression_key(&self) -> (&std::path::Path, usize, &str) {
        (
            self.probe.location.file.as_path(),
            self.probe.location.line,
            self.probe.expression.as_str(),
        )
    }

    /// A weak predicate finding whose related tests already supply the changed
    /// boundary input (infection `yes`): the remaining gap is the oracle, not
    /// the input (#10622). Static evidence only. The finding must also carry
    /// the boundary-observed Next step itself, so the advisory never claims
    /// more than the Next step says: an assertion-shaped owner (which gets the
    /// assertion-helper Next step) or an earlier Next-step branch never
    /// qualifies.
    pub fn boundary_input_observed_with_weak_oracle(&self) -> bool {
        boundary_input_observed_with_weak_oracle(
            &self.class,
            &self.probe.family,
            &self.ripr.infect,
            &self.ripr.reveal.discriminate,
            &self.related_tests,
            &self.stop_reasons,
        ) && self.recommended_next_step.as_deref()
            == Some(crate::domain::BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP)
    }

    /// Related tests that supplied an oracle row or a refusal, skipping tests
    /// listed only as examined misses (see [`RelatedTest::is_examined_only`]).
    /// Gates that predate #5344 asked whether this set was empty.
    pub fn oracle_related_tests(&self) -> impl Iterator<Item = &RelatedTest> {
        self.related_tests
            .iter()
            .filter(|test| test.is_contributing())
    }

    /// Public count projection without changing retained evidence or selection.
    pub fn related_tests_total(&self) -> usize {
        self.related_tests_matched_total
            .unwrap_or(self.related_tests.len())
            .max(self.related_tests.len())
    }

    pub fn unknown_has_stop_reason(&self) -> bool {
        !self.class.requires_stop_reason() || !self.stop_reasons.is_empty()
    }

    pub fn effective_stop_reasons(&self) -> Vec<StopReason> {
        if self.unknown_has_stop_reason() {
            return self.stop_reasons.clone();
        }
        StopReason::for_unknown_class(&self.class)
            .into_iter()
            .collect()
    }

    /// The single candidate-actionable eligibility authority (#3281, C2 of
    /// #3212): every count, gate candidate, repair route, diagnostic, and
    /// agent action derived from a finding must flow through this
    /// predicate. A finding qualifies only when its producer proved the
    /// source is candidate-current; `base_deleted` and `moved_or_renamed`
    /// are base-side evidence, and `unresolved_subject` (a Perl change the
    /// diff does not add a line to or whose source is not on disk, and
    /// pre-#3280 artifacts) is not established
    /// current. Classifications, severity, and repair readiness never
    /// upgrade a non-current finding.
    pub fn is_candidate_actionable(&self) -> bool {
        self.source_currentness.permits_candidate_action()
    }

    /// Whether the producer established that nothing in the workspace calls
    /// the changed owner (#14820): a candidate-current `no_static_path`
    /// finding with no static limit, reach ruled out, and the RIPR-SPEC-0219
    /// [`UNREFERENCED_IN_WORKSPACE`] marker, which the producer sets only
    /// when the owner's name occurs nowhere outside its own definition in a
    /// fully indexed workspace and the owner is not an entry point. That is
    /// a test-side gap (write a test that calls it), not an analysis limit.
    /// Any other `no_static_path` finding, including an owner some non-test
    /// code calls on a path ripr cannot trace, stays fail-closed.
    pub fn is_unreferenced_no_static_path(&self) -> bool {
        self.is_candidate_actionable()
            && self.class == ExposureClass::NoStaticPath
            && self.static_limit_kind.is_none()
            && self.ripr.reach.state == crate::domain::StageState::No
            && self.ripr.reach.summary.ends_with(UNREFERENCED_IN_WORKSPACE)
    }

    /// Whether the only thing between this finding and a test path is a proof
    /// harness ripr does not evaluate (#12921, RIPR-SPEC-0367). The single
    /// authority every count and surface reads: it separates "no guard found"
    /// from "a verifier harness exists but is unevaluated". It is a
    /// disclosure only: the class stays `no_static_path`, the harness earns no
    /// test credit, and nothing here claims the change is guarded.
    pub fn is_proof_harness_only(&self) -> bool {
        self.class == ExposureClass::NoStaticPath
            && self.static_limit_kind == Some(super::StaticLimitKind::ProofHarnessNotEvaluated)
    }

    /// The one sentence every surface uses to say a finding is not a
    /// candidate edit target (#11905). `None` for a candidate-current finding.
    /// Human, hover and any later renderer take the wording from here so a
    /// base-side finding reads the same on every surface.
    pub fn source_currentness_disclosure(&self) -> Option<String> {
        if self.is_candidate_actionable() {
            return None;
        }
        // `unresolved_subject` means the producer could not tell, so it must
        // not be called base-side evidence.
        let reason = if self.source_currentness.is_base_side() {
            "base-side evidence, not a candidate edit target"
        } else {
            "source currentness not established, not a candidate edit target"
        };
        Some(format!(
            "{} \u{2014} {reason}",
            self.source_currentness.as_str()
        ))
    }

    /// Mark this finding as produced by a repo-mode run (#5531). The single
    /// place that sets the typed scope and derives the compatibility
    /// evidence line `analysis_scope: repo` (disclosed in
    /// `docs/LANGUAGE_ADAPTER_PREVIEW.md`) from it; idempotent.
    pub fn mark_repo_scoped(&mut self) {
        use crate::domain::language_capability::REPO_SCOPE_EVIDENCE_LINE;
        self.analysis_scope = AnalysisScope::Repo;
        if !self
            .evidence
            .iter()
            .any(|line| line == REPO_SCOPE_EVIDENCE_LINE)
        {
            self.evidence.push(REPO_SCOPE_EVIDENCE_LINE.to_string());
        }
    }

    /// Capability-table mode for this finding, fail closed (#5531): `None`
    /// when the typed scope says `diff` but the legacy evidence line says
    /// repo (a hand-stamped or pre-typed artifact finding), because the
    /// scope is then contradictory and no capability cell may be borrowed.
    pub fn analysis_mode(&self) -> Option<crate::domain::language_capability::AnalysisMode> {
        use crate::domain::language_capability::{AnalysisMode, finding_is_repo_scoped};
        match self.analysis_scope {
            AnalysisScope::Repo => Some(AnalysisMode::Repo),
            AnalysisScope::Diff if finding_is_repo_scoped(&self.evidence) => None,
            AnalysisScope::Diff => Some(AnalysisMode::Diff),
        }
    }
}

#[cfg(kani)]
#[path = "probe_kani.rs"]
mod kani_proofs;

#[cfg(test)]
mod tests {
    use super::{Probe, ProbeFamily};
    use crate::domain::{DeltaKind, ProbeId, SourceLocation};
    use std::path::PathBuf;

    fn probe_over(family: ProbeFamily, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:src_lib_rs:2:static_unknown".to_string()),
            location: SourceLocation {
                file: PathBuf::from("src/lib.rs"),
                line: 2,
                column: 1,
            },
            owner: None,
            family,
            delta: DeltaKind::Unknown,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    #[test]
    fn changed_const_name_reads_only_const_and_static_items() {
        let name = |expression: &str| {
            probe_over(ProbeFamily::StaticUnknown, expression)
                .changed_const_name()
                .map(str::to_string)
        };
        for (expression, expected) in [
            ("const CAP: u64 = 5_000;", "CAP"),
            ("pub const CAP: u64 = 5;", "CAP"),
            ("pub(crate) const CAP_2: u64 = 5;", "CAP_2"),
            ("static LIMIT: u32 = 3;", "LIMIT"),
            ("pub static mut LIMIT: u32 = 3;", "LIMIT"),
        ] {
            assert_eq!(name(expression).as_deref(), Some(expected), "{expression}");
        }
        for expression in [
            "const fn cap() -> u64 { 5 }",
            "let const_limit = 3;",
            "const _: () = ();",
            "pub const",
            "x = 1;",
            "const N: usize,",
        ] {
            assert_eq!(name(expression), None, "{expression}");
        }
        assert_eq!(
            probe_over(ProbeFamily::Predicate, "const CAP: u64 = 5;").changed_const_name(),
            None,
            "only a static_unknown probe names a changed constant"
        );
    }

    #[test]
    fn a_changed_constant_offers_a_stub_route_but_other_static_unknown_probes_do_not() {
        assert!(probe_over(ProbeFamily::StaticUnknown, "const CAP: u64 = 5;").offers_stub_route());
        assert!(!probe_over(ProbeFamily::StaticUnknown, "mystery!();").offers_stub_route());
        assert!(probe_over(ProbeFamily::Predicate, "a > b").offers_stub_route());
    }

    /// #4323: every stop-reason gloss is one clean clause (no whitespace runs
    /// from broken literal continuations) and differs from its token.
    #[test]
    fn stop_reason_glosses_are_clean_prose() {
        for reason in [
            StopReason::MaxDepthReached,
            StopReason::ExternalCrateBoundary,
            StopReason::DynamicDispatchUnresolved,
            StopReason::ProcMacroOpaque,
            StopReason::FixtureOpaque,
            StopReason::FeatureUnknown,
            StopReason::AsyncBoundaryOpaque,
            StopReason::NoChangedRustLine,
            StopReason::InfectionEvidenceUnknown,
            StopReason::PropagationEvidenceUnknown,
            StopReason::StaticProbeUnknown,
            StopReason::TransitiveReachUnresolved,
            StopReason::MacroReachUnresolved,
            StopReason::GapEvidenceUnresolved,
            StopReason::HelperIdentityUnresolved,
            StopReason::CompileTimeInvariant,
            StopReason::UnsafeObligation,
        ] {
            let gloss = reason.describe();
            assert!(!gloss.contains("  "), "{}: {gloss}", reason.as_str());
            assert!(!gloss.contains('_'), "{}: {gloss}", reason.as_str());
            assert!(!gloss.ends_with('.'), "{}: {gloss}", reason.as_str());
        }
    }
    use super::{FlowSinkKind, StopReason, ValueContext};
    use crate::domain::ExposureClass;

    #[test]
    fn flow_sink_kind_labels_are_stable_contract_terms() {
        let cases = [
            (FlowSinkKind::ReturnValue, "return_value", "returned value"),
            (FlowSinkKind::ErrorVariant, "error_variant", "error variant"),
            (
                FlowSinkKind::StructField,
                "struct_field",
                "constructed field",
            ),
            (
                FlowSinkKind::EventCall,
                "event_call",
                "event or outbound call",
            ),
            (FlowSinkKind::StateWrite, "state_write", "state write"),
            (
                FlowSinkKind::Persistence,
                "persistence",
                "persistence write",
            ),
            (FlowSinkKind::LogMessage, "log_message", "log message"),
            (
                FlowSinkKind::ConfigChange,
                "config_change",
                "configuration change",
            ),
            (FlowSinkKind::CallEffect, "call_effect", "call effect"),
            (FlowSinkKind::MatchArm, "match_arm", "match arm result"),
            (FlowSinkKind::Unknown, "unknown", "unknown sink"),
        ];

        for (kind, value, label) in cases {
            assert_eq!(kind.as_str(), value);
            assert_eq!(kind.label(), label);
        }
    }

    #[test]
    fn stop_reason_for_unknown_class_matches_contract() {
        assert_eq!(
            StopReason::for_unknown_class(&ExposureClass::PropagationUnknown)
                .map(|reason| reason.as_str()),
            Some("propagation_evidence_unknown")
        );
        assert_eq!(StopReason::for_unknown_class(&ExposureClass::Exposed), None);
    }

    #[test]
    fn value_context_labels_are_stable_contract_terms() {
        let cases = [
            (ValueContext::FunctionArgument, "function_argument"),
            (ValueContext::AssertionArgument, "assertion_argument"),
            (ValueContext::BuilderMethod, "builder_method"),
            (ValueContext::TableRow, "table_row"),
            (ValueContext::EnumVariant, "enum_variant"),
            (ValueContext::Constant, "constant"),
            (ValueContext::ReturnValue, "return_value"),
            (ValueContext::Unknown, "unknown"),
        ];

        for (context, value) in cases {
            assert_eq!(context.as_str(), value);
        }
    }
}

#[cfg(test)]
mod source_currentness_tests {
    use super::{SOURCE_CURRENTNESS_VALUES, SourceCurrentness};

    #[test]
    fn source_currentness_labels_are_the_controlled_vocabulary() {
        // The wire form is pinned where JSON is allowed to be known
        // (`app::check_artifact` tests); here the stable labels themselves
        // are the contract.
        for (value, label) in [
            (SourceCurrentness::CandidateCurrent, "candidate_current"),
            (SourceCurrentness::BaseDeleted, "base_deleted"),
            (SourceCurrentness::MovedOrRenamed, "moved_or_renamed"),
            (SourceCurrentness::UnresolvedSubject, "unresolved_subject"),
        ] {
            assert_eq!(value.as_str(), label);
            assert!(SOURCE_CURRENTNESS_VALUES.contains(&label));
        }
    }
}

#[cfg(test)]
mod candidate_actionable_predicate_tests {
    use super::SourceCurrentness;

    #[test]
    fn only_proven_candidate_currentness_permits_action() {
        // RIPR-SPEC-0152: the single eligibility authority. Class and
        // severity never upgrade a non-current finding.
        for (value, eligible) in [
            (SourceCurrentness::CandidateCurrent, true),
            (SourceCurrentness::BaseDeleted, false),
            (SourceCurrentness::MovedOrRenamed, false),
            (SourceCurrentness::UnresolvedSubject, false),
        ] {
            assert_eq!(value.permits_candidate_action(), eligible, "{value:?}");
        }
    }

    #[test]
    fn delta_rule_resolves_head_side_probes_only() {
        use SourceCurrentness::{BaseDeleted, CandidateCurrent, UnresolvedSubject};
        assert_eq!(
            SourceCurrentness::from_probe_delta(None, Some("head text")),
            CandidateCurrent
        );
        assert_eq!(
            SourceCurrentness::from_probe_delta(Some("base"), Some("head")),
            CandidateCurrent
        );
        assert_eq!(
            SourceCurrentness::from_probe_delta(None, None),
            UnresolvedSubject
        );
        // The delta-only helper never claims base_deleted: removed-only
        // probes need the diff-level resolver with movement evidence.
        assert_eq!(
            SourceCurrentness::from_probe_delta(Some("base"), None),
            UnresolvedSubject
        );
        assert_ne!(
            SourceCurrentness::from_probe_delta(Some("base"), None),
            BaseDeleted
        );
    }
}

#[cfg(test)]
mod decision_helper_tests {
    use super::{
        ARM_UNSELECTED_REASON_PREFIX, ActivationEvidence, AnalysisScope, DeltaKind,
        DiscriminatorShape, Finding, GUARD_UNENTERED_REASON_PREFIX, MissingDiscriminatorFact,
        Probe, ProbeFamily, RelatedTest, RelatedTestMiss, RelatedTestRole, SourceCurrentness,
        StopReason, boundary_input_observed_with_weak_oracle, exact_assertion_fact,
        input_boundary_fact, is_unselected_input_reason, row_exact_discriminator_fact,
    };
    use crate::domain::language_capability::{AnalysisMode, REPO_SCOPE_EVIDENCE_LINE};
    use crate::domain::{
        BOUNDARY_INPUT_OBSERVED_SUMMARY, BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP, Confidence,
        ExposureClass, OracleKind, OracleStrength, ProbeId, RevealEvidence, RiprEvidence,
        SourceLocation, StageEvidence, StageState, StaticLimitKind,
    };
    use std::path::PathBuf;

    const ALL_FAMILIES: [(ProbeFamily, &str); 8] = [
        (ProbeFamily::Predicate, "predicate"),
        (ProbeFamily::ReturnValue, "return_value"),
        (ProbeFamily::ErrorPath, "error_path"),
        (ProbeFamily::CallDeletion, "call_deletion"),
        (ProbeFamily::FieldConstruction, "field_construction"),
        (ProbeFamily::SideEffect, "side_effect"),
        (ProbeFamily::MatchArm, "match_arm"),
        (ProbeFamily::StaticUnknown, "static_unknown"),
    ];

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    fn fact(value: &str, reason: &str, shape: DiscriminatorShape) -> MissingDiscriminatorFact {
        MissingDiscriminatorFact {
            value: value.to_string(),
            reason: reason.to_string(),
            flow_sink: None,
            drafted_boundary: None,
            shape,
        }
    }

    fn related(name: &str, strength: OracleStrength) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/limits.rs"),
            line: 7,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            role: RelatedTestRole::Contributing,
            miss_support: None,
        }
    }

    /// A weak predicate finding whose one related test reaches the boundary
    /// input and carries the boundary-observed Next step: every condition of
    /// the weak-oracle advisory holds.
    fn boundary_weak_finding() -> Finding {
        Finding {
            analysis_scope: AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "finding:limit".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:limit".to_string()),
                location: SourceLocation::new("src/lib.rs", 4, 5),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "n >= limit".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "Related test at_limit calls over_limit"),
                infect: stage(StageState::Yes, BOUNDARY_INPUT_OBSERVED_SUMMARY),
                propagate: stage(StageState::Yes, "value is returned"),
                reveal: RevealEvidence {
                    observe: stage(StageState::Yes, "assertion observes the result"),
                    discriminate: stage(StageState::Weak, "Only weak oracle found"),
                },
            },
            confidence: 0.5,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: vec![related("at_limit", OracleStrength::Weak)],
            recommended_next_step: Some(BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP.to_string()),
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    fn edited(edit: impl FnOnce(&mut Finding)) -> Finding {
        let mut finding = boundary_weak_finding();
        edit(&mut finding);
        finding
    }

    #[test]
    fn every_probe_family_token_round_trips_and_unknown_tokens_are_refused() {
        for (family, token) in ALL_FAMILIES {
            assert_eq!(family.as_str(), token);
            assert_eq!(ProbeFamily::from_wire(token), Some(family));
        }
        for token in ["", "Predicate", "predicate ", "return-value"] {
            assert_eq!(ProbeFamily::from_wire(token), None, "{token:?}");
        }
    }

    #[test]
    fn delta_role_and_miss_tokens_are_the_wire_vocabulary() {
        for (delta, token) in [
            (DeltaKind::Value, "value"),
            (DeltaKind::Control, "control"),
            (DeltaKind::Effect, "effect"),
            (DeltaKind::Unknown, "unknown"),
        ] {
            assert_eq!(delta.as_str(), token);
        }
        assert_eq!(RelatedTestRole::Contributing.as_str(), "contributing");
        assert_eq!(RelatedTestRole::ExaminedOnly.as_str(), "examined_only");
        for (miss, token, refusal) in [
            (RelatedTestMiss::NoCallPath, "no_call_path", false),
            (RelatedTestMiss::NoAssertion, "no_assertion", false),
            (
                RelatedTestMiss::AssertionNotObserving,
                "assertion_not_observing",
                false,
            ),
            (
                RelatedTestMiss::AssertionNotCredited,
                "assertion_not_credited",
                true,
            ),
            (RelatedTestMiss::AssertionInert, "assertion_inert", true),
            (
                RelatedTestMiss::AssertionAdmissionUnproven,
                "assertion_admission_unproven",
                true,
            ),
            (RelatedTestMiss::WeakAssertion, "weak_assertion", false),
            (RelatedTestMiss::MissingInput, "missing_input", false),
            (
                RelatedTestMiss::MissingExactAssertion,
                "missing_exact_assertion",
                false,
            ),
            (
                RelatedTestMiss::ObservationUnconfirmed,
                "observation_unconfirmed",
                false,
            ),
        ] {
            assert_eq!(miss.as_str(), token);
            assert_eq!(miss.is_assertion_refusal_miss(), refusal, "{token}");
        }
    }

    #[test]
    fn stop_reason_glosses_say_something_distinct_for_every_reason() {
        let reasons = [
            StopReason::MaxDepthReached,
            StopReason::ExternalCrateBoundary,
            StopReason::DynamicDispatchUnresolved,
            StopReason::ProcMacroOpaque,
            StopReason::FixtureOpaque,
            StopReason::FeatureUnknown,
            StopReason::AsyncBoundaryOpaque,
            StopReason::NoChangedRustLine,
            StopReason::InfectionEvidenceUnknown,
            StopReason::PropagationEvidenceUnknown,
            StopReason::StaticProbeUnknown,
            StopReason::TransitiveReachUnresolved,
            StopReason::MacroReachUnresolved,
            StopReason::GapEvidenceUnresolved,
            StopReason::HelperIdentityUnresolved,
            StopReason::CompileTimeInvariant,
            StopReason::UnsafeObligation,
        ];
        let glosses: std::collections::BTreeSet<&str> =
            reasons.iter().map(StopReason::describe).collect();
        assert_eq!(glosses.len(), reasons.len());
        for gloss in glosses {
            assert!(gloss.split(' ').count() >= 6, "{gloss}");
        }
        assert_eq!(
            StopReason::ExternalCrateBoundary.describe(),
            "the path crosses into another crate, which ripr does not analyze"
        );
    }

    #[test]
    fn each_unknown_class_has_its_own_fallback_stop_reason() {
        for (class, reason) in [
            (
                ExposureClass::InfectionUnknown,
                Some(StopReason::InfectionEvidenceUnknown),
            ),
            (
                ExposureClass::PropagationUnknown,
                Some(StopReason::PropagationEvidenceUnknown),
            ),
            (
                ExposureClass::StaticUnknown,
                Some(StopReason::StaticProbeUnknown),
            ),
            (ExposureClass::WeaklyExposed, None),
            (ExposureClass::NoStaticPath, None),
        ] {
            assert_eq!(StopReason::for_unknown_class(&class), reason, "{class:?}");
        }
    }

    #[test]
    fn only_arm_and_guard_reasons_name_an_unselected_input() {
        assert!(is_unselected_input_reason(&format!(
            "{ARM_UNSELECTED_REASON_PREFIX} `Mode::Fast`"
        )));
        assert!(is_unselected_input_reason(&format!(
            "{GUARD_UNENTERED_REASON_PREFIX} `n > 10`"
        )));
        assert!(!is_unselected_input_reason(
            "No related test asserts the variant"
        ));
        assert!(!is_unselected_input_reason(&format!(
            "note: {ARM_UNSELECTED_REASON_PREFIX}"
        )));
    }

    #[test]
    fn the_input_boundary_fact_is_chosen_by_family() {
        let arm = format!("{ARM_UNSELECTED_REASON_PREFIX} `Mode::Fast`");
        let guard_reason = format!("{GUARD_UNENTERED_REASON_PREFIX} `n > 10`");
        let facts = [
            fact("exact", "No test pins the value", DiscriminatorShape::Other),
            fact("domain", "guard domain", DiscriminatorShape::GuardDomain),
            fact("n == 10", "boundary", DiscriminatorShape::Boundary),
            fact("arm", &arm, DiscriminatorShape::Other),
            fact("guard", &guard_reason, DiscriminatorShape::Other),
        ];
        let chosen = |family: ProbeFamily| {
            input_boundary_fact(&facts, &family).map(|fact| fact.value.as_str())
        };
        // The equality boundary wins over the guard domain even when listed later.
        assert_eq!(chosen(ProbeFamily::Predicate), Some("n == 10"));
        assert_eq!(chosen(ProbeFamily::ReturnValue), Some("n == 10"));
        assert_eq!(chosen(ProbeFamily::MatchArm), Some("arm"));
        assert_eq!(chosen(ProbeFamily::SideEffect), Some("guard"));
        assert_eq!(chosen(ProbeFamily::CallDeletion), Some("guard"));
        assert_eq!(chosen(ProbeFamily::ErrorPath), Some("guard"));
        assert_eq!(chosen(ProbeFamily::FieldConstruction), None);
        assert_eq!(chosen(ProbeFamily::StaticUnknown), None);

        // Without an equality boundary, predicate and return-value read the guard domain.
        let without_boundary = &facts[..2];
        for family in [ProbeFamily::Predicate, ProbeFamily::ReturnValue] {
            assert_eq!(
                input_boundary_fact(without_boundary, &family).map(|fact| fact.value.as_str()),
                Some("domain"),
                "{family:?}"
            );
        }
        // A reason prefix alone is not a boundary for a predicate.
        assert_eq!(
            input_boundary_fact(&facts[3..], &ProbeFamily::Predicate),
            None
        );
        assert_eq!(
            input_boundary_fact(&facts[..1], &ProbeFamily::MatchArm),
            None
        );
    }

    #[test]
    fn exact_facts_skip_the_input_boundary_and_row_facts_also_skip_unselected_inputs() {
        let guard_reason = format!("{GUARD_UNENTERED_REASON_PREFIX} `n > 10`");
        let facts = [
            fact("n == 10", "boundary", DiscriminatorShape::Boundary),
            fact("guard", &guard_reason, DiscriminatorShape::Other),
            fact(
                "Error::Full",
                "No test pins the variant",
                DiscriminatorShape::Other,
            ),
        ];
        let value = |fact: Option<&MissingDiscriminatorFact>| fact.map(|fact| fact.value.clone());

        // Predicate: the boundary is the input fact, so the exact assertion is
        // the next fact, and the row fact also steps over the unentered guard.
        assert_eq!(
            value(exact_assertion_fact(&facts, &ProbeFamily::Predicate)).as_deref(),
            Some("guard")
        );
        assert_eq!(
            value(row_exact_discriminator_fact(
                &facts,
                &ProbeFamily::Predicate
            ))
            .as_deref(),
            Some("Error::Full")
        );
        // Field construction has no input boundary, so the first fact is exact.
        assert_eq!(
            value(exact_assertion_fact(
                &facts,
                &ProbeFamily::FieldConstruction
            ))
            .as_deref(),
            Some("n == 10")
        );
        assert_eq!(
            value(row_exact_discriminator_fact(
                &facts,
                &ProbeFamily::FieldConstruction
            ))
            .as_deref(),
            Some("n == 10")
        );
        // Only input facts: nothing exact remains.
        assert_eq!(
            exact_assertion_fact(&facts[..1], &ProbeFamily::Predicate),
            None
        );
        assert_eq!(
            row_exact_discriminator_fact(&facts[..2], &ProbeFamily::Predicate),
            None
        );
        // Identity, not equality, excludes the boundary: an equal twin is still exact.
        let twins = [facts[0].clone(), facts[0].clone()];
        let second = exact_assertion_fact(&twins, &ProbeFamily::Predicate);
        assert!(second.is_some_and(|fact| std::ptr::eq(fact, &twins[1])));
    }

    #[test]
    fn related_test_roles_split_decision_rows_from_examined_only_rows() {
        let contributing = related("at_limit", OracleStrength::Weak);
        let mut examined = related("elsewhere", OracleStrength::None);
        examined.role = RelatedTestRole::ExaminedOnly;
        // The role decides, never the miss.
        examined.miss = None;
        let mut contributing_with_miss = contributing.clone();
        contributing_with_miss.miss = Some(RelatedTestMiss::NoAssertion);

        assert!(RelatedTestRole::Contributing.is_contributing());
        assert!(!RelatedTestRole::ExaminedOnly.is_contributing());
        assert!(contributing.is_contributing() && !contributing.is_examined_only());
        assert!(!examined.is_contributing() && examined.is_examined_only());
        assert!(contributing_with_miss.is_contributing());

        let finding = edited(|finding| {
            finding.related_tests = vec![examined.clone(), contributing.clone()];
        });
        let names: Vec<&str> = finding
            .oracle_related_tests()
            .map(|test| test.name.as_str())
            .collect();
        assert_eq!(names, ["at_limit"]);
    }

    #[test]
    fn the_related_test_total_never_reads_below_the_retained_rows() {
        let with_total = |total: Option<usize>, rows: usize| {
            edited(|finding| {
                finding.related_tests_matched_total = total;
                finding.related_tests = (0..rows)
                    .map(|index| related(&format!("t{index}"), OracleStrength::Weak))
                    .collect();
            })
            .related_tests_total()
        };
        assert_eq!(with_total(None, 0), 0);
        assert_eq!(with_total(None, 3), 3);
        assert_eq!(with_total(Some(9), 3), 9);
        assert_eq!(with_total(Some(2), 3), 3);
    }

    #[test]
    fn an_unknown_class_without_a_stop_reason_reports_its_fallback() {
        let weak = boundary_weak_finding();
        assert!(weak.unknown_has_stop_reason());
        assert_eq!(weak.effective_stop_reasons(), Vec::new());

        let bare_unknown = edited(|finding| finding.class = ExposureClass::StaticUnknown);
        assert!(!bare_unknown.unknown_has_stop_reason());
        assert_eq!(
            bare_unknown.effective_stop_reasons(),
            [StopReason::StaticProbeUnknown]
        );

        let explained = edited(|finding| {
            finding.class = ExposureClass::StaticUnknown;
            finding.stop_reasons = vec![StopReason::ProcMacroOpaque, StopReason::FeatureUnknown];
        });
        assert!(explained.unknown_has_stop_reason());
        assert_eq!(
            explained.effective_stop_reasons(),
            [StopReason::ProcMacroOpaque, StopReason::FeatureUnknown]
        );
    }

    #[test]
    fn only_a_candidate_current_finding_is_actionable_and_others_disclose_why() {
        let with = |currentness: SourceCurrentness| {
            edited(|finding| finding.source_currentness = currentness)
        };
        let current = with(SourceCurrentness::CandidateCurrent);
        assert!(current.is_candidate_actionable());
        assert_eq!(current.source_currentness_disclosure(), None);

        for (currentness, base_side, disclosure) in [
            (
                SourceCurrentness::BaseDeleted,
                true,
                "base_deleted \u{2014} base-side evidence, not a candidate edit target",
            ),
            (
                SourceCurrentness::MovedOrRenamed,
                true,
                "moved_or_renamed \u{2014} base-side evidence, not a candidate edit target",
            ),
            (
                SourceCurrentness::UnresolvedSubject,
                false,
                "unresolved_subject \u{2014} source currentness not established, not a candidate edit target",
            ),
        ] {
            let finding = with(currentness);
            assert!(!finding.is_candidate_actionable(), "{currentness:?}");
            assert_eq!(currentness.is_base_side(), base_side, "{currentness:?}");
            assert_eq!(
                finding.source_currentness_disclosure().as_deref(),
                Some(disclosure)
            );
        }
        assert!(!SourceCurrentness::CandidateCurrent.is_base_side());
    }

    #[test]
    fn proof_harness_only_needs_both_the_class_and_the_limit_kind() {
        let harness = |class: ExposureClass, kind: Option<StaticLimitKind>| {
            edited(|finding| {
                finding.class = class;
                finding.static_limit_kind = kind;
            })
            .is_proof_harness_only()
        };
        let proof = Some(StaticLimitKind::ProofHarnessNotEvaluated);
        assert!(harness(ExposureClass::NoStaticPath, proof));
        assert!(!harness(ExposureClass::StaticUnknown, proof));
        assert!(!harness(ExposureClass::NoStaticPath, None));
        assert!(!harness(
            ExposureClass::NoStaticPath,
            Some(StaticLimitKind::DynamicDispatch)
        ));
    }

    #[test]
    fn marking_repo_scope_sets_the_scope_and_one_evidence_line() {
        let mut finding = boundary_weak_finding();
        assert!(finding.analysis_scope.is_diff());
        assert_eq!(finding.analysis_mode(), Some(AnalysisMode::Diff));

        finding.mark_repo_scoped();
        finding.mark_repo_scoped();
        assert_eq!(finding.analysis_scope, AnalysisScope::Repo);
        assert!(!finding.analysis_scope.is_diff());
        assert_eq!(finding.evidence, [REPO_SCOPE_EVIDENCE_LINE]);
        assert_eq!(finding.analysis_mode(), Some(AnalysisMode::Repo));

        // A diff-scoped finding that carries the repo evidence line is
        // contradictory, so no capability mode is borrowed.
        let contradictory = edited(|finding| {
            finding.evidence = vec![REPO_SCOPE_EVIDENCE_LINE.to_string()];
        });
        assert_eq!(contradictory.analysis_mode(), None);
    }

    #[test]
    fn the_weak_oracle_advisory_holds_only_when_every_condition_does() {
        assert!(boundary_weak_finding().boundary_input_observed_with_weak_oracle());

        let refused: [(&str, Finding); 12] = [
            (
                "class",
                edited(|finding| finding.class = ExposureClass::ReachableUnrevealed),
            ),
            (
                "family",
                edited(|finding| finding.probe.family = ProbeFamily::ReturnValue),
            ),
            (
                "infection state",
                edited(|finding| finding.ripr.infect.state = StageState::Weak),
            ),
            (
                "infection summary",
                edited(|finding| finding.ripr.infect.summary = "input reaches the change".into()),
            ),
            (
                "discriminator state",
                edited(|finding| finding.ripr.reveal.discriminate.state = StageState::No),
            ),
            (
                "discriminator summary opening",
                edited(|finding| {
                    finding.ripr.reveal.discriminate.summary = "A weak oracle found".into();
                }),
            ),
            (
                "discriminator summary limitation",
                edited(|finding| {
                    finding.ripr.reveal.discriminate.summary = "Only unpaired assertions".into();
                }),
            ),
            (
                "stop reason",
                edited(|finding| finding.stop_reasons = vec![StopReason::FixtureOpaque]),
            ),
            (
                "no weak row",
                edited(|finding| {
                    finding.related_tests = vec![related("smoke", OracleStrength::Smoke)];
                }),
            ),
            (
                "strong row",
                edited(|finding| {
                    finding
                        .related_tests
                        .push(related("exact", OracleStrength::Strong));
                }),
            ),
            (
                "medium row",
                edited(|finding| {
                    finding
                        .related_tests
                        .push(related("medium", OracleStrength::Medium));
                }),
            ),
            (
                "next step",
                edited(|finding| finding.recommended_next_step = Some("Add a test".into())),
            ),
        ];
        for (condition, finding) in &refused {
            assert!(
                !finding.boundary_input_observed_with_weak_oracle(),
                "{condition}"
            );
        }

        // The shared gate ignores the Next step; the finding-level reading adds it.
        let other_step = &refused[11].1;
        assert!(boundary_input_observed_with_weak_oracle(
            &other_step.class,
            &other_step.probe.family,
            &other_step.ripr.infect,
            &other_step.ripr.reveal.discriminate,
            &other_step.related_tests,
            &other_step.stop_reasons,
        ));
        // A weak row beside a smoke row still qualifies.
        assert!(
            edited(|finding| {
                finding
                    .related_tests
                    .push(related("smoke", OracleStrength::Smoke));
            })
            .boundary_input_observed_with_weak_oracle()
        );
    }
}
