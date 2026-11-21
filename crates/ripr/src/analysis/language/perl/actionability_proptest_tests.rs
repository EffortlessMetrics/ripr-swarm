//! Property tests for the Perl strict actionability mapper (#13948).
//!
//! `strict_actionability_for_change` is the only Perl path that returns
//! `PerlGapState::Actionable`. The example tests in `tests.rs` knock out one
//! field at a time; these properties perturb every enumerated input of the
//! packet row the mapper selects and of the caller's context at once, starting
//! from the exporter fixture with a concrete return discriminator.
//!
//! The refusal conditions reachable from these perturbations (32, one per
//! `note` in `refusals`), taken from the mapper, the related-evidence filter
//! and the classifier it reads the selected evidence class from:
//!
//! - packet status other than `complete`;
//! - the requested change is absent or lacks a concrete discriminator;
//! - owner confidence, or any confidence folded into the evidence confidence
//!   (owner, relation, test, oracle, verify command), is `low`/`unknown`;
//! - a behavior hint without a repair kind (`call_effect`, `unknown`);
//! - relation kind other than `direct_owner_call`/`helper_call`;
//! - reachability other than `reachable`;
//! - the relation has no oracle, the oracle is not bound to the owner, the
//!   oracle belongs to another test, its strength is not `strong_exact`, or an
//!   `isnt` comparison downgrades it (the evidence is not `weakly_exposed`);
//! - `oracle_matcher_grading` declared and the oracle is an exception
//!   observer (rule 16), or the change's observable is declared absent or
//!   blank (rule 18);
//! - the relation names another owner (it is not related evidence at all);
//! - an `unknown` test framework;
//! - the oracle shape is not the behavior hint's default assertion shape;
//! - a blocking limitation (every blocking kind, against the relation, change,
//!   test or with no refs) or any dynamic boundary kind on the owner, source
//!   file or test file;
//! - an owner of kind `unknown` (no canonical gap identity);
//! - a missing, empty, unsafe or non-test verify command;
//! - a missing, empty or invalid receipt command;
//! - a test file without the `test` role;
//! - an unsafe source or test path inside the packet;
//! - empty `provenance_refs` on any fact the raw evidence refs cite;
//! - an unsafe allowed or forbidden edit path, an allowed path set other than
//!   exactly the related test file, or a forbidden set without the source;
//! - empty `stop_if`, or `must_not_change` without both required rules.
//!
//! Not covered, and why:
//!
//! - `test_file.path != evidence.test_path`: unreachable, both are read from
//!   the same file row;
//! - an empty raw-evidence-ref list after success: unreachable, eight refs are
//!   pushed or an earlier check refuses;
//! - an owner id without the `perl:` prefix: every row keyed by that id would
//!   have to be renamed together, so it is not perturbed;
//! - the related-evidence filter that drops another test's oracle is
//!   backstopped by the classifier's own `oracle.test_id == relation.test_id`
//!   binding check, so removing either one alone still refuses; the condition
//!   is pinned, the individual filter is not;
//! - several related tests and the selection among them: only the fixture's
//!   single related test is perturbed (`tests.rs` pins a limitation on a
//!   non-selected relation).
//!
//! The oracle is not fully independent: `refusals` calls the production
//! helpers `repair_kind`, `default_assertion_shape`, `assertion_shape` and
//! `is_strict_actionable`, so a wrong answer from one of those is mirrored
//! rather than caught here. The blocking limitation-kind table is a separate
//! copy, not the production predicate.
//!
//! Monotonicity holds for *supporting* evidence only, and that is what is
//! asserted: removing a supporting row, field or context entry never turns a
//! refusal into an actionable result. It is deliberately not asserted for
//! blocker facts, where the code intends the opposite: removing a blocking
//! limitation or dynamic boundary, an extra allowed edit path, or an unsafe
//! path is exactly how a refused change becomes actionable.
//!
//! The default case count is small so the required test run stays cheap. Run
//! longer locally with `PROPTEST_CASES=2000 cargo test -p ripr --features
//! lang-perl --lib actionability_proptest_tests`.

use super::tests::{RETURN_PACKET, concrete_return_packet, consume};
use super::*;
use proptest::prelude::*;
use proptest::test_runner::Config;

const DEFAULT_CASES: u32 = 64;

const CHANGE_ID: &str = "change:lib/My/App.pm:8:return";
const OWNER_ID: &str = "perl:lib/My/App.pm::My::App::discount";
const RELATION_ID: &str = "relation:return:discount-smoke";
const TEST_ID: &str = "test:t/app.t:discount_smoke";
const ORACLE_ID: &str = "oracle:t/app.t:7:is";
const VERIFY_ID: &str = "verify:t/app.t:prove";
const SOURCE_FILE_ID: &str = "file:lib/My/App.pm";
const TEST_FILE_ID: &str = "file:t/app.t";
const SOURCE_PATH: &str = "lib/My/App.pm";
const TEST_PATH: &str = "t/app.t";
const PRODUCTION_RULE: &str = "do not edit Perl production code";
const SUPPRESSION_RULE: &str = "do not add suppressions or intent ledger entries";

fn config() -> Config {
    Config {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_CASES),
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::Off)),
        ..Config::default()
    }
}

const CONFIDENCES: [Confidence; 4] = [
    Confidence::High,
    Confidence::Medium,
    Confidence::Low,
    Confidence::Unknown,
];
const BEHAVIOR_HINTS: [BehaviorHint; 9] = [
    BehaviorHint::PredicateBoundary,
    BehaviorHint::ReturnValue,
    BehaviorHint::ExceptionPath,
    BehaviorHint::HashOrObjectField,
    BehaviorHint::OutputObserver,
    BehaviorHint::WarnObserver,
    BehaviorHint::LogObserver,
    BehaviorHint::CallEffect,
    BehaviorHint::Unknown,
];
const ORACLE_KINDS: [OracleKind; 13] = [
    OracleKind::ExactReturnAssertion,
    OracleKind::PredicateBoundaryAssertion,
    OracleKind::ExceptionObserver,
    OracleKind::HashOrObjectFieldAssertion,
    OracleKind::OutputObserver,
    OracleKind::WarnObserver,
    OracleKind::LogObserver,
    OracleKind::SmokeOk,
    OracleKind::MentionOnly,
    OracleKind::DiesOnly,
    OracleKind::UnknownHelper,
    OracleKind::DynamicFrameworkIndirection,
    OracleKind::Unknown,
];
/// Behavior hints paired with the oracle kind whose shape is the hint's
/// default assertion shape: the non-refusing combinations.
const MATCHED_HINTS: [(BehaviorHint, OracleKind); 7] = [
    (
        BehaviorHint::PredicateBoundary,
        OracleKind::PredicateBoundaryAssertion,
    ),
    (BehaviorHint::ReturnValue, OracleKind::ExactReturnAssertion),
    (BehaviorHint::ExceptionPath, OracleKind::ExceptionObserver),
    (
        BehaviorHint::HashOrObjectField,
        OracleKind::HashOrObjectFieldAssertion,
    ),
    (BehaviorHint::OutputObserver, OracleKind::OutputObserver),
    (BehaviorHint::WarnObserver, OracleKind::WarnObserver),
    (BehaviorHint::LogObserver, OracleKind::LogObserver),
];
const RELATION_KINDS: [RelationKind; 8] = [
    RelationKind::DirectOwnerCall,
    RelationKind::PackageReference,
    RelationKind::MethodReceiver,
    RelationKind::TestNameMatch,
    RelationKind::FileProximity,
    RelationKind::HelperCall,
    RelationKind::FixtureSetup,
    RelationKind::Unknown,
];
const FRAMEWORKS: [TestFramework; 7] = [
    TestFramework::TestMore,
    TestFramework::Test2V0,
    TestFramework::Test2V1,
    TestFramework::Test2Suite,
    TestFramework::TestException,
    TestFramework::TestFatal,
    TestFramework::Unknown,
];
const STRENGTHS: [OracleStrength; 5] = [
    OracleStrength::StrongExact,
    OracleStrength::WeakSmoke,
    OracleStrength::WeakBroad,
    OracleStrength::MentionOnly,
    OracleStrength::Unknown,
];

