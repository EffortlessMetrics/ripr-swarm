//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).

use super::owner_pin::{AssertionRefusal, OwnerPinSyntax};
use crate::analysis::classify::{ProbeContext, direct_call_name};
use crate::analysis::facts::{SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind};
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::domain::{RelatedTest, RelatedTestMiss};
use std::collections::BTreeSet;
use std::path::Path;

pub(in crate::analysis) fn assertion_refusal_counts_as_analyzer_limit(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> bool {
    let owner_name = context.owner_fn.map_or("", |owner| owner.name.as_str());
    (refusal.is_analyzer_limit()
        || (refusal.is_gated_item() && test_shares_owner_cfg_gates(context, test))
        || context.owner_fn.is_some_and(|owner| {
            owner_compiled_only_under_refused_declaration(context.index, owner, test, refusal)
        }))
        && refusal
            .value_guard_subject()
            .is_none_or(|subject| direct_call_name(subject) == Some(owner_name))
}

fn test_shares_owner_cfg_gates(context: &ProbeContext<'_>, test: &TestSummary) -> bool {
    let Some(owner) = context.owner_fn else {
        return false;
    };
    let gates = |file: &Path, name: &str, line: usize| {
        let facts = context.index.files().get(file)?;
        crate::analysis::syntax::unestablished_cfg_gates_of_fn(&facts.source, name, line)
    };
    let (Some(test_gates), Some(owner_gates)) = (
        gates(&test.file, &test.name, test.start_line),
        gates(&owner.file, &owner.name, owner.start_line),
    ) else {
        return false;
    };
    !test_gates.is_empty() && test_gates.is_subset(&owner_gates)
}

/// #14463: the test's file is refused because an out-of-line `mod name;`
/// declaration on its module chain carries a gate ripr cannot evaluate
/// (`#[cfg(feature = "x")] mod x;`). When the changed owner's file is reached
/// only through that same declaration, the owner is compiled exactly when the
/// test is, so the gate cannot keep the test from checking the owner: the
/// refusal is a limit of ripr's reading, as #9643 decided for an inline gated
/// module. Every gate on the test's side must be one the owner also carries:
/// - the owner's module ancestry is one linear chain through the refused
///   declaration, so no other path compiles the owner without it;
/// - every declaration on the test's chain is either on the owner's chain and
///   gated only by `cfg`s ripr cannot evaluate (or not gated), or not on it
///   and available in every test build (no attribute, or `cfg(test)`); a
///   provably-off gate such as `cfg(any())` keeps the gap, as it does for the
///   inline form;
/// - the test's in-file `cfg` gates are a subset of the owner's;
/// - the test's own refusal of each assertion, read past the provenance
///   refusal, is none, an analyzer limit, or that shared gate, so an ignored
///   test, an expected panic or a conditional assertion keeps the gap.
///
/// #14537: the predicate family reads this same rule rather than skipping the
/// Module edge unconditionally, so an unknown gate admits the equality only
/// when the owner is reached solely through that declaration.
pub(in crate::analysis) fn owner_compiled_only_under_refused_declaration(
    index: &RustIndex,
    owner: &FunctionSummary,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> bool {
    let Some((parent, line, declaration)) = refusal.module_declaration_site() else {
        return false;
    };
    let (Some(owner_context), Some(test_context)) = (
        index.execution_context(&owner.file),
        index.execution_context(&test.file),
    ) else {
        return false;
    };
    let owner_edges = owner_context
        .edges
        .iter()
        .map(edge_key)
        .collect::<BTreeSet<_>>();
    let children = owner_context
        .edges
        .iter()
        .map(|edge| edge.child.as_path())
        .collect::<BTreeSet<_>>();
    if owner_context.roots.len() != 1
        || children.len() != owner_context.edges.len()
        || !owner_edges.contains(&(parent, line, declaration))
    {
        return false;
    }
    let gate_of = |edge: &SourceRoleProvenanceEdge| {
        (edge.kind == SourceRoleProvenanceEdgeKind::Module)
            .then(|| index.files().get(&edge.parent))
            .flatten()
            .filter(|facts| !facts.used_lexical_fallback)
            .and_then(|facts| {
                crate::analysis::syntax::module_declaration_unestablished_gate(
                    &facts.source,
                    edge.line,
                    &edge.declaration,
                )
            })
    };
    let chain_admitted = test_context.edges.iter().all(|edge| match gate_of(edge) {
        // Shared with the owner: any unevaluated gate binds both.
        Some(_) if owner_edges.contains(&edge_key(edge)) => true,
        // The test's own declaration must be on in every test build.
        Some(false) => true,
        _ => false,
    });
    let refused_is_unknown_gate = test_context
        .edges
        .iter()
        .find(|edge| edge_key(edge) == (parent, line, declaration))
        .and_then(gate_of)
        == Some(true);
    let in_file_gates = |file: &Path, name: &str, line: usize| {
        let facts = index.files().get(file)?;
        crate::analysis::syntax::unestablished_cfg_gates_of_fn(&facts.source, name, line)
    };
    let in_file_shared = match (
        in_file_gates(&test.file, &test.name, test.start_line),
        in_file_gates(&owner.file, &owner.name, owner.start_line),
    ) {
        (Some(test_gates), Some(owner_gates)) => test_gates.is_subset(&owner_gates),
        _ => false,
    };
    // The shared declaration explains only the provenance refusal: every
    // assertion's own in-file reading must be admitted, an analyzer limit, or
    // the enclosing gate the subset check above shares with the owner.
    let test_local_admitted =
        AssertionRefusal::test_local_refusals(test, index).is_some_and(|refusals| {
            !refusals.is_empty()
                && refusals.iter().all(|refusal| {
                    refusal.as_ref().is_none_or(|refusal| {
                        refusal.is_analyzer_limit() || refusal.is_gated_item()
                    })
                })
        });
    refused_is_unknown_gate && chain_admitted && in_file_shared && test_local_admitted
}

/// A module edge's identity: declaring file, line and declaration text.
fn edge_key(edge: &SourceRoleProvenanceEdge) -> (&Path, usize, &str) {
    (edge.parent.as_path(), edge.line, edge.declaration.as_str())
}

pub(in crate::analysis) fn related_test_miss_for_assertion_refusal(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> RelatedTestMiss {
    if assertion_refusal_counts_as_analyzer_limit(context, test, refusal) {
        RelatedTestMiss::AssertionAdmissionUnproven
    } else {
        RelatedTestMiss::AssertionInert
    }
}

/// Order-independent: agreeing refusals keep their miss, any disagreement is
/// admission-unproven (commutative and associative over the two values).
fn combine_refusal_misses(
    prior: Option<RelatedTestMiss>,
    next: RelatedTestMiss,
) -> RelatedTestMiss {
    match prior {
        Some(prior) if prior != next => RelatedTestMiss::AssertionAdmissionUnproven,
        _ => next,
    }
}

pub(in crate::analysis) fn split_assertion_not_credited_misses(
    context: &ProbeContext<'_>,
    pin_syntax: &OwnerPinSyntax,
    related_tests: &mut [RelatedTest],
) {
    for row in related_tests.iter_mut() {
        if row.miss != Some(RelatedTestMiss::AssertionNotCredited) {
            continue;
        }
        let Some((test, _)) = context.related_tests.iter().find(|(test, _)| {
            test.name == row.name && test.file == row.file && test.start_line == row.line
        }) else {
            row.miss = Some(RelatedTestMiss::AssertionAdmissionUnproven);
            continue;
        };
        let probe = context.probe;
        // Reveal keeps no pairing between a row and the refused assertion, so
        // the row cannot name the responsible one. Read every refusal and keep
        // `AssertionInert` only when all of them agree; any disagreement or
        // unproven refusal is admission-unproven, independent of source order.
        let mut split: Option<RelatedTestMiss> = None;
        for assertion in &test.assertions {
            let Some(refusal) = pin_syntax.equality_assertion_refusal(
                probe,
                context.owner_fn,
                test,
                assertion,
                context.index,
            ) else {
                continue;
            };
            let miss = related_test_miss_for_assertion_refusal(context, test, &refusal);
            split = Some(combine_refusal_misses(split, miss));
        }
        row.miss = Some(split.unwrap_or(RelatedTestMiss::AssertionAdmissionUnproven));
    }
}

/// #9981 forwarding residual: a row read as `no_assertion` whose test calls a
/// same-file helper that asserts after forwarding the test's closure to a call
/// ripr cannot resolve (tracing's `with_default(subscriber, fun)`) does have
/// an assertion; whether it observes the closure is what is unknown. Such a
/// row becomes the analyzer-limit miss `assertion_admission_unproven`
/// (RIPR-SPEC-0221) and the finding names the helper and callee. It grants no
/// oracle, strength or class, so no class can rise from it; only the false
/// absence claim (and its "add an assertion" repair) is withdrawn.
///
/// A `#[quickcheck]` property returning `bool`/`TestResult`
/// ([`is_returned_quickcheck_property`], #6643) is the same false absence:
/// its returned value is the harness check. It is rewritten the same way.
///
/// #15237: a `no_assertion` row whose test body invokes an assertion-like
/// macro ripr does not classify (`assert_delta!`, read with the #7089
/// witness's [`first_unresolved_assertion_like_macro`] predicate) has a
/// check ripr cannot read, not a missing one. It is rewritten the same way
/// and named in one evidence line, but it never counts toward
/// `only_limits`, so the finding-level summaries do not move.
///
/// [`first_unresolved_assertion_like_macro`]: crate::analysis::language::first_unresolved_assertion_like_macro
///
/// [`ForwardedCallbackLimits::only_limits`] is true when at least one row was
/// rewritten, no `no_assertion` row is left, no row carries an oracle, and
/// every oracle-crediting related test, including any the display window
/// omits, is a forwarded-limit test with no assertion of its own: the
/// only assertions the related tests hold are the forwarded helpers'. Only
/// then may the caller replace the "no assertion" stage summaries with the
/// existing refused-context ones (RIPR-SPEC-0221), so the finding-level next
/// step stops prescribing an assertion the test already has.
pub(in crate::analysis) fn name_forwarded_callback_limits(
    context: &ProbeContext<'_>,
    related_tests: &mut [RelatedTest],
) -> ForwardedCallbackLimits {
    let mut named = ForwardedCallbackLimits::default();
    // Recorded by this producer loop as it rewrites, so the `only_limits`
    // decision below reads these facts rather than re-reading the miss.
    let mut rows_clear = true;
    for row in related_tests.iter_mut() {
        if row.oracle.is_some() {
            rows_clear = false;
            continue;
        }
        if row.miss != Some(RelatedTestMiss::NoAssertion) {
            continue;
        }
        let row_test = context
            .related_tests
            .iter()
            .map(|(test, _)| *test)
            .find(|test| {
                test.file == row.file && test.start_line == row.line && test.name == row.name
            });
        let line = if let Some(limit) = context
            .index
            .forwarded_callback_limit(&row.file, row.line, &row.name)
        {
            format!(
                "{FORWARDED_CALLBACK_PREFIX}test `{}` calls helper `{}`, which asserts after forwarding the closure to `{}`; whether it runs the closure is not established statically",
                row.name, limit.helper, limit.callee
            )
        } else if row_test.is_some_and(is_returned_quickcheck_property) {
            format!(
                "{QUICKCHECK_PROPERTY_PREFIX}`{}` reports failure through the value it returns; ripr does not read that returned check",
                row.name
            )
        } else if let Some(macro_name) =
            row_test.and_then(|test| unclassified_assertion_macro_of(context.index, test))
        {
            // #15237: the row is rewritten, but the test's check is not a
            // forwarded or returned one, so the stage summaries keep their
            // no-assertion wording (`only_limits` is not widened).
            rows_clear = false;
            unclassified_assertion_macro_line(&row.name, &macro_name)
        } else {
            rows_clear = false;
            continue;
        };
        row.miss = Some(RelatedTestMiss::AssertionAdmissionUnproven);
        if !named.evidence.contains(&line) {
            named.evidence.push(line);
        }
    }
    // The rows are the capped display window; the stage summaries read every
    // related test. So the decision also reads every related test: each one
    // whose relation may credit an oracle must be a forwarded-limit test with
    // no assertion of its own. A hidden assertion-free or oracle-bearing test
    // anywhere keeps the finding's no-assertion wording.
    let credits = super::oracle_crediting_relations(&context.related_tests);
    named.only_limits = !named.evidence.is_empty()
        && rows_clear
        && context
            .related_tests
            .iter()
            .filter(|(_, reason)| credits(*reason))
            .all(|(test, _)| {
                test.assertions.is_empty()
                    && (context
                        .index
                        .forwarded_callback_limit(&test.file, test.start_line, &test.name)
                        .is_some()
                        || is_returned_quickcheck_property(test))
            });
    named
}

/// #15237: the assertion-like macro ripr does not classify for which
/// [`name_forwarded_callback_limits`] rewrites `test`'s `no_assertion` row,
/// when no forwarded-callback or returned-QuickCheck reading takes the row
/// first. Shared with the #7089 macro-limit gates, which read such a row as
/// the `no_assertion` row it was: the rewrite withdraws a false absence
/// claim; it is not an assertion ripr judged.
pub(in crate::analysis) fn unclassified_assertion_macro_of(
    index: &RustIndex,
    test: &TestSummary,
) -> Option<String> {
    if index
        .forwarded_callback_limit(&test.file, test.start_line, &test.name)
        .is_some()
        || is_returned_quickcheck_property(test)
    {
        return None;
    }
    crate::analysis::language::first_unresolved_assertion_like_macro(&test.body)
}

/// The one evidence line the #15237 rewrite adds for `test_name`.
pub(in crate::analysis) fn unclassified_assertion_macro_line(
    test_name: &str,
    macro_name: &str,
) -> String {
    format!(
        "{UNRESOLVED_ASSERTION_MACRO_PREFIX}test `{test_name}` invokes `{macro_name}!`, an assertion-like macro ripr does not classify; whether it checks the changed behavior is not established statically"
    )
}

/// #6643: a `#[quickcheck]` property returning `bool` or `TestResult` fails
/// through its return value, which the harness checks. With no assertion
/// macro in the body it reads as `no_assertion`, and the finding would tell
/// the reader to add an assertion the property already has. Recognizing the
/// shape grants no oracle: the returned expression stays unread, so only the
/// false absence claim is withdrawn. A same-named local attribute is not
/// distinguished here; that can only withdraw an absence claim, never credit.
pub(in crate::analysis) fn is_returned_quickcheck_property(test: &TestSummary) -> bool {
    let attribute = test.attrs.iter().any(|attr| {
        let compact: String = attr.chars().filter(|c| !c.is_whitespace()).collect();
        let compact = compact
            .strip_prefix("#[")
            .and_then(|rest| rest.strip_suffix(']'));
        matches!(
            compact.map(|path| path.strip_prefix("::").unwrap_or(path)),
            Some("quickcheck" | "quickcheck_macros::quickcheck")
        )
    });
    attribute && returned_type(&test.body).is_some_and(is_property_result_type)
}

/// The return type written in the function header before its body's `{`.
/// The declared return type of the function whose source starts at `body`:
/// the text after the parameter list's closing parenthesis and its `->`, up to
/// a `where` clause or the body. A `->` inside generics, parameters or a
/// `where` bound is never read as the return type.
fn returned_type(body: &str) -> Option<&str> {
    let mut angle = 0usize;
    let mut paren = 0usize;
    let mut previous = ' ';
    let mut params_end = None;
    for (index, c) in body.char_indices() {
        match c {
            '<' if paren == 0 => angle += 1,
            '>' if paren == 0 && previous != '-' => angle = angle.saturating_sub(1),
            '(' if angle == 0 => paren += 1,
            ')' if angle == 0 && paren > 0 => {
                paren -= 1;
                if paren == 0 {
                    params_end = Some(index + 1);
                    break;
                }
            }
            '{' | ';' if angle == 0 && paren == 0 => return None,
            _ => {}
        }
        previous = c;
    }
    let rest = body[params_end?..].trim_start().strip_prefix("->")?;
    let end = rest.find('{').unwrap_or(rest.len());
    let returned = &rest[..end];
    let cut = returned
        .match_indices("where")
        .find(|(at, _)| {
            let before = returned[..*at].chars().next_back();
            let after = returned[at + "where".len()..].chars().next();
            before.is_some_and(char::is_whitespace) && after.is_none_or(char::is_whitespace)
        })
        .map_or(returned.len(), |(at, _)| at);
    let returned = returned[..cut].trim();
    (!returned.is_empty()).then_some(returned)
}

fn is_property_result_type(returned: &str) -> bool {
    let compact: String = returned.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.strip_prefix("::").unwrap_or(&compact),
        "bool" | "TestResult" | "quickcheck::TestResult"
    )
}

/// What [`name_forwarded_callback_limits`] rewrote.
#[derive(Default)]
pub(in crate::analysis) struct ForwardedCallbackLimits {
    /// One evidence line per rewritten test, naming the helper and callee.
    pub(in crate::analysis) evidence: Vec<String>,
    /// Every assertion the related tests hold is a forwarded helper's.
    pub(in crate::analysis) only_limits: bool,
}

/// Evidence prefix of a [`name_forwarded_callback_limits`] line.
pub(in crate::analysis) const FORWARDED_CALLBACK_PREFIX: &str = "closure forwarded by helper: ";

/// Evidence prefix of a returned QuickCheck property line (#6643).
pub(in crate::analysis) const QUICKCHECK_PROPERTY_PREFIX: &str =
    "QuickCheck property returns its check: ";

/// Evidence prefix of an unclassified assertion-like macro line (#15237).
pub(in crate::analysis) const UNRESOLVED_ASSERTION_MACRO_PREFIX: &str =
    "assertion-like macro not classified: ";

#[cfg(test)]
mod combine_tests {
    use super::combine_refusal_misses;
    use crate::domain::RelatedTestMiss::{AssertionAdmissionUnproven, AssertionInert};

    #[test]
    fn mixed_refusals_are_unproven_in_either_order() {
        let fold = |misses: &[_]| {
            misses
                .iter()
                .fold(None, |acc, miss| Some(combine_refusal_misses(acc, *miss)))
        };
        assert_eq!(
            fold(&[AssertionInert, AssertionAdmissionUnproven]),
            Some(AssertionAdmissionUnproven)
        );
        assert_eq!(
            fold(&[AssertionAdmissionUnproven, AssertionInert]),
            Some(AssertionAdmissionUnproven)
        );
        assert_eq!(
            fold(&[AssertionInert, AssertionInert]),
            Some(AssertionInert)
        );
    }
}

#[cfg(test)]
mod returned_type_tests {
    use super::{is_property_result_type, returned_type};

    fn property(body: &str) -> bool {
        returned_type(body).is_some_and(is_property_result_type)
    }

    #[test]
    fn a_declared_bool_or_test_result_is_a_property_result() {
        for body in [
            "fn holds(x: u32) -> bool {\n    true\n}",
            "fn holds(x: u32) -> TestResult { TestResult::passed() }",
            "fn holds(x: u32) -> quickcheck::TestResult { TestResult::passed() }",
            "fn holds<T: Clone>(x: T) -> bool where T: Copy { true }",
            "fn holds<T>(x: T) -> bool\nwhere\n    T: Copy,\n{ true }",
            "fn holds(f: fn(u32) -> u32) -> bool { true }",
        ] {
            assert!(property(body), "{body}");
        }
    }

    #[test]
    fn an_arrow_outside_the_return_type_is_not_read_as_one() {
        for body in [
            "fn p<F>(f: F) where F: Fn() -> bool { f(); }",
            "fn p<F: Fn() -> bool>(f: F) { f(); }",
            "fn p(f: impl Fn(u32) -> bool) { f(1); }",
            "fn p(x: u32) { let _ = x; }",
            "fn p(x: u32) -> u32 { x }",
            "fn p(x: u32) -> boolean { x }",
        ] {
            assert!(!property(body), "{body}");
        }
    }
}

/// #15237: a `no_assertion` row whose test invokes an assertion-like macro
/// ripr does not classify reads `assertion_admission_unproven`, named in one
/// evidence line, with no oracle or `only_limits` change.
#[cfg(test)]
mod unresolved_assertion_macro_tests {
    use super::{UNRESOLVED_ASSERTION_MACRO_PREFIX, name_forwarded_callback_limits};
    use crate::analysis::classify::ProbeContext;
    use crate::analysis::facts::RustIndex;
    use crate::domain::{
        DeltaKind, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest,
        RelatedTestMiss, RelationConfidence, RelationReason, SourceLocation,
    };
    use std::path::PathBuf;

    /// The strsim shape: a test-module `macro_rules!` whose body is a
    /// hand-rolled `if .. { panic!(..) }` check.
    const ASSERT_DELTA: &str = "    macro_rules! assert_delta {\n        ($x:expr, $y:expr, $d:expr) => {\n            if ($x - $y).abs() > $d {\n                panic!(\"not within delta\");\n            }\n        };\n    }\n";

    fn source(test_body: &str) -> String {
        format!(
            "pub fn bump(x: f64) -> f64 {{\n    x + 2.0\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{ASSERT_DELTA}\n    #[test]\n    fn holds() {{\n{test_body}\n    }}\n}}\n"
        )
    }

    /// The `holds` row after the rewrite, with the evidence and `only_limits`.
    fn rewrite(test_body: &str) -> Result<(RelatedTest, Vec<String>, bool), String> {
        let source = source(test_body);
        let path = PathBuf::from("src/lib.rs");
        let facts = crate::analysis::syntax::ra::summarize_file_with_parser(&path, &source)?;
        let mut index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: facts.functions.clone(),
            tests: facts.tests.clone(),
            files: [(path, facts)].into_iter().collect(),
            ..Default::default()
        });
        crate::analysis::facts::compose_fixture_source_roles(&mut index, std::path::Path::new(""));
        let owner = index
            .functions()
            .iter()
            .find(|function| function.name == "bump")
            .ok_or("premise: owner `bump` parses")?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "holds")
            .ok_or("premise: test `holds` parses")?;
        // Fixture construction: the parsed test body is the intended subject.
        if !test.body.contains(test_body.trim()) {
            return Err(format!(
                "premise: the parsed body holds the stimulus\n{}",
                &*test.body
            ));
        }
        let probe = Probe {
            id: ProbeId("probe:assert-delta".to_string()),
            location: SourceLocation::new("src/lib.rs", 2, 5),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("x + 1.0".to_string()),
            after: Some("x + 2.0".to_string()),
            expression: "x + 2.0".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let context = ProbeContext::new(
            &probe,
            Some(owner),
            vec![(test, RelationReason::DirectOwnerCall)],
            false,
            &index,
            true,
        );
        let mut rows = vec![RelatedTest {
            name: test.name.clone(),
            file: test.file.clone(),
            line: test.start_line,
            oracle: None,
            oracle_kind: OracleKind::SmokeOnly,
            oracle_strength: OracleStrength::None,
            relation_reason: Some(RelationReason::DirectOwnerCall),
            relation_confidence: Some(RelationConfidence::High),
            miss: Some(RelatedTestMiss::NoAssertion),
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }];
        let before = rows[0].clone();
        let named = name_forwarded_callback_limits(&context, &mut rows);
        let [row] = <[RelatedTest; 1]>::try_from(rows)
            .map_err(|rows| format!("expected one row, got {}", rows.len()))?;
        // Nothing but the miss may move: no oracle, kind, strength or relation.
        let mut unchanged = row.clone();
        unchanged.miss = before.miss;
        if unchanged != before {
            return Err(format!("only the miss may change: {before:?} -> {row:?}"));
        }
        Ok((row, named.evidence, named.only_limits))
    }

    #[test]
    fn an_unclassified_assertion_macro_withdraws_the_absence_claim() -> Result<(), String> {
        for (body, macro_name) in [
            (
                "        assert_delta!(3.0, bump(1.0), 0.001);",
                "assert_delta",
            ),
            (
                "        let _ = 1;\n        crate::assert_close!(bump(1.0), 3.0);",
                "crate::assert_close",
            ),
            // The first unclassified macro by line names the limit.
            (
                "        assert_within!(bump(0.0), 2.0);\n        assert_delta!(3.0, bump(1.0), 0.001);",
                "assert_within",
            ),
        ] {
            let (row, evidence, only_limits) = rewrite(body)?;
            assert_eq!(
                row.miss,
                Some(RelatedTestMiss::AssertionAdmissionUnproven),
                "{body}"
            );
            assert_eq!(
                evidence,
                [format!(
                    "{UNRESOLVED_ASSERTION_MACRO_PREFIX}test `holds` invokes `{macro_name}!`, an assertion-like macro ripr does not classify; whether it checks the changed behavior is not established statically"
                )],
                "{body}"
            );
            assert!(!only_limits, "only_limits is not widened: {body}");
        }
        Ok(())
    }

    #[test]
    fn other_bodies_keep_no_assertion() -> Result<(), String> {
        for body in [
            // No macro at all.
            "        let _ = bump(1.0);",
            // Non-assertion macros.
            "        println!(\"{}\", bump(1.0));",
            "        let _values = vec![bump(1.0)];",
            // The macro only in a comment or a string.
            "        // assert_delta!(3.0, bump(1.0), 0.001);\n        let _ = bump(1.0);",
            "        /* assert_delta!(3.0, bump(1.0), 0.001); */\n        let _ = bump(1.0);",
            "        let _text = \"assert_delta!(3.0, bump(1.0), 0.001)\";\n        let _ = bump(1.0);",
            // std assertion macros are classified (as oracles on the real
            // path), so they never name this limit.
            "        assert_eq!(bump(1.0), 3.0);",
            "        assert!(bump(1.0) > 2.0);",
            // Not `assert` or `assert_*`.
            "        debug_assert_eq!(bump(1.0), 3.0);",
            "        assertion!(bump(1.0));",
        ] {
            let (row, evidence, only_limits) = rewrite(body)?;
            assert_eq!(row.miss, Some(RelatedTestMiss::NoAssertion), "{body}");
            assert!(evidence.is_empty(), "{body}: {evidence:?}");
            assert!(!only_limits, "{body}");
        }
        Ok(())
    }
}
