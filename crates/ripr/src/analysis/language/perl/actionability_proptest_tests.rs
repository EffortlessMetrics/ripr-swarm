//! Property tests for the Perl strict actionability mapper (#13948).
//!
//! `strict_actionability_for_change` is the only Perl path that returns
//! `PerlGapState::Actionable`. The example tests in `tests.rs` knock out one
//! field at a time; these properties perturb every enumerated input of the
//! packet row the mapper selects and of the caller's context at once, starting
//! from the committed actionable fixture.
//!
//! Refusal conditions enumerated from the mapper (and the classifier it reads
//! the selected evidence class from):
//!
//! - packet status other than `complete`;
//! - the requested change is absent;
//! - owner confidence, or any confidence folded into the evidence confidence
//!   (owner, relation, test, oracle, verify command), is `low`/`unknown`;
//! - a behavior hint without a repair kind (`call_effect`, `unknown`);
//! - relation kind other than `direct_owner_call`/`helper_call`;
//! - reachability other than `reachable`;
//! - the relation has no oracle, the oracle is not bound to the owner, or its
//!   strength is not `strong_exact` (the evidence is not `weakly_exposed`);
//! - an `unknown` test framework;
//! - the oracle shape is not the behavior hint's default assertion shape;
//! - a blocking limitation or dynamic boundary applies (static limit);
//! - a missing, empty, unsafe or non-test verify command;
//! - a missing, empty or invalid receipt command;
//! - a test file without the `test` role;
//! - an unsafe allowed or forbidden edit path, an allowed path set other than
//!   exactly the related test file, or a forbidden set without the source;
//! - empty `stop_if`, or `must_not_change` without both required rules.
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

use super::tests::consume;
use super::*;
use proptest::prelude::*;
use proptest::test_runner::Config;

const DEFAULT_CASES: u32 = 64;

const FIXTURE: &str = include_str!(
    "../../../../../../fixtures/perl_lsp_facts_exporter/expected/ripr-perl-source-test-oracle-facts-v1.json"
);
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
    (packet, relation, context).prop_map(
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
            },
        )
}

/// Number of independently drawn fields in `Draw`; `copy_field` names each.
const DRAW_FIELDS: usize = 30;

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
    note(draw.limitation_on_relation, "blocking_limitation");
    note(draw.boundary_on_owner, "dynamic_boundary");
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
    consume(FIXTURE).map_err(TestCaseError::fail)
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

    let change = packet
        .changes
        .iter_mut()
        .find(|change| change.change_id == CHANGE_ID)
        .ok_or_else(|| missing("change"))?;
    change.behavior_hint = draw.behavior_hint;
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

    let test_file = packet
        .files
        .iter_mut()
        .find(|file| file.file_id == TEST_FILE_ID)
        .ok_or_else(|| missing("test file"))?;
    if !draw.test_file_has_test_role {
        test_file.role.retain(|role| *role != FileRole::Test);
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
        packet.limitations.push(LimitationFact {
            limitation_id: "limitation:proptest:relation".to_string(),
            kind: "framework_indirection".to_string(),
            message: "an opaque assertion boundary on the related test".to_string(),
            evidence_refs: vec![RELATION_ID.to_string()],
        });
    }
    if draw.boundary_on_owner {
        packet.dynamic_boundaries.push(DynamicBoundaryFact {
            boundary_id: "boundary:proptest:owner".to_string(),
            kind: BoundaryKind::DynamicDispatch,
            file_id: SOURCE_FILE_ID.to_string(),
            owner_id: Some(OWNER_ID.to_string()),
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

fn change_id(draw: &Draw) -> &'static str {
    if draw.ask_absent_change {
        "change:lib/My/App.pm:99:absent"
    } else {
        CHANGE_ID
    }
}

/// Supporting evidence a removal can take away. Every entry is something the
/// mapper requires, except the last three, which only drop surplus context or
/// provenance and are present to exercise monotonicity on non-refusing
/// removals.
#[derive(Clone, Copy, Debug)]
enum Removal {
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
    // Not required by the mapper:
    ForbiddenBadgePath,
    OneStopIf,
    ProvenanceRows,
}

const REMOVALS: [Removal; 20] = [
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
        !matches!(
            self,
            Removal::ForbiddenBadgePath | Removal::OneStopIf | Removal::ProvenanceRows
        )
    }

    fn apply(self, packet: &mut PerlFactPacket, context: &mut PerlActionabilityContext) {
        match self {
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

/// The unperturbed fixture is actionable, so the refusal properties below
/// are not vacuously satisfied by a fixture that never promotes.
#[test]
fn perl_actionability_proptest_fixture_is_actionable() -> Result<(), String> {
    let packet = consume(FIXTURE)?;
    let result = packet.strict_actionability_for_change(
        CHANGE_ID,
        &PerlActionabilityContext {
            receipt_command: Some(strings(&[
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
            ])),
            allowed_edit_boundaries: strings(&[TEST_PATH]),
            forbidden_edit_boundaries: strings(&[SOURCE_PATH]),
            stop_if: strings(&["perl-lsp packet status changes"]),
            must_not_change: strings(&[PRODUCTION_RULE, SUPPRESSION_RULE]),
        },
    );
    match result {
        Ok(actionable) if actionable.gap_state == PerlGapState::Actionable => Ok(()),
        other => Err(format!("fixture must be actionable, got {other:?}")),
    }
}

/// The fixture's own values as a `Draw`: actionable, no refusal condition.
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
    draws
}

/// Each refusal condition, alone, refuses; each single departure that names
/// no refusal stays actionable.
#[test]
fn perl_strict_actionability_refuses_each_condition_alone() -> Result<(), String> {
    let base = consume(FIXTURE)?;
    let draws = single_field_draws();
    let mut refused = 0usize;
    for draw in &draws {
        let refusals = refusals(draw);
        let actionable = actionable_after(&base, draw, &[]).map_err(|err| err.to_string())?;
        if refusals.is_empty() != actionable {
            return Err(format!(
                "refusals {refusals:?} disagree with actionable={actionable} for {draw:?}"
            ));
        }
        refused += usize::from(!actionable);
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

    /// Over the whole enumerated input space: any refusal condition yields a
    /// refusal, and a draw with none of them is actionable (so the refusal
    /// list above is the mapper's complete list for these inputs).
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