/// Limitation kinds with whether they block strict actionability (the union
/// of the operational blocking list and the static-limit taxonomy), plus kinds
/// that do not block.
const LIMITATION_KINDS: [(&str, bool); 24] = [
    ("dynamic_dispatch", true),
    ("module_resolution_unknown", true),
    ("generated_symbol", true),
    ("role_composition", true),
    ("monkeypatch_or_symbol_patch", true),
    ("eval_or_string_code", true),
    ("symbol_table_mutation", true),
    ("framework_indirection", true),
    ("unknown_helper", true),
    ("unsupported_syntax", true),
    ("missing_test_runner", true),
    ("missing_diff_owner", true),
    ("narrowed_representation", true),
    ("packet_incomplete", true),
    ("partial_emitter", true),
    ("unknown", true),
    ("missing_import_graph", true),
    ("metaprogramming", true),
    ("decorator_indirection", true),
    ("mocked_module", true),
    ("opaque_custom_assertion_helper", true),
    ("parse_failure", true),
    ("advisory_note", false),
    ("", false),
];
const BOUNDARY_KINDS: [BoundaryKind; 15] = [
    BoundaryKind::DynamicDispatch,
    BoundaryKind::ModuleResolutionUnknown,
    BoundaryKind::GeneratedSymbol,
    BoundaryKind::RoleComposition,
    BoundaryKind::MonkeypatchOrSymbolPatch,
    BoundaryKind::EvalOrStringCode,
    BoundaryKind::SymbolTableMutation,
    BoundaryKind::FrameworkIndirection,
    BoundaryKind::UnknownHelper,
    BoundaryKind::UnsupportedSyntax,
    BoundaryKind::MissingTestRunner,
    BoundaryKind::MissingDiffOwner,
    BoundaryKind::PacketIncomplete,
    BoundaryKind::PartialEmitter,
    BoundaryKind::Unknown,
];
const OTHER_OWNER_ID: &str = "perl:lib/My/App.pm::My::App::eligible";
const OTHER_TEST_ID: &str = "test:t/app_test2.t:discount_profile";

/// What a pushed limitation's `evidence_refs` name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LimitTarget {
    Relation,
    Change,
    Test,
    NoRefs,
    Unrelated,
}

/// Where a pushed dynamic boundary sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundaryScope {
    Owner,
    SourceFile,
    TestFile,
    OtherOwner,
}

/// The change's `changed_observable` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObservableVariant {
    NotCarried,
    Present,
    DeclaredNull,
    Blank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PacketPath {
    Safe,
    UnsafeSource,
    UnsafeTest,
}

/// A fact whose `provenance_refs` the raw evidence refs require.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProvenanceFact {
    Change,
    SourceFile,
    Owner,
    Relation,
    Test,
    TestFile,
    Oracle,
    Verify,
}

/// Perturbations beyond the row fields, each defaulting to the fixture.
#[derive(Clone, Copy, Debug)]
struct Extra {
    limitation_kind: usize,
    limitation_target: LimitTarget,
    boundary_kind: usize,
    boundary_scope: BoundaryScope,
    matcher_grading: bool,
    observable: ObservableVariant,
    oracle_from_other_test: bool,
    negated_comparison: bool,
    owner_kind_unknown: bool,
    packet_path: PacketPath,
    empty_provenance: Option<ProvenanceFact>,
    relation_for_other_owner: bool,
    missing_discriminator: bool,
}

impl Default for Extra {
    fn default() -> Self {
        Self {
            limitation_kind: 7,
            limitation_target: LimitTarget::Relation,
            boundary_kind: 0,
            boundary_scope: BoundaryScope::Owner,
            matcher_grading: false,
            observable: ObservableVariant::NotCarried,
            oracle_from_other_test: false,
            negated_comparison: false,
            owner_kind_unknown: false,
            packet_path: PacketPath::Safe,
            empty_provenance: None,
            relation_for_other_owner: false,
            missing_discriminator: false,
        }
    }
}

const LIMIT_TARGETS: [LimitTarget; 5] = [
    LimitTarget::Relation,
    LimitTarget::Change,
    LimitTarget::Test,
    LimitTarget::NoRefs,
    LimitTarget::Unrelated,
];
const BOUNDARY_SCOPES: [BoundaryScope; 4] = [
    BoundaryScope::Owner,
    BoundaryScope::SourceFile,
    BoundaryScope::TestFile,
    BoundaryScope::OtherOwner,
];
const OBSERVABLES: [ObservableVariant; 4] = [
    ObservableVariant::NotCarried,
    ObservableVariant::Present,
    ObservableVariant::DeclaredNull,
    ObservableVariant::Blank,
];
const PACKET_PATHS: [PacketPath; 3] = [
    PacketPath::Safe,
    PacketPath::UnsafeSource,
    PacketPath::UnsafeTest,
];
const PROVENANCE_FACTS: [ProvenanceFact; 8] = [
    ProvenanceFact::Change,
    ProvenanceFact::SourceFile,
    ProvenanceFact::Owner,
    ProvenanceFact::Relation,
    ProvenanceFact::Test,
    ProvenanceFact::TestFile,
    ProvenanceFact::Oracle,
    ProvenanceFact::Verify,
];

