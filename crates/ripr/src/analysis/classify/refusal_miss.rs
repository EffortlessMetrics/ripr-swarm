//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).

use super::owner_pin::{AssertionRefusal, OwnerPinSyntax};
use crate::analysis::classify::{ProbeContext, direct_call_name};
use crate::analysis::facts::{SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind};
use crate::analysis::resource_cost::trace_latency_phase;
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::domain::{RelatedTest, RelatedTestMiss};
use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

type TestKey = (PathBuf, String, usize);

#[derive(Default)]
pub(in crate::analysis) struct RefusalMemo {
    local_refusals: RefCell<BTreeMap<TestKey, Option<Vec<Option<AssertionRefusal>>>>>,
    test_cfg_gates: RefCell<BTreeMap<TestKey, Option<BTreeSet<String>>>>,
    owner_cfg_gates: OnceCell<Option<BTreeSet<String>>>,
    module_gates: RefCell<BTreeMap<(PathBuf, usize, String), Option<bool>>>,
}

pub(in crate::analysis) fn assertion_refusal_counts_as_analyzer_limit(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
) -> bool {
    assertion_refusal_counts_as_analyzer_limit_with_local(
        context,
        test,
        refusal,
        &context.refusal_memo,
    )
}

fn assertion_refusal_counts_as_analyzer_limit_with_local(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
    local: &RefusalMemo,
) -> bool {
    let owner_name = context.owner_fn.map_or("", |owner| owner.name.as_str());
    (refusal.is_analyzer_limit()
        || (refusal.is_gated_item() && test_shares_owner_cfg_gates(context, test, local))
        || context.owner_fn.is_some_and(|owner| {
            owner_compiled_only_under_refused_declaration_with_local(
                context.index,
                owner,
                test,
                refusal,
                local,
            )
        }))
        && refusal
            .value_guard_subject()
            .is_none_or(|subject| direct_call_name(subject) == Some(owner_name))
}

fn cfg_gates(
    index: &RustIndex,
    owner: &FunctionSummary,
    test: &TestSummary,
    local: &RefusalMemo,
) -> (Option<BTreeSet<String>>, Option<BTreeSet<String>>) {
    let test_gates = local
        .test_cfg_gates
        .borrow_mut()
        .entry((test.file.clone(), test.name.clone(), test.start_line))
        .or_insert_with(|| {
            let facts = index.files().get(&test.file)?;
            crate::analysis::syntax::unestablished_cfg_gates_of_fn(
                &facts.source,
                &test.name,
                test.start_line,
            )
        })
        .clone();
    let owner_gates = local
        .owner_cfg_gates
        .get_or_init(|| {
            let facts = index.files().get(&owner.file)?;
            crate::analysis::syntax::unestablished_cfg_gates_of_fn(
                &facts.source,
                &owner.name,
                owner.start_line,
            )
        })
        .clone();
    (test_gates, owner_gates)
}