fn extra_strategy() -> impl Strategy<Value = Extra> {
    (
        0..LIMITATION_KINDS.len(),
        select(&LIMIT_TARGETS),
        0..BOUNDARY_KINDS.len(),
        select(&BOUNDARY_SCOPES),
        any::<bool>(),
        select(&OBSERVABLES),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        select(&PACKET_PATHS),
        prop::option::of(select(&PROVENANCE_FACTS)),
        (any::<bool>(), any::<bool>()),
    )
        .prop_map(
            |(
                limitation_kind,
                limitation_target,
                boundary_kind,
                boundary_scope,
                matcher_grading,
                observable,
                oracle_from_other_test,
                negated_comparison,
                owner_kind_unknown,
                packet_path,
                empty_provenance,
                (relation_for_other_owner, missing_discriminator),
            )| Extra {
                limitation_kind,
                limitation_target,
                boundary_kind,
                boundary_scope,
                matcher_grading,
                observable,
                oracle_from_other_test,
                negated_comparison,
                owner_kind_unknown,
                packet_path,
                empty_provenance,
                relation_for_other_owner,
                missing_discriminator,
            },
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VerifyVariant {
    Valid,
    Missing,
    Empty,
    Unsafe,
    NotATest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReceiptVariant {
    Valid,
    Missing,
    Empty,
    MissingJson,
    Unsafe,
}

/// One point in the mapper's input space, relative to the actionable fixture.
#[derive(Clone, Debug)]
struct Draw {
    packet_status: PacketStatus,
    ask_absent_change: bool,
    owner_confidence: Confidence,
    relation_confidence: Confidence,
    test_confidence: Confidence,
    oracle_confidence: Confidence,
    verify_confidence: Confidence,
    behavior_hint: BehaviorHint,
    oracle_kind: OracleKind,
    oracle_strength: OracleStrength,
    oracle_bound: bool,
    relation_has_oracle: bool,
    relation_kind: RelationKind,
    reachability: ReachabilityHint,
    framework: TestFramework,
    limitation_on_relation: bool,
    boundary_on_owner: bool,
    test_file_has_test_role: bool,
    verify: VerifyVariant,
    receipt: ReceiptVariant,
    allowed_test: bool,
    allowed_source: bool,
    allowed_other: bool,
    allowed_unsafe: bool,
    forbidden_source: bool,
    forbidden_badge: bool,
    forbidden_unsafe: bool,
    stop_if_count: usize,
    production_rule: bool,
    suppression_rule: bool,
    extra: Extra,
}

fn select<T: Clone + std::fmt::Debug + 'static>(values: &[T]) -> prop::sample::Select<T> {
    prop::sample::select(values.to_vec())
}

/// Every enumerated input ranges over its whole domain.
fn uniform_draw() -> impl Strategy<Value = Draw> {
    let packet = (
        select(&[
            PacketStatus::Complete,
            PacketStatus::Partial,
            PacketStatus::Unavailable,
        ]),
        any::<bool>(),
        select(&CONFIDENCES),
        select(&CONFIDENCES),
        select(&CONFIDENCES),
        select(&CONFIDENCES),
        select(&CONFIDENCES),
        select(&BEHAVIOR_HINTS),
        select(&ORACLE_KINDS),
        select(&STRENGTHS),
    );
    let relation = (
        any::<bool>(),
        any::<bool>(),
        select(&RELATION_KINDS),
        select(&[
            ReachabilityHint::Reachable,
            ReachabilityHint::WeaklyReachable,
            ReachabilityHint::StaticUnknown,
        ]),
        select(&FRAMEWORKS),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        select(&[
            VerifyVariant::Valid,
            VerifyVariant::Missing,
            VerifyVariant::Empty,
            VerifyVariant::Unsafe,
            VerifyVariant::NotATest,
        ]),
    );
    let context = (
        select(&[
            ReceiptVariant::Valid,
            ReceiptVariant::Missing,
            ReceiptVariant::Empty,
            ReceiptVariant::MissingJson,
            ReceiptVariant::Unsafe,
        ]),
        prop::array::uniform4(any::<bool>()),
        prop::array::uniform3(any::<bool>()),
        0usize..3,
        any::<bool>(),
        any::<bool>(),
    );
    (packet, relation, context, extra_strategy()).prop_map(
        |(
            (
                packet_status,
                ask_absent_change,
                owner_confidence,
                relation_confidence,
                test_confidence,
                oracle_confidence,
                verify_confidence,
                behavior_hint,
                oracle_kind,
                oracle_strength,
            ),
            (
                oracle_bound,
                relation_has_oracle,
                relation_kind,
                reachability,
                framework,
                limitation_on_relation,
                boundary_on_owner,
                test_file_has_test_role,
                verify,
            ),
            (receipt, allowed, forbidden, stop_if_count, production_rule, suppression_rule),
            extra,
        )| Draw {
            packet_status,
            ask_absent_change,
            owner_confidence,
            relation_confidence,
            test_confidence,
            oracle_confidence,
            verify_confidence,
            behavior_hint,
            oracle_kind,
            oracle_strength,
            oracle_bound,
            relation_has_oracle,
            relation_kind,
            reachability,
            framework,
            limitation_on_relation,
            boundary_on_owner,
            test_file_has_test_role,
            verify,
            receipt,
            allowed_test: allowed[0],
            allowed_source: allowed[1],
            allowed_other: allowed[2],
            allowed_unsafe: allowed[3],
            forbidden_source: forbidden[0],
            forbidden_badge: forbidden[1],
            forbidden_unsafe: forbidden[2],
            stop_if_count,
            production_rule,
            suppression_rule,
            extra,
        },
    )
}

/// Draws that vary only within non-refusing values, so the result is
/// actionable before any evidence is removed.
fn supporting_draw() -> impl Strategy<Value = Draw> {
    (
        select(&CONFIDENCES[..2]),
        prop::array::uniform4(select(&CONFIDENCES[..2])),
        select(&MATCHED_HINTS),
        select(&[RelationKind::DirectOwnerCall, RelationKind::HelperCall]),
        select(&FRAMEWORKS[..6]),
        any::<bool>(),
        1usize..3,
        supporting_extra(),
    )
        .prop_map(
            |(
                owner,
                [relation, test, oracle, verify],
                (hint, kind),
                relation_kind,
                framework,
                badge,
                stop_if_count,
                extra,
            )| Draw {
                packet_status: PacketStatus::Complete,
                ask_absent_change: false,
                owner_confidence: owner,
                relation_confidence: relation,
                test_confidence: test,
                oracle_confidence: oracle,
                verify_confidence: verify,
                behavior_hint: hint,
                oracle_kind: kind,
                oracle_strength: OracleStrength::StrongExact,
                oracle_bound: true,
                relation_has_oracle: true,
                relation_kind,
                reachability: ReachabilityHint::Reachable,
                framework,
                limitation_on_relation: false,
                boundary_on_owner: false,
                test_file_has_test_role: true,
                verify: VerifyVariant::Valid,
                receipt: ReceiptVariant::Valid,
                allowed_test: true,
                allowed_source: false,
                allowed_other: false,
                allowed_unsafe: false,
                forbidden_source: true,
                forbidden_badge: badge,
                forbidden_unsafe: false,
                stop_if_count,
                production_rule: true,
                suppression_rule: true,
                extra,
            },
        )
}

/// Non-refusing extras: an observable that is not carried or present, and
/// limitation and boundary choices that only matter once one is pushed.
fn supporting_extra() -> impl Strategy<Value = Extra> {
    (
        select(&OBSERVABLES[..2]),
        0..LIMITATION_KINDS.len(),
        select(&LIMIT_TARGETS),
        0..BOUNDARY_KINDS.len(),
        select(&BOUNDARY_SCOPES),
    )
        .prop_map(
            |(observable, limitation_kind, limitation_target, boundary_kind, boundary_scope)| {
                Extra {
                    observable,
                    limitation_kind,
                    limitation_target,
                    boundary_kind,
                    boundary_scope,
                    ..Extra::default()
                }
            },
        )
}

/// Number of independently drawn fields in `Draw`; `copy_field` names each.
const DRAW_FIELDS: usize = 43;

/// Overwrite one field of `draw` with the value drawn in `from`.
fn copy_field(draw: &mut Draw, from: &Draw, field: usize) {
    match field {
        0 => draw.packet_status = from.packet_status,
        1 => draw.ask_absent_change = from.ask_absent_change,
        2 => draw.owner_confidence = from.owner_confidence,
        3 => draw.relation_confidence = from.relation_confidence,
        4 => draw.test_confidence = from.test_confidence,
        5 => draw.oracle_confidence = from.oracle_confidence,
        6 => draw.verify_confidence = from.verify_confidence,
        7 => draw.behavior_hint = from.behavior_hint,
        8 => draw.oracle_kind = from.oracle_kind,
        9 => draw.oracle_strength = from.oracle_strength,
        10 => draw.oracle_bound = from.oracle_bound,
        11 => draw.relation_has_oracle = from.relation_has_oracle,
        12 => draw.relation_kind = from.relation_kind,
        13 => draw.reachability = from.reachability,
        14 => draw.framework = from.framework,
        15 => draw.limitation_on_relation = from.limitation_on_relation,
        16 => draw.boundary_on_owner = from.boundary_on_owner,
        17 => draw.test_file_has_test_role = from.test_file_has_test_role,
        18 => draw.verify = from.verify,
        19 => draw.receipt = from.receipt,
        20 => draw.allowed_test = from.allowed_test,
        21 => draw.allowed_source = from.allowed_source,
        22 => draw.allowed_other = from.allowed_other,
        23 => draw.allowed_unsafe = from.allowed_unsafe,
        24 => draw.forbidden_source = from.forbidden_source,
        25 => draw.forbidden_badge = from.forbidden_badge,
        26 => draw.forbidden_unsafe = from.forbidden_unsafe,
        27 => draw.stop_if_count = from.stop_if_count,
        28 => draw.production_rule = from.production_rule,
        29 => draw.suppression_rule = from.suppression_rule,
        30 => draw.extra.limitation_kind = from.extra.limitation_kind,
        31 => draw.extra.limitation_target = from.extra.limitation_target,
        32 => draw.extra.boundary_kind = from.extra.boundary_kind,
        33 => draw.extra.boundary_scope = from.extra.boundary_scope,
        34 => draw.extra.matcher_grading = from.extra.matcher_grading,
        35 => draw.extra.observable = from.extra.observable,
        36 => draw.extra.oracle_from_other_test = from.extra.oracle_from_other_test,
        37 => draw.extra.negated_comparison = from.extra.negated_comparison,
        38 => draw.extra.owner_kind_unknown = from.extra.owner_kind_unknown,
        39 => draw.extra.packet_path = from.extra.packet_path,
        40 => draw.extra.empty_provenance = from.extra.empty_provenance,
        41 => draw.extra.relation_for_other_owner = from.extra.relation_for_other_owner,
        42 => draw.extra.missing_discriminator = from.extra.missing_discriminator,
        _ => {}
    }
}

/// A non-refusing draw with one to three fields replaced by values from the
/// whole domain. Uniform draws almost always trip several refusals at once, so
/// without this a mapper that ignored any single condition would still pass;
/// here a lone refusal is common and must be the one that refuses.
fn near_supporting_draw() -> impl Strategy<Value = Draw> {
    (
        supporting_draw(),
        uniform_draw(),
        prop::collection::vec(0..DRAW_FIELDS, 1..4),
    )
        .prop_map(|(mut draw, from, fields)| {
            for field in fields {
                copy_field(&mut draw, &from, field);
            }
            draw
        })
}

/// Uniform draws over the whole input space, plus near-actionable draws.
fn any_draw() -> impl Strategy<Value = Draw> {
    prop_oneof![1 => uniform_draw(), 3 => near_supporting_draw()]
}

fn weak(confidence: Confidence) -> bool {
    !confidence.is_strict_actionable()
}

/// The refusal conditions `draw` triggers, named from the mapper's checks.
fn refusals(draw: &Draw) -> Vec<&'static str> {
    let mut found = Vec::new();
    let mut note = |condition: bool, name: &'static str| {
        if condition {
            found.push(name);
        }
    };
    note(
        draw.packet_status != PacketStatus::Complete,
        "packet_not_complete",
    );
    note(draw.ask_absent_change, "missing_change");
    note(
        [
            draw.owner_confidence,
            draw.relation_confidence,
            draw.test_confidence,
            draw.oracle_confidence,
            draw.verify_confidence,
        ]
        .into_iter()
        .any(weak),
        "low_confidence",
    );
    note(
        draw.behavior_hint.repair_kind().is_none(),
        "unsupported_behavior",
    );
    note(
        !matches!(
            draw.relation_kind,
            RelationKind::DirectOwnerCall | RelationKind::HelperCall
        ),
        "advisory_relation_kind",
    );
    note(
        draw.reachability != ReachabilityHint::Reachable,
        "not_reachable",
    );
    note(!draw.relation_has_oracle, "relation_without_oracle");
    note(!draw.oracle_bound, "oracle_not_bound_to_owner");
    note(
        draw.oracle_strength != OracleStrength::StrongExact,
        "oracle_not_strong_exact",
    );
    note(
        draw.framework == TestFramework::Unknown,
        "unsupported_framework",
    );
    note(
        draw.oracle_kind.assertion_shape() != draw.behavior_hint.default_assertion_shape(),
        "oracle_shape_mismatch",
    );
    let extra = &draw.extra;
    let limitation_blocks = LIMITATION_KINDS
        .get(extra.limitation_kind)
        .is_some_and(|(_, blocks)| *blocks);
    note(
        draw.limitation_on_relation
            && limitation_blocks
            && extra.limitation_target != LimitTarget::Unrelated,
        "blocking_limitation",
    );
    note(
        draw.boundary_on_owner && extra.boundary_scope != BoundaryScope::OtherOwner,
        "dynamic_boundary",
    );
    note(
        extra.matcher_grading && draw.oracle_kind == OracleKind::ExceptionObserver,
        "matcher_graded_exception_oracle",
    );
    note(
        matches!(
            extra.observable,
            ObservableVariant::DeclaredNull | ObservableVariant::Blank
        ),
        "changed_observable_declared_absent",
    );
    note(extra.oracle_from_other_test, "oracle_from_another_test");
    note(extra.negated_comparison, "negated_comparison_oracle");
    note(extra.owner_kind_unknown, "missing_canonical_owner_identity");
    note(extra.packet_path != PacketPath::Safe, "unsafe_packet_path");
    note(
        extra.empty_provenance.is_some(),
        "missing_fact_provenance_refs",
    );
    note(
        extra.missing_discriminator,
        "missing_concrete_discriminator",
    );
    note(extra.relation_for_other_owner, "relation_for_another_owner");
    note(!draw.test_file_has_test_role, "test_file_not_test_role");
    note(
        draw.verify != VerifyVariant::Valid,
        "invalid_verify_command",
    );
    note(
        draw.receipt != ReceiptVariant::Valid,
        "invalid_receipt_command",
    );
    note(
        draw.allowed_unsafe || draw.forbidden_unsafe,
        "unsafe_edit_boundary",
    );
    note(!draw.allowed_test, "missing_allowed_test_path");
    note(draw.allowed_source, "allowed_production_path");
    note(draw.allowed_other, "unexpected_allowed_path");
    note(!draw.forbidden_source, "missing_forbidden_source_path");
    note(draw.stop_if_count == 0, "missing_stop_if");
    note(
        !(draw.production_rule && draw.suppression_rule),
        "missing_must_not_change",
    );
    found
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn base_packet() -> Result<PerlFactPacket, TestCaseError> {
    concrete_return_packet().map_err(TestCaseError::fail)
}

/// Apply `draw` to a fresh copy of the fixture packet and build its context.
fn build(
    base: &PerlFactPacket,
    draw: &Draw,
) -> Result<(PerlFactPacket, PerlActionabilityContext), TestCaseError> {
    let mut packet = base.clone();
    let missing = |what: &str| TestCaseError::fail(format!("fixture lost its {what} row"));
    packet.packet_status = draw.packet_status;

    let owner = packet
        .owners
        .iter_mut()
        .find(|owner| owner.owner_id == OWNER_ID)
        .ok_or_else(|| missing("owner"))?;
    owner.confidence = draw.owner_confidence;
    if draw.extra.owner_kind_unknown {
        owner.kind = OwnerKind::Unknown;
    }

    let change = packet
        .changes
        .iter_mut()
        .find(|change| change.change_id == CHANGE_ID)
        .ok_or_else(|| missing("change"))?;
    if draw.extra.missing_discriminator {
        change.missing_discriminator = None;
    }
    change.behavior_hint = draw.behavior_hint;
    change.changed_observable = ObservableField(match draw.extra.observable {
        ObservableVariant::NotCarried => None,
        ObservableVariant::Present => Some(Some("$amount / 2".to_string())),
        ObservableVariant::DeclaredNull => Some(None),
        ObservableVariant::Blank => Some(Some("  ".to_string())),
    });
    let change_range = change.range;

    let relation = packet
        .relations
        .iter_mut()
        .find(|relation| relation.relation_id == RELATION_ID)
        .ok_or_else(|| missing("relation"))?;
    relation.confidence = draw.relation_confidence;
    relation.relation_kind = draw.relation_kind;
    relation.reachability_hint = draw.reachability;
    if !draw.relation_has_oracle {
        relation.oracle_id = None;
    }
    if draw.extra.relation_for_other_owner {
        relation.owner_id = OTHER_OWNER_ID.to_string();
    }

    let test = packet
        .tests
        .iter_mut()
        .find(|test| test.test_id == TEST_ID)
        .ok_or_else(|| missing("test"))?;
    test.confidence = draw.test_confidence;
    test.framework = draw.framework;

    let oracle = packet
        .oracles
        .iter_mut()
        .find(|oracle| oracle.oracle_id == ORACLE_ID)
        .ok_or_else(|| missing("oracle"))?;
    oracle.confidence = draw.oracle_confidence;
    oracle.kind = draw.oracle_kind;
    oracle.strength = draw.oracle_strength;
    if !draw.oracle_bound {
        oracle.target_owner_id = None;
    }
    if draw.extra.oracle_from_other_test {
        oracle.test_id = OTHER_TEST_ID.to_string();
    }
    if draw.extra.negated_comparison {
        oracle.expression =
            Some("isnt(My::App::discount(100), 11, 'discount threshold')".to_string());
    }

    let test_file = packet
        .files
        .iter_mut()
        .find(|file| file.file_id == TEST_FILE_ID)
        .ok_or_else(|| missing("test file"))?;
    if !draw.test_file_has_test_role {
        test_file.role.retain(|role| *role != FileRole::Test);
    }
    if draw.extra.packet_path == PacketPath::UnsafeTest {
        test_file.path = "t/../t/app.t".to_string();
    }
    if draw.extra.packet_path == PacketPath::UnsafeSource {
        let source_file = packet
            .files
            .iter_mut()
            .find(|file| file.file_id == SOURCE_FILE_ID)
            .ok_or_else(|| missing("source file"))?;
        source_file.path = "lib/My/../My/App.pm".to_string();
    }
    if draw.extra.matcher_grading {
        packet
            .producer
            .capabilities
            .push(PERL_ORACLE_MATCHER_GRADING.to_string());
    }
    if let Some(fact) = draw.extra.empty_provenance {
        clear_provenance_refs(&mut packet, fact);
    }

    let verify_position = packet
        .verify_commands
        .iter()
        .position(|command| command.command_id == VERIFY_ID)
        .ok_or_else(|| missing("verify command"))?;
    match draw.verify {
        VerifyVariant::Missing => {
            packet.verify_commands.remove(verify_position);
        }
        variant => {
            let command = packet
                .verify_commands
                .get_mut(verify_position)
                .ok_or_else(|| missing("verify command"))?;
            command.confidence = draw.verify_confidence;
            command.argv = match variant {
                VerifyVariant::Empty => Vec::new(),
                VerifyVariant::Unsafe => strings(&["prove", "t/app.t;rm"]),
                VerifyVariant::NotATest => strings(&["prove", "lib/My/App.pm"]),
                _ => strings(&["prove", TEST_PATH]),
            };
        }
    }

    if draw.limitation_on_relation {
        let kind = LIMITATION_KINDS
            .get(draw.extra.limitation_kind)
            .map_or("framework_indirection", |(kind, _)| *kind);
        let evidence_refs = match draw.extra.limitation_target {
            LimitTarget::Relation => vec![RELATION_ID.to_string()],
            LimitTarget::Change => vec![CHANGE_ID.to_string()],
            LimitTarget::Test => vec![TEST_ID.to_string()],
            LimitTarget::NoRefs => Vec::new(),
            LimitTarget::Unrelated => vec!["oracle:t/app_helper.t:10:unknown-helper".to_string()],
        };
        packet.limitations.push(LimitationFact {
            limitation_id: "limitation:proptest:relation".to_string(),
            kind: kind.to_string(),
            message: "an opaque assertion boundary on the related test".to_string(),
            evidence_refs,
        });
    }
    if draw.boundary_on_owner {
        let (file_id, owner_id) = match draw.extra.boundary_scope {
            BoundaryScope::Owner => (SOURCE_FILE_ID, Some(OWNER_ID)),
            BoundaryScope::SourceFile => (SOURCE_FILE_ID, None),
            BoundaryScope::TestFile => (TEST_FILE_ID, None),
            BoundaryScope::OtherOwner => (SOURCE_FILE_ID, Some(OTHER_OWNER_ID)),
        };
        packet.dynamic_boundaries.push(DynamicBoundaryFact {
            boundary_id: "boundary:proptest:owner".to_string(),
            kind: BOUNDARY_KINDS
                .get(draw.extra.boundary_kind)
                .copied()
                .unwrap_or(BoundaryKind::DynamicDispatch),
            file_id: file_id.to_string(),
            owner_id: owner_id.map(str::to_string),
            range: change_range,
            confidence: Confidence::High,
            provenance_refs: Vec::new(),
        });
    }
    packet.reset_index();

    let valid_receipt = [
        "ripr",
        "agent",
        "receipt",
        "--root",
        ".",
        "--verify-json",
        "target/ripr/workflow/agent-verify.json",
        "--seam-id",
        "perl-gap",
        "--json",
    ];
    let receipt_command = match draw.receipt {
        ReceiptVariant::Valid => Some(strings(&valid_receipt)),
        ReceiptVariant::Missing => None,
        ReceiptVariant::Empty => Some(Vec::new()),
        ReceiptVariant::MissingJson => Some(strings(&valid_receipt[..9])),
        ReceiptVariant::Unsafe => {
            let mut command = strings(&valid_receipt);
            if let Some(seam) = command.get_mut(8) {
                *seam = "perl-gap;rm".to_string();
            }
            Some(command)
        }
    };
    let mut allowed_edit_boundaries = Vec::new();
    for (include, path) in [
        (draw.allowed_test, TEST_PATH),
        (draw.allowed_source, SOURCE_PATH),
        (draw.allowed_other, "t/other.t"),
        (draw.allowed_unsafe, "../t/app.t"),
    ] {
        if include {
            allowed_edit_boundaries.push(path.to_string());
        }
    }
    let mut forbidden_edit_boundaries = Vec::new();
    for (include, path) in [
        (draw.forbidden_source, SOURCE_PATH),
        (draw.forbidden_badge, "badges/ripr-plus.json"),
        (draw.forbidden_unsafe, "/etc/passwd"),
    ] {
        if include {
            forbidden_edit_boundaries.push(path.to_string());
        }
    }
    let stop_if = strings(
        &[
            "perl-lsp packet status changes",
            "related test no longer reaches owner",
        ][..draw.stop_if_count],
    );
    let mut must_not_change = Vec::new();
    if draw.production_rule {
        must_not_change.push(PRODUCTION_RULE.to_string());
    }
    if draw.suppression_rule {
        must_not_change.push(SUPPRESSION_RULE.to_string());
    }
    Ok((
        packet,
        PerlActionabilityContext {
            receipt_command,
            allowed_edit_boundaries,
            forbidden_edit_boundaries,
            stop_if,
            must_not_change,
        },
    ))
}

/// Empty the `provenance_refs` of one fact the raw evidence refs cite.
fn clear_provenance_refs(packet: &mut PerlFactPacket, fact: ProvenanceFact) {
    match fact {
        ProvenanceFact::Change => {
            for change in &mut packet.changes {
                if change.change_id == CHANGE_ID {
                    change.provenance_refs.clear();
                }
            }
        }
        ProvenanceFact::SourceFile | ProvenanceFact::TestFile => {
            let file_id = if fact == ProvenanceFact::SourceFile {
                SOURCE_FILE_ID
            } else {
                TEST_FILE_ID
            };
            for file in &mut packet.files {
                if file.file_id == file_id {
                    file.provenance_refs.clear();
                }
            }
        }
        ProvenanceFact::Owner => {
            for owner in &mut packet.owners {
                if owner.owner_id == OWNER_ID {
                    owner.provenance_refs.clear();
                }
            }
        }
        ProvenanceFact::Relation => {
            for relation in &mut packet.relations {
                if relation.relation_id == RELATION_ID {
                    relation.provenance_refs.clear();
                }
            }
        }
        ProvenanceFact::Test => {
            for test in &mut packet.tests {
                if test.test_id == TEST_ID {
                    test.provenance_refs.clear();
                }
            }
        }
        ProvenanceFact::Oracle => {
            for oracle in &mut packet.oracles {
                if oracle.oracle_id == ORACLE_ID {
                    oracle.provenance_refs.clear();
                }
            }
        }
        ProvenanceFact::Verify => {
            for command in &mut packet.verify_commands {
                if command.command_id == VERIFY_ID {
                    command.provenance_refs.clear();
                }
            }
        }
    }
}

fn change_id(draw: &Draw) -> &'static str {
    if draw.ask_absent_change {
        "change:lib/My/App.pm:99:absent"
    } else {
        CHANGE_ID
    }
}

/// Supporting evidence a removal can take away. Every entry is something the
/// mapper requires, except `ForbiddenBadgePath` and `OneStopIf`, which only
/// drop surplus context and exercise monotonicity on non-refusing removals.
/// `ProvenanceRows` is required: the raw evidence refs resolve every cited
/// provenance id, so clearing the rows refuses with `MissingProvenanceRefs`.
#[derive(Clone, Copy, Debug)]
enum Removal {
    ConcreteDiscriminator,
    RelationOracleId,
    OracleRow,
    OracleTargetOwner,
    VerifyCommandRow,
    TestRow,
    OwnerRow,
    ChangeRow,
    SourceFileRow,
    TestFileRow,
    TestFileTestRole,
    RelationRow,
    ReceiptCommand,
    AllowedTestPath,
    ForbiddenSourcePath,
    AllStopIf,
    ProductionRule,
    SuppressionRule,
    ProvenanceRows,
    // Not required by the mapper:
    ForbiddenBadgePath,
    OneStopIf,
}

const REMOVALS: [Removal; 21] = [
    Removal::ConcreteDiscriminator,
    Removal::RelationOracleId,
    Removal::OracleRow,
    Removal::OracleTargetOwner,
    Removal::VerifyCommandRow,
    Removal::TestRow,
    Removal::OwnerRow,
    Removal::ChangeRow,
    Removal::SourceFileRow,
    Removal::TestFileRow,
    Removal::TestFileTestRole,
    Removal::RelationRow,
    Removal::ReceiptCommand,
    Removal::AllowedTestPath,
    Removal::ForbiddenSourcePath,
    Removal::AllStopIf,
    Removal::ProductionRule,
    Removal::SuppressionRule,
    Removal::ForbiddenBadgePath,
    Removal::OneStopIf,
    Removal::ProvenanceRows,
];