fn test_shares_owner_cfg_gates(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    local: &RefusalMemo,
) -> bool {
    let Some(owner) = context.owner_fn else {
        return false;
    };
    let (Some(test_gates), Some(owner_gates)) = cfg_gates(context.index, owner, test, local) else {
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
    owner_compiled_only_under_refused_declaration_with_local(
        index,
        owner,
        test,
        refusal,
        &RefusalMemo::default(),
    )
}

fn owner_compiled_only_under_refused_declaration_with_local(
    index: &RustIndex,
    owner: &FunctionSummary,
    test: &TestSummary,
    refusal: &AssertionRefusal,
    local: &RefusalMemo,
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
    let gate_of = |edge: &SourceRoleProvenanceEdge| module_edge_gate(index, local, edge);
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
    let in_file_shared = match cfg_gates(index, owner, test, local) {
        (Some(test_gates), Some(owner_gates)) => test_gates.is_subset(&owner_gates),
        _ => false,
    };
    // The shared declaration explains only the provenance refusal: every
    // assertion's own in-file reading must be admitted, an analyzer limit, or
    // the enclosing gate the subset check above shares with the owner.
    let test_local_admitted = local
        .local_refusals
        .borrow_mut()
        .entry((test.file.clone(), test.name.clone(), test.start_line))
        .or_insert_with(|| AssertionRefusal::test_local_refusals(test, index))
        .as_ref()
        .is_some_and(|refusals| {
            !refusals.is_empty()
                && refusals.iter().all(|refusal| {
                    refusal.as_ref().is_none_or(|refusal| {
                        refusal.is_analyzer_limit() || refusal.is_gated_item()
                    })
                })
        });
    refused_is_unknown_gate && chain_admitted && in_file_shared && test_local_admitted
}

/// The memoized unestablished-gate reading of a module edge's declaration.
/// The memo key is the full edge identity: two `mod` declarations on one line
/// are distinct declarations with distinct gates.
fn module_edge_gate(
    index: &RustIndex,
    local: &RefusalMemo,
    edge: &SourceRoleProvenanceEdge,
) -> Option<bool> {
    if edge.kind != SourceRoleProvenanceEdgeKind::Module {
        return None;
    }
    let key = (edge.parent.clone(), edge.line, edge.declaration.clone());
    local
        .module_gates
        .borrow_mut()
        .entry(key)
        .or_insert_with(|| {
            index
                .files()
                .get(&edge.parent)
                .filter(|facts| !facts.used_lexical_fallback)
                .and_then(|facts| {
                    crate::analysis::syntax::module_declaration_unestablished_gate(
                        &facts.source,
                        edge.line,
                        &edge.declaration,
                    )
                })
        })
        .to_owned()
}

/// A module edge's identity: declaring file, line and declaration text.
fn edge_key(edge: &SourceRoleProvenanceEdge) -> (&Path, usize, &str) {
    (edge.parent.as_path(), edge.line, edge.declaration.as_str())
}

fn related_test_miss_for_assertion_refusal_with_local(
    context: &ProbeContext<'_>,
    test: &TestSummary,
    refusal: &AssertionRefusal,
    local: &RefusalMemo,
) -> RelatedTestMiss {
    if assertion_refusal_counts_as_analyzer_limit_with_local(context, test, refusal, local) {
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
    let started = Instant::now();
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
            let miss = related_test_miss_for_assertion_refusal_with_local(
                context,
                test,
                &refusal,
                &context.refusal_memo,
            );
            split = Some(combine_refusal_misses(split, miss));
        }
        row.miss = Some(split.unwrap_or(RelatedTestMiss::AssertionAdmissionUnproven));
    }
    trace_latency_phase(
        "classify_assertion_not_credited_misses",
        "ok",
        started.elapsed(),
    );
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
        let line = if let Some(limit) = context
            .index
            .forwarded_callback_limit(&row.file, row.line, &row.name)
        {
            format!(
                "{FORWARDED_CALLBACK_PREFIX}test `{}` calls helper `{}`, which asserts after forwarding the closure to `{}`; whether it runs the closure is not established statically",
                row.name, limit.helper, limit.callee
            )
        } else if context.related_tests.iter().any(|(test, _)| {
            test.file == row.file
                && test.start_line == row.line
                && test.name == row.name
                && is_returned_quickcheck_property(test)
        }) {
            format!(
                "{QUICKCHECK_PROPERTY_PREFIX}`{}` reports failure through the value it returns; ripr does not read that returned check",
                row.name
            )
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

#[cfg(test)]
mod module_gate_memo_tests {
    use super::{RefusalMemo, module_edge_gate};
    use crate::analysis::facts::{
        FileFacts, SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind,
    };
    use crate::analysis::rust_index::RustIndex;
    use std::path::PathBuf;

    fn edge(declaration: &str) -> SourceRoleProvenanceEdge {
        SourceRoleProvenanceEdge {
            kind: SourceRoleProvenanceEdgeKind::Module,
            parent: PathBuf::from("lib.rs"),
            child: PathBuf::from("child.rs"),
            declaration: declaration.to_string(),
            line: 1,
            requires_test: false,
        }
    }

    #[test]
    fn same_line_sibling_modules_do_not_share_a_memoized_gate() {
        let source = "#[cfg(feature = \"v4\")] mod v4; mod helpers;\n";
        let mut index = RustIndex::default();
        index.insert_file_only(
            PathBuf::from("lib.rs"),
            FileFacts {
                path: PathBuf::from("lib.rs"),
                source: source.into(),
                ..FileFacts::default()
            },
        );
        let memo = RefusalMemo::default();
        // Gated sibling first, so a line-only key would poison the second.
        assert_eq!(
            module_edge_gate(&index, &memo, &edge("mod v4;")),
            Some(true)
        );
        assert_eq!(
            module_edge_gate(&index, &memo, &edge("mod helpers;")),
            Some(false)
        );
    }
}