impl Removal {
    fn required(self) -> bool {
        !matches!(self, Removal::ForbiddenBadgePath | Removal::OneStopIf)
    }

    fn apply(self, packet: &mut PerlFactPacket, context: &mut PerlActionabilityContext) {
        match self {
            Removal::ConcreteDiscriminator => {
                for change in &mut packet.changes {
                    if change.change_id == CHANGE_ID {
                        change.missing_discriminator = None;
                    }
                }
            }
            Removal::RelationOracleId => {
                for relation in &mut packet.relations {
                    relation.oracle_id = None;
                }
            }
            Removal::OracleRow => packet
                .oracles
                .retain(|oracle| oracle.oracle_id != ORACLE_ID),
            Removal::OracleTargetOwner => {
                for oracle in &mut packet.oracles {
                    if oracle.oracle_id == ORACLE_ID {
                        oracle.target_owner_id = None;
                    }
                }
            }
            Removal::VerifyCommandRow => packet
                .verify_commands
                .retain(|command| command.command_id != VERIFY_ID),
            Removal::TestRow => packet.tests.retain(|test| test.test_id != TEST_ID),
            Removal::OwnerRow => packet.owners.retain(|owner| owner.owner_id != OWNER_ID),
            Removal::ChangeRow => packet
                .changes
                .retain(|change| change.change_id != CHANGE_ID),
            Removal::SourceFileRow => packet.files.retain(|file| file.file_id != SOURCE_FILE_ID),
            Removal::TestFileRow => packet.files.retain(|file| file.file_id != TEST_FILE_ID),
            Removal::TestFileTestRole => {
                for file in &mut packet.files {
                    if file.file_id == TEST_FILE_ID {
                        file.role.retain(|role| *role != FileRole::Test);
                    }
                }
            }
            Removal::RelationRow => packet
                .relations
                .retain(|relation| relation.relation_id != RELATION_ID),
            Removal::ReceiptCommand => context.receipt_command = None,
            Removal::AllowedTestPath => context
                .allowed_edit_boundaries
                .retain(|path| path != TEST_PATH),
            Removal::ForbiddenSourcePath => context
                .forbidden_edit_boundaries
                .retain(|path| path != SOURCE_PATH),
            Removal::AllStopIf => context.stop_if.clear(),
            Removal::ProductionRule => context
                .must_not_change
                .retain(|rule| rule != PRODUCTION_RULE),
            Removal::SuppressionRule => context
                .must_not_change
                .retain(|rule| rule != SUPPRESSION_RULE),
            Removal::ForbiddenBadgePath => context
                .forbidden_edit_boundaries
                .retain(|path| path != "badges/ripr-plus.json"),
            Removal::OneStopIf => {
                context.stop_if.truncate(1);
            }
            Removal::ProvenanceRows => packet.provenance.clear(),
        }
        packet.reset_index();
    }
}

fn actionable_after(
    base: &PerlFactPacket,
    draw: &Draw,
    removals: &[Removal],
) -> Result<bool, TestCaseError> {
    let (mut packet, mut context) = build(base, draw)?;
    for removal in removals {
        removal.apply(&mut packet, &mut context);
    }
    let result = packet.strict_actionability_for_change(change_id(draw), &context);
    if let Ok(actionable) = &result {
        prop_assert_eq!(actionable.gap_state, PerlGapState::Actionable);
    }
    Ok(result.is_ok())
}

/// Only the discriminator channel changes across these fixture cases.
#[test]
fn perl_actionability_proptest_fixture_is_actionable() -> Result<(), String> {
    let context = super::tests::complete_perl_actionability_context();
    // The honesty corpus captures this exact packet on the public advisory surface.
    // Exercise its private strict mapper before perturbing either producer channel.
    let original = consume(RETURN_PACKET)?;
    assert_eq!(
        original.strict_actionability_for_change(CHANGE_ID, &context),
        Err(PerlActionabilityBlocker::MissingConcreteDiscriminator)
    );
    for (field, digest, expected) in [
        (Some("net amount"), "sha256:return", Some("net amount")),
        (None, "discriminator:net amount", Some("net amount")),
        (None, "sha256:return", None),
        (Some(" "), "discriminator:  ", None),
    ] {
        let mut packet = consume(RETURN_PACKET)?;
        let change = packet
            .changes
            .iter_mut()
            .find(|change| change.change_id == CHANGE_ID)
            .ok_or("missing return change")?;
        change.missing_discriminator = field.map(str::to_string);
        change.changed_text_digest = digest.to_string();
        let result = packet.strict_actionability_for_change(CHANGE_ID, &context);
        if let Some(value) = expected {
            let actionable = result.map_err(|err| format!("{err:?}"))?;
            assert_eq!(actionable.gap_state, PerlGapState::Actionable);
            assert_eq!(actionable.missing_discriminator, value);
        } else {
            assert_eq!(
                result,
                Err(PerlActionabilityBlocker::MissingConcreteDiscriminator)
            );
            assert_eq!(
                packet.repair_card_for_change(CHANGE_ID, &context),
                Err(PerlActionabilityBlocker::MissingConcreteDiscriminator)
            );
            assert_eq!(
                packet.agent_packet_for_change(CHANGE_ID, &context),
                Err(PerlActionabilityBlocker::MissingConcreteDiscriminator)
            );
            let advisory = packet
                .canonical_gap_identity_for_change(CHANGE_ID)
                .ok_or("missing advisory identity")?;
            assert_eq!(advisory.missing_discriminator, "return_value");
        }
    }
    Ok(())
}

/// The supporting draw includes a concrete return discriminator.
fn fixture_draw() -> Draw {
    Draw {
        packet_status: PacketStatus::Complete,
        ask_absent_change: false,
        owner_confidence: Confidence::High,
        relation_confidence: Confidence::Medium,
        test_confidence: Confidence::Medium,
        oracle_confidence: Confidence::Medium,
        verify_confidence: Confidence::Medium,
        behavior_hint: BehaviorHint::ReturnValue,
        oracle_kind: OracleKind::ExactReturnAssertion,
        oracle_strength: OracleStrength::StrongExact,
        oracle_bound: true,
        relation_has_oracle: true,
        relation_kind: RelationKind::DirectOwnerCall,
        reachability: ReachabilityHint::Reachable,
        framework: TestFramework::TestMore,
        limitation_on_relation: false,
        boundary_on_owner: false,
        test_file_has_test_role: true,
        verify: VerifyVariant::Valid,
        receipt: ReceiptVariant::Valid,
        allowed_test: true,
        allowed_source: false,
        allowed_other: false,
        allowed_unsafe: false,
        forbidden_source: true,
        forbidden_badge: true,
        forbidden_unsafe: false,
        stop_if_count: 2,
        production_rule: true,
        suppression_rule: true,
        extra: Extra::default(),
    }
}

/// Every single-field departure from the fixture draw, over each field's
/// whole domain, plus every behavior-hint and oracle-kind pair. Random draws
/// rarely isolate one refusal, so this exhaustive sweep is what pins each
/// condition on its own regardless of the case budget.
fn single_field_draws() -> Vec<Draw> {
    let base = fixture_draw();
    let mut draws = Vec::new();
    let mut push = |edit: &dyn Fn(&mut Draw)| {
        let mut draw = base.clone();
        edit(&mut draw);
        draws.push(draw);
    };
    for status in [
        PacketStatus::Complete,
        PacketStatus::Partial,
        PacketStatus::Unavailable,
    ] {
        push(&|draw| draw.packet_status = status);
    }
    push(&|draw| draw.ask_absent_change = true);
    for confidence in CONFIDENCES {
        push(&|draw| draw.owner_confidence = confidence);
        push(&|draw| draw.relation_confidence = confidence);
        push(&|draw| draw.test_confidence = confidence);
        push(&|draw| draw.oracle_confidence = confidence);
        push(&|draw| draw.verify_confidence = confidence);
    }
    for hint in BEHAVIOR_HINTS {
        for kind in ORACLE_KINDS {
            push(&|draw| {
                draw.behavior_hint = hint;
                draw.oracle_kind = kind;
            });
        }
    }
    for strength in STRENGTHS {
        push(&|draw| draw.oracle_strength = strength);
    }
    for kind in RELATION_KINDS {
        push(&|draw| draw.relation_kind = kind);
    }
    for reachability in [
        ReachabilityHint::Reachable,
        ReachabilityHint::WeaklyReachable,
        ReachabilityHint::StaticUnknown,
    ] {
        push(&|draw| draw.reachability = reachability);
    }
    for framework in FRAMEWORKS {
        push(&|draw| draw.framework = framework);
    }
    for verify in [
        VerifyVariant::Missing,
        VerifyVariant::Empty,
        VerifyVariant::Unsafe,
        VerifyVariant::NotATest,
    ] {
        push(&|draw| draw.verify = verify);
    }
    for receipt in [
        ReceiptVariant::Missing,
        ReceiptVariant::Empty,
        ReceiptVariant::MissingJson,
        ReceiptVariant::Unsafe,
    ] {
        push(&|draw| draw.receipt = receipt);
    }
    for stop_if_count in 0..3 {
        push(&|draw| draw.stop_if_count = stop_if_count);
    }
    push(&|draw| draw.oracle_bound = false);
    push(&|draw| draw.relation_has_oracle = false);
    push(&|draw| draw.limitation_on_relation = true);
    push(&|draw| draw.boundary_on_owner = true);
    push(&|draw| draw.test_file_has_test_role = false);
    push(&|draw| draw.allowed_test = false);
    push(&|draw| draw.allowed_source = true);
    push(&|draw| draw.allowed_other = true);
    push(&|draw| draw.allowed_unsafe = true);
    push(&|draw| draw.forbidden_source = false);
    push(&|draw| draw.forbidden_badge = false);
    push(&|draw| draw.forbidden_unsafe = true);
    push(&|draw| draw.production_rule = false);
    push(&|draw| draw.suppression_rule = false);
    for limitation_kind in 0..LIMITATION_KINDS.len() {
        for target in LIMIT_TARGETS {
            push(&|draw| {
                draw.limitation_on_relation = true;
                draw.extra.limitation_kind = limitation_kind;
                draw.extra.limitation_target = target;
            });
        }
    }
    for boundary_kind in 0..BOUNDARY_KINDS.len() {
        for scope in BOUNDARY_SCOPES {
            push(&|draw| {
                draw.boundary_on_owner = true;
                draw.extra.boundary_kind = boundary_kind;
                draw.extra.boundary_scope = scope;
            });
        }
    }
    for (hint, kind) in MATCHED_HINTS {
        push(&|draw| {
            draw.extra.matcher_grading = true;
            draw.behavior_hint = hint;
            draw.oracle_kind = kind;
        });
    }
    for observable in OBSERVABLES {
        push(&|draw| draw.extra.observable = observable);
    }
    for packet_path in PACKET_PATHS {
        push(&|draw| draw.extra.packet_path = packet_path);
    }
    for fact in PROVENANCE_FACTS {
        push(&|draw| draw.extra.empty_provenance = Some(fact));
    }
    push(&|draw| draw.extra.oracle_from_other_test = true);
    push(&|draw| draw.extra.negated_comparison = true);
    push(&|draw| draw.extra.owner_kind_unknown = true);
    push(&|draw| draw.extra.missing_discriminator = true);
    push(&|draw| draw.extra.relation_for_other_owner = true);
    draws
}

/// Each refusal condition, alone, refuses; each single departure that names
/// no refusal stays actionable.
#[test]
fn perl_strict_actionability_refuses_each_condition_alone() -> Result<(), String> {
    let base = concrete_return_packet()?;
    let draws = single_field_draws();
    let mut refused = 0usize;
    let mut disagreements = Vec::new();
    for draw in &draws {
        let refusals = refusals(draw);
        let actionable = actionable_after(&base, draw, &[]).map_err(|err| err.to_string())?;
        if refusals.is_empty() != actionable {
            disagreements.push(format!("refusals {refusals:?} vs actionable={actionable}"));
        }
        refused += usize::from(!actionable);
    }
    if !disagreements.is_empty() {
        return Err(format!(
            "{} sweep draws disagree: {disagreements:#?}",
            disagreements.len()
        ));
    }
    if refused == 0 || refused == draws.len() {
        return Err(format!(
            "sweep is degenerate: {refused} of {} refused",
            draws.len()
        ));
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// Over the perturbed input space: any refusal condition yields a refusal,
    /// and a draw with none of them is actionable. This makes `refusals` the
    /// complete list of the conditions reachable from these perturbations, not
    /// of every production refusal path (see the module doc for omissions).
    #[test]
    fn perl_strict_actionability_refuses_whenever_a_refusal_condition_holds(draw in any_draw()) {
        let base = base_packet()?;
        let refusals = refusals(&draw);
        let actionable = actionable_after(&base, &draw, &[])?;
        if refusals.is_empty() {
            prop_assert!(actionable, "no refusal condition holds, yet refused: {draw:?}");
        } else {
            prop_assert!(!actionable, "actionable despite refusal conditions {refusals:?}: {draw:?}");
        }
    }

    /// Removing supporting evidence never turns a refusal into an actionable
    /// result: for removal sets S within T, refused(S) implies refused(T).
    /// Removing any required item always refuses.
    #[test]
    fn perl_strict_actionability_is_monotone_in_supporting_evidence(
        draw in supporting_draw(),
        larger in prop::collection::vec(any::<bool>(), REMOVALS.len()),
        keep in prop::collection::vec(any::<bool>(), REMOVALS.len()),
    ) {
        let base = base_packet()?;
        prop_assert!(refusals(&draw).is_empty());
        let larger_set: Vec<Removal> = REMOVALS
            .iter()
            .zip(&larger)
            .filter(|(_, chosen)| **chosen)
            .map(|(removal, _)| *removal)
            .collect();
        let smaller_set: Vec<Removal> = REMOVALS
            .iter()
            .zip(larger.iter().zip(&keep))
            .filter(|(_, (chosen, kept))| **chosen && **kept)
            .map(|(removal, _)| *removal)
            .collect();
        prop_assert!(actionable_after(&base, &draw, &[])?, "supporting draw must start actionable: {draw:?}");
        let smaller = actionable_after(&base, &draw, &smaller_set)?;
        let larger_ok = actionable_after(&base, &draw, &larger_set)?;
        prop_assert!(
            smaller || !larger_ok,
            "removing more evidence promoted a refusal: {smaller_set:?} refused, {larger_set:?} actionable"
        );
        if larger_set.iter().any(|removal| removal.required()) {
            prop_assert!(!larger_ok, "actionable after removing required evidence {larger_set:?}");
        }
        for removal in REMOVALS.iter().filter(|removal| removal.required()) {
            prop_assert!(
                !actionable_after(&base, &draw, &[*removal])?,
                "actionable after removing required {removal:?}"
            );
        }
    }
}
