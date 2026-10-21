use crate::analysis::classify::{
    ASSERTION_CONTEXT_UNESTABLISHED, ArmSelector, AssertionRefusal, EffectStateCarrier,
    HELPER_RESULT_NOT_FORWARDED, OwnerPinSyntax, OwnerReturnPin, ProbeContext,
    PropagationWitnessV1, ReturnOracleAdmission, TransitiveReachIndex, WrapperEntryPairing,
    activation_and_boundary_input, assertion_observes_collection_argument,
    body_contains_owner_call, call_parameter_names, callee_is_unique,
    chain_forwards_to_observed_hops, chain_passes_effect_target_to_observed_hops, classify,
    confidence_score, constructor_starts_empty, contains_as_whole_word, current_path_witness_with,
    direct_call_name, direct_collection_mutation_receiver, expected_operand_value,
    fallible_check_receipt, fallible_literal_witness, function_returns_value, getter_plainly_reads,
    getter_plainly_reads_key, has_same_test_boundary_oracle_pairing, helper_only_reach,
    helper_pins_owner_call, infection_evidence_for_owner, local_flow_sinks, method_cannot_store,
    observes_returned_element_field, operand_only_pin, oracle_crediting_relations,
    owner_touches_field_only_at, package_prefix, parameter_rebound,
    propagation_evidence_with_witness, reach_evidence, reveal_outcome,
    same_test_pairing_missing_summary, setter_plainly_writes, signature_parameters,
    unconditional_owner_insert_field,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::sync_memo::SyncMemo;
use crate::domain::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod side_flip;
mod tuple_match;

pub(super) const PROPAGATION_BOUNDARY_UNVERIFIED: &str =
    "propagation_boundary_activation_unestablished";

pub(in crate::analysis) struct ClassifiedProbeEvidence {
    pub(in crate::analysis) ripr: RiprEvidence,
    pub(in crate::analysis) evidence: Vec<String>,
    pub(in crate::analysis) flow_sinks: Vec<FlowSinkFact>,
    /// Retained diagnostic witness outcome for the PR-A migration slice.  It
    /// is not projected into `Finding` or used to strengthen a stage yet.
    pub(in crate::analysis) propagation_witness: Option<PropagationWitnessDiagnostic>,
    pub(in crate::analysis) activation: ActivationEvidence,
    pub(in crate::analysis) related_tests: Vec<RelatedTest>,
    pub(in crate::analysis) related_tests_matched_total: usize,
    pub(in crate::analysis) reach: StageEvidence,
    pub(in crate::analysis) infect: StageEvidence,
    pub(in crate::analysis) propagate: StageEvidence,
    pub(in crate::analysis) observe: StageEvidence,
    pub(in crate::analysis) discriminate: StageEvidence,
    /// Reach is `No` for a resolved owner that nothing in the workspace
    /// names (see `owner_may_be_reached_unseen`), so no test can run the
    /// change. An owner with an unresolved caller chain keeps its
    /// shape-based class even when no related test was found.
    pub(in crate::analysis) reach_ruled_out: bool,
    /// When the reveal is not fully established: where a refused related
    /// assertion is, why it was refused, and whether its text calls the
    /// changed owner. Only an owner-calling refusal could have observed
    /// the change had it been credited, so only that one may be presented
    /// as a possible static limit.
    pub(in crate::analysis) assertion_refusal: Option<AssertionRefusalNote>,
    /// When Observe is `rust_assertion_context_unestablished`: every refused
    /// related `assert_eq!` was refused for an analyzer limit, so the gap
    /// rests on what ripr could not read (RIPR-SPEC-0240).
    pub(in crate::analysis) refusals_are_analyzer_limits: bool,
    /// #8051: related direct-call tests (labelled `name` (file:line)) whose
    /// bare `debug_assert_eq!` the owner-return pin would admit as
    /// `assert_eq!`. Never credit; the gap-admission post-pass reads it.
    pub(in crate::analysis) debug_assert_eq_pins: Vec<String>,
    /// #8600: some related test (before the render cap trims the rows) has a
    /// line at the per-line call-fact budget, so a call there may be missing.
    pub(in crate::analysis) related_call_budget_saturated: bool,
}

#[derive(Clone, Debug)]
pub(in crate::analysis) struct AssertionRefusalNote {
    pub(in crate::analysis) location: String,
    pub(in crate::analysis) reason: String,
    pub(in crate::analysis) calls_owner: bool,
    /// Whether the refusal is a limit of ripr's own reading rather than a
    /// shape that can keep the assertion from running (RIPR-SPEC-0240).
    pub(in crate::analysis) is_analyzer_limit: bool,
}

fn return_ledger_limit(
    owner: &str,
    ledger: &crate::analysis::classify::ReturnedLedger,
) -> StageEvidence {
    StageEvidence::new(
        StageState::Unknown,
        Confidence::Low,
        format!(
            "Propagation unknown: `{owner}` parameter `{}` mutably borrows local `{}` from `{}`, which returns that same local ledger; transfer to the exact indexed field observer is unestablished (returned_mutable_ledger_observer_unresolved)",
            ledger.parameter, ledger.local, ledger.caller.0
        ),
    )
}

impl ClassifiedProbeEvidence {
    pub(in crate::analysis) fn gather(
        context: &ProbeContext<'_>,
        reveal_expression: &str,
        proximity_reach: &TransitiveReachIndex<'_>,
    ) -> Self {
        let test_summaries = context.related_test_summaries();
        let fallback_pin_syntax = OwnerPinSyntax::default();
        let pin_syntax = context.owner_pin_syntax.unwrap_or(&fallback_pin_syntax);
        let fallible_checks = context
            .owner_fn
            .map(|owner| {
                context
                    .related_tests
                    .iter()
                    .filter_map(|(test, _)| {
                        fallible_check_receipt(
                            context.probe,
                            owner,
                            test,
                            context.index,
                            pin_syntax,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Without a resolved owner (a line in a `macro_rules!` template, an
        // impl the index did not attribute) there is no name to search for,
        // so nothing rules reach out.
        let reach = reach_evidence(&context.related_tests, context.owner_fn, || {
            context
                .owner_fn
                .is_none_or(|owner| context.owner_may_be_reached_unseen(owner))
        });
        let flow_sinks = local_flow_sinks(context.probe, context.owner_fn);
        let returned_ledger = match (context.owner_fn, context.helper_chain.as_ref()) {
            (Some(owner), Some(chain)) if helper_only_reach(&context.related_tests) => {
                context.index.files().get(&owner.file).and_then(|file| {
                    crate::analysis::classify::returned_ledger(
                        owner,
                        context.probe,
                        chain,
                        &file.source,
                    )
                })
            }
            _ => None,
        };
        let propagation_witness = current_path_witness_with(context.probe, &flow_sinks, &|path| {
            context.owner_fn.is_some_and(|owner| {
                crate::analysis::classify::owner_builds_declared_error_variant(
                    path,
                    owner,
                    context.index,
                )
            })
        })
        .or_else(|| {
            fallible_checks
                .iter()
                .find_map(|receipt| fallible_literal_witness(context.probe, &flow_sinks, receipt))
        })
        .or_else(|| {
            context.owner_fn.and_then(|owner| {
                crate::analysis::classify::constant_tail_witness(context.probe, owner, &flow_sinks)
            })
        })
        .or_else(|| {
            context.owner_fn.and_then(|owner| {
                crate::analysis::classify::format_tail_witness(
                    context.probe,
                    owner,
                    context
                        .index
                        .files()
                        .get(&owner.file)
                        .map(|facts| facts.data().source.as_ref()),
                    &flow_sinks,
                )
            })
        })
        .or_else(|| {
            returned_ledger.as_ref().and_then(|_| {
                crate::analysis::classify::returned_ledger_witness(context.probe, &flow_sinks)
            })
        })
        .map(|mut witness| {
            if let (Some(owner), Some(ledger)) = (context.owner_fn, returned_ledger.as_ref()) {
                crate::analysis::classify::retain_returned_ledger(
                    &mut witness,
                    &owner.id,
                    &ledger.parameter,
                    &ledger.caller,
                    &ledger.local,
                );
                if let Some(helper) = context
                    .index
                    .functions()
                    .iter()
                    .find(|function| function.id == ledger.caller)
                {
                    witness.selected_helper_calls =
                        crate::analysis::classify::selected_inherent_calls(
                            helper,
                            context.index,
                            context.workspace_complete,
                        );
                    witness.semantic_digest = witness.compute_semantic_digest();
                }
            }
            PropagationWitnessDiagnostic::from_witness(witness)
        });
        let field_projection = propagation_witness.as_ref().and_then(|diagnostic| {
            let PropagationWitnessDiagnostic::Valid(witness) = diagnostic else {
                return None;
            };
            crate::analysis::classify::field_projection_receipt(context, pin_syntax, witness)
        });
        let propagation_witness = propagation_witness.map(|diagnostic| match diagnostic {
            PropagationWitnessDiagnostic::Valid(mut witness) => {
                witness.field_projection = field_projection.clone();
                witness.semantic_digest = witness.compute_semantic_digest();
                PropagationWitnessDiagnostic::from_witness(witness)
            }
            other => other,
        });
        let mut gathered = activation_and_boundary_input(
            context.probe,
            context.owner_fn,
            &test_summaries,
            &flow_sinks,
            context.helper_chain.as_ref(),
            context.index,
            context.workspace_complete,
            context.test_value_facts,
        );
        crate::analysis::classify::refine_slice_boundary(
            context,
            pin_syntax,
            &mut gathered,
            &flow_sinks,
        );
        let mut activation = gathered.activation;
        // #3731 review (F11, G1): the changed owner's package scope, computed
        // once — the cross-package same-name defeats compare each related
        // test's package against it.
        let owner_package = context
            .owner_fn
            .and_then(|owner| package_prefix(&owner.file));
        // RIPR-SPEC-0229: an unselected-arm discriminator (and a
        // RIPR-SPEC-0263 unentered guard) reads each related test's owner
        // calls as inputs to the changed owner. A test whose
        // file imports a foreign same-named function, or whose own package
        // defines one, may be calling that function instead, so the same
        // identity defeats reveal applies withhold the named arm or guard here.
        if let Some(owner) = context.owner_fn
            && activation
                .missing_discriminators
                .iter()
                .any(|fact| is_unselected_input_reason(&fact.reason))
            && test_summaries.iter().any(|test| {
                let imports_foreign = context.index.files().get(&test.file).is_some_and(|facts| {
                    context.test_file_imports_foreign_callee_name(
                        &test.file,
                        &facts.source,
                        &owner.name,
                    )
                });
                let package_defines = package_prefix(&test.file).is_some_and(|test_package| {
                    owner_package
                        .as_deref()
                        .is_some_and(|owner_package| owner_package != test_package)
                        && context.index.functions().iter().any(|function| {
                            function.name == owner.name
                                && package_prefix(&function.file).as_deref()
                                    == Some(test_package.as_str())
                        })
                });
                imports_foreign || package_defines
            })
        {
            activation
                .missing_discriminators
                .retain(|fact| !is_unselected_input_reason(&fact.reason));
        }
        let infect = infection_evidence_for_owner(
            context.probe,
            &test_summaries,
            &activation,
            gathered.unresolved_boundary.as_deref(),
            context.owner_fn,
            // #7970: a function that is not one of the related tests and
            // calls the owner by name (a wrapper, a helper, a production
            // caller) may feed it inputs the literal call lines do not show.
            // #8462: a production function that holds the owner without a call
            // shape (`let f = owner; f(1, 1)`) or a `macro_rules!` body that
            // expands to a call is invisible to `calls`, so the bare name in
            // either counts as another caller.
            context.owner_fn.is_none_or(|owner| {
                let other_function_holds_owner = context.index.functions().iter().any(|function| {
                    function.id != owner.id
                        && !test_summaries
                            .iter()
                            .any(|test| test.file == function.file && test.name == function.name)
                        && (function.calls.iter().any(|call| call.name == owner.name)
                            || mentions_identifier(function.body.as_str(), &owner.name))
                });
                // Only the `&&`/`||` operand reading consumes this, so other
                // families skip the per-file macro scan.
                other_function_holds_owner
                    || (matches!(context.probe.family, ProbeFamily::Predicate)
                        && context.index.files().values().any(|file| {
                            macro_rules_bodies(&file.source)
                                .iter()
                                .any(|body| mentions_identifier(body, &owner.name))
                        }))
            }),
        );
        // #6796's computed-argument discipline, wrapper edition (#6672,
        // #6694): when the tests reach the owner only through the helper
        // chain and a chain entry argument is a computed expression
        // (`base + 1`), whether the changed boundary is activated at all is
        // unreadable. The predicate lens abstains through activation's
        // unresolved-boundary reason; a non-predicate lens on the same owner
        // reads infection off the same tests' reach, so without this guard it
        // grades the finding as a gap ("tests miss the boundary") that the
        // computed argument makes unreadable — a claim this seam must not
        // make. Infection abstains with the same reason.
        let chain_computed_inputs = match (context.owner_fn, context.helper_chain.as_ref()) {
            (Some(owner), Some(chain)) => {
                let chain_tests = context
                    .related_tests
                    .iter()
                    .map(|(test, _)| *test)
                    .collect::<Vec<_>>();
                !crate::analysis::classify::computed_input_parameters(
                    owner,
                    &owner_parameter_names(owner),
                    &chain_tests,
                    Some(chain),
                )
                .is_empty()
            }
            _ => false,
        };
        let infect = match (context.owner_fn, context.helper_chain.as_ref()) {
            (Some(owner), Some(_))
                if !matches!(context.probe.family, ProbeFamily::Predicate)
                    && chain_computed_inputs
                    && helper_only_reach(&context.related_tests)
                    && matches!(infect.state, StageState::Yes | StageState::Weak) =>
            {
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    format!(
                        "Infection unknown: a related test passes a computed argument for `{}`, so ripr cannot tell whether the changed boundary is activated",
                        owner_parameter_names(owner)
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "the changed input".to_string())
                    ),
                )
            }
            _ => infect,
        };
        let valid_witness = propagation_witness
            .as_ref()
            .and_then(|diagnostic| match diagnostic {
                PropagationWitnessDiagnostic::Valid(witness) => Some(witness),
                PropagationWitnessDiagnostic::InvalidDigest(_) => None,
            });
        let propagate =
            propagation_evidence_with_witness(context.probe, &flow_sinks, valid_witness);
        // #6780 review B2: when the owner is reached only through the
        // RIPR-SPEC-0159 chain (no related test calls it directly), the
        // tests observe a hop caller's result, not the owner's. Unless every
        // hop up to the highest one a test calls directly hands the call's
        // result to its caller's return, the owner's
        // change is not shown to reach that result: propagation is unknown
        // (an abstention), never credit or an actionable gap.
        // #6780 Devin review and round 5: a side effect or deleted call acts
        // on state, not on a returned value. It keeps its owner-local
        // effect-sink propagation only when every observed hop passes the
        // effect's target through from its own parameter (`wrapper(out) {
        // record(out) }`), so the test's caller-owned state is what changes.
        // A fresh temporary or a wrapper-local target is invisible to the
        // test: those abstain like a dropped result (fail closed).
        let effect_family = matches!(
            context.probe.family,
            ProbeFamily::SideEffect | ProbeFamily::CallDeletion
        );
        let propagate = match (context.owner_fn, context.helper_chain.as_ref()) {
            // An already-unknown stage keeps its own reason.
            (Some(owner), Some(chain))
                if matches!(propagate.state, StageState::Yes | StageState::Weak)
                    && helper_only_reach(&context.related_tests)
                    // #6614, #7549: when every oracle on a caller's result
                    // is weak on its own terms, its gap is established
                    // without pairing the caller to the owner; only an
                    // oracle that might pin the owner abstains.
                    && !crate::analysis::classify::hop_reach_keeps_weak_oracle_gap(
                        owner,
                        chain,
                        &context.related_tests,
                        effect_family,
                    )
                    && if effect_family {
                        !chain_passes_effect_target_to_observed_hops(
                            owner,
                            &context.probe.expression,
                            chain,
                            &context.related_tests,
                        )
                    } else {
                        !chain_forwards_to_observed_hops(&owner.name, chain, &context.related_tests)
                            && field_projection.is_none()
                    } =>
            {
                if let Some(ledger) = returned_ledger.as_ref().filter(|ledger| {
                    crate::analysis::classify::returned_ledger_caller_is_observed(
                        ledger,
                        chain,
                        &context.related_tests,
                    )
                }) {
                    return_ledger_limit(&owner.name, ledger)
                } else {
                    let stop = if effect_family {
                        "a caller that does not pass the changed state through from its own parameter"
                    } else {
                        "a caller that does not forward its result unchanged"
                    };
                    StageEvidence::new(
                        StageState::Unknown,
                        Confidence::Low,
                        format!(
                            "Propagation unknown: the related tests reach `{}` only through {stop} ({HELPER_RESULT_NOT_FORWARDED})",
                            owner.name
                        ),
                    )
                }
            }
            _ => propagate,
        };
        // Both defeats below depend only on the test's file (and the probe's
        // constant owner callee), never on the individual test. A
        // high-traffic owner relates to thousands of tests spread over a
        // few files; without memoization every test re-masked and
        // re-scanned its whole file source (profiled: ~80% of a 60 s
        // `ripr check` on a 505-line diff of this repository). The package
        // defeat is memoized per probe because it also depends on the
        // owner's package; the import scan does not, so it uses the
        // run-scoped per-file memo on the context.
        // #4478: the owner-side half of the owner-return pin, established
        // once per probe; `None` keeps every assertion on the token rule.
        // Every diff-classifier consumer of a covered equality uses this same
        // decision. Keep the original TestSummary and assertion cardinality;
        // filtering a clone would manufacture singleton matching fallbacks.
        let sibling_exclusion_admitted = |test: &TestSummary, assertion: &OracleFact| {
            pin_syntax.admits_assertion_context(test, assertion, context.index)
        };
        let owner_return_pin = context
            .owner_fn
            .and_then(|owner| OwnerReturnPin::establish(context.probe, owner, context.index));
        // RIPR-SPEC-0229: which owner-call input selects a changed arm.
        // A same-named function elsewhere (a trait method on another enum
        // with the same variant names) makes a direct call ambiguous; a
        // partial index cannot show the name is unique.
        let arm_selector = context
            .owner_fn
            .filter(|owner| {
                matches!(context.probe.family, ProbeFamily::MatchArm)
                    && context.workspace_complete
                    && callee_is_unique(&owner.name, context.index)
            })
            .and_then(|owner| ArmSelector::establish(context.probe, owner))
            .map(|selector| {
                selector
                    .in_workspace(
                        context.index,
                        context
                            .related_tests
                            .iter()
                            .map(|(test, _)| test.file.as_path()),
                    )
                    // RIPR-SPEC-0229 (#6616): exact pattern pins through a verified
                    // local `assert_matches!`, read once per file per run.
                    .with_pattern_pins(
                        context.related_tests.iter().map(|(test, _)| *test),
                        |test, assertion| {
                            pin_syntax.exact_pattern_pin_value(test, assertion, context.index)
                        },
                    )
                    // RIPR-SPEC-0229 decision 5: a check helper's borrowed
                    // assertion (#6482) under the owner pin's helper
                    // identity gates, as boundary pairing reads it.
                    .with_helper_loans(
                        context.related_tests.iter().map(|(test, _)| *test),
                        |test, assertion| {
                            pin_syntax
                                .helper_loan(test, assertion, context.index)
                                .filter(|loan| {
                                    helper_pins_owner_call(
                                        &loan.helper,
                                        &mask_comments_and_strings(&test.body),
                                        context.owner_fn.map_or("", |owner| owner.name.as_str()),
                                    )
                                })
                        },
                    )
                    // RIPR-SPEC-0229 decision 7: a constant-row table's cells (#5328).
                    .with_table_rows(context.related_tests.iter().map(|(test, _)| *test))
            });
        let field_oracle_needs_admission = arm_selector
            .as_ref()
            .is_some_and(ArmSelector::field_oracle_needs_admission);
        let assertion_admitted = |test: &TestSummary, assertion: &OracleFact| {
            pin_syntax.admits_equality_assertion(context.probe, test, assertion, context.index)
                && (!field_oracle_needs_admission
                    || pin_syntax.admits(test, assertion, context.index))
        };
        // RIPR-SPEC-0094 Part D: the state a deleted `self.callee(..)` writes,
        // established once per probe; `None` keeps the Part C reading.
        let effect_carrier = context.owner_fn.and_then(|owner| {
            EffectStateCarrier::establish(
                context.probe,
                owner,
                context.index,
                context.workspace_complete,
            )
        });
        let package_defeats_by_file = FileDefeatMemo::default();
        // The run-level graph is lazy and shared with reach-limit disclosure
        // (#6778). Keep this owner walk local to the probe; unused proximity
        // gates still do not build the graph.
        let owner_reach = std::cell::OnceCell::new();
        let owner_locals = context
            .owner_fn
            .map(owner_local_binding_names)
            .unwrap_or_default();
        let owner_parameters = context
            .owner_fn
            .map(owner_parameter_names)
            .unwrap_or_default();
        // RIPR-SPEC-0228 keyed reads: only a map insert reads these names,
        // and only one at the owner's top level (not under a branch, loop or
        // closure), in an owner that touches that map nowhere else but to
        // read it. An earlier `return` can still skip it; the reveal side's
        // consumed-result and empty-start rules cover that. A
        // parameter the owner rebinds in its body (`let sku = "LAST"`) may
        // not be what a later use of its name holds, so it keys nothing.
        let constructor_memo: RefCell<BTreeMap<(String, String, String), bool>> =
            RefCell::new(BTreeMap::new());
        let owner_argument_names: Vec<String> = context
            .owner_fn
            .filter(|function| {
                unconditional_owner_insert_field(
                    function.body.as_str(),
                    function.start_line,
                    context.probe.location.line,
                )
                .is_some_and(|field| {
                    owner_touches_field_only_at(
                        context.index,
                        &function.id.0,
                        function.body.as_str(),
                        function.start_line,
                        context.probe.location.line,
                        &field,
                    )
                })
            })
            .map(|function| {
                call_parameter_names(function)
                    .into_iter()
                    .map(|name| {
                        if name.is_empty() || parameter_rebound(function.body.as_str(), &name) {
                            String::new()
                        } else {
                            name
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        // #5830/#7024: names of functions that transitively call the owner,
        // computed once per owner per run through the attached memo and only
        // when an assertion asks. The memo answers under its borrow without
        // cloning; unit-test contexts without a memo keep the per-probe cell.
        let owner_callers = std::cell::OnceCell::new();
        let expected_reaches_owner = |ty: Option<&str>, name: &str| {
            context.owner_fn.is_some_and(|owner| {
                if let Some(memo) = context.owner_caller_names {
                    return memo.caller_reaches(context.index, owner, ty, name);
                }
                owner_callers
                    .get_or_init(|| transitive_caller_names(owner, context.index))
                    .iter()
                    .any(|(caller_type, caller)| {
                        caller == name && ty.is_none_or(|ty| caller_type.as_deref() == Some(ty))
                    })
            })
        };
        // #3731 review (F11): the related test's file source is reachable
        // here, so the caller computes the same-name-import defeat per test
        // instead of restructuring the reveal inputs.
        let import_defeats = |test: &TestSummary, callee: &str| {
            context.index.files().get(&test.file).is_some_and(|facts| {
                context.test_file_imports_foreign_callee_name(&test.file, &facts.source, callee)
            })
        };
        // #3731 review (G1): the test's OWN package defining a same-named
        // function defeats the bare-scrutinee binding the same way a foreign
        // import does — the bare call in that test may bind the local
        // definition while the changed owner lives in another package.
        // Index-backed, not a new lexical scan: package scopes come from the
        // shared `package_prefix` authority and the same-named definition
        // from the workspace's indexed functions. Both package scopes must
        // resolve; an unscopable side (single-crate relative paths, absolute
        // paths) keeps today's behavior.
        let cross_package_defeats = |test: &TestSummary, callee: &str| {
            memoized_file_defeat(&package_defeats_by_file, &test.file, callee, || {
                let Some(test_package) = package_prefix(&test.file) else {
                    return false;
                };
                let Some(owner_package) = owner_package.as_deref() else {
                    return false;
                };
                if test_package == owner_package {
                    return false;
                }
                context.index.functions().iter().any(|function| {
                    function.name == callee
                        && package_prefix(&function.file).as_deref() == Some(test_package.as_str())
                })
            })
        };
        let owner_pin_admits = |test: &TestSummary, assertion: &OracleFact| {
            field_projection.as_ref().is_some_and(|receipt| {
                receipt.valid_for(context.probe) && receipt.admits(test, assertion)
            }) || fallible_checks
                .iter()
                .any(|receipt| receipt.valid_for(context.probe) && receipt.admits(test, assertion))
                || owner_return_pin.as_ref().is_some_and(|pin| {
                    pin.admits(
                        test,
                        assertion,
                        context.index,
                        &|file, name| {
                            context.index.files().get(file).is_some_and(|facts| {
                                context.test_file_imports_foreign_callee_name(
                                    file,
                                    &facts.source,
                                    name,
                                )
                            })
                        },
                        pin_syntax,
                    )
                })
        };
        let reveal = reveal_outcome(
            context.probe,
            reveal_expression,
            &context.related_tests,
            &owner_locals,
            &import_defeats,
            &cross_package_defeats,
            &ReturnOracleAdmission {
                owner_return_pin: &owner_pin_admits,
                assertion_admitted: &assertion_admitted,
                sibling_exclusion_admitted: &sibling_exclusion_admitted,
                proximity_may_reach_owner: &|test| {
                    context.owner_fn.is_none_or(|owner| {
                        owner_reach
                            .get_or_init(|| proximity_reach.owner_reach(&owner.name))
                            .test_may_reach(test)
                    })
                },
                owner_parameters: &owner_parameters,
                expected_reaches_owner: &expected_reaches_owner,
                expected_operand_value: &|test, operand| {
                    context.owner_fn.and_then(|owner| {
                        expected_operand_value(context.index, test, operand, owner)
                    })
                },
                effect_state_carried: &|test, assertion| {
                    effect_carrier
                        .as_ref()
                        .is_none_or(|carrier| carrier.admits(test, assertion))
                },
                setter_writes_field: &|impl_type, setter, field| {
                    setter_plainly_writes(context.index, impl_type, setter, field)
                },
                collection_argument_observed: &|test, assertion| {
                    direct_collection_mutation_receiver(&context.probe.expression)
                        .zip(context.owner_fn)
                        .is_some_and(|(receiver, owner)| {
                            assertion_observes_collection_argument(
                                receiver,
                                owner,
                                context.index,
                                test,
                                assertion,
                                pin_syntax,
                            )
                        })
                },
                getter_reads_field: &|impl_type, getter, field| {
                    getter_plainly_reads(context.index, impl_type, getter, field)
                },
                getter_reads_key: &|impl_type, getter, field| {
                    getter_plainly_reads_key(context.index, impl_type, getter, field)
                },
                owner_argument_names: &owner_argument_names,
                owner_returns_value: context
                    .owner_fn
                    .is_none_or(|function| function_returns_value(function.body.as_str())),
                owner_may_exit_before_probe: context.owner_fn.is_some_and(|function| {
                    crate::analysis::classify::owner_may_exit_before_line(
                        function.body.as_str(),
                        function.start_line,
                        context.probe.location.line,
                        &context.probe.expression,
                    )
                }),
                constructor_starts_empty: &|impl_type, ctor, field| {
                    // Memoized per probe: the derived-`Default` check parses
                    // the files declaring the type, once per question.
                    let key = (impl_type.to_string(), ctor.to_string(), field.to_string());
                    if let Some(known) = constructor_memo.borrow().get(&key) {
                        return *known;
                    }
                    let empty = constructor_starts_empty(context.index, impl_type, ctor, field);
                    constructor_memo.borrow_mut().insert(key, empty);
                    empty
                },
                call_cannot_store: &|impl_type, method, field, expected| {
                    method_cannot_store(context.index, impl_type, method, field, expected)
                },
            },
            arm_selector.as_ref(),
            context.diff_touched,
        );
        let (observe, discriminate, related_tests, matched_total) = (
            reveal.observe,
            reveal.discriminate,
            reveal.related,
            reveal.related_total,
        );
        // #6692: a clone-field pin (`assert_eq!(recv.clone(), recv)` through
        // a derived `PartialEq`) observes the constructed field, so the
        // missing-field fact below no longer stands for this probe. Only an
        // owner pin that reveal credited clears it, after reveal's own
        // gates (a name-only relation next to a reach-bearing test, a
        // foreign same-name import, a cross-package same-name definition);
        // a token match never does.
        // #7858: a field built inside an array the owner returns is observed
        // by a test that pins that element alone, by its literal index or by
        // an `.any(..)` membership no other element can satisfy. The same
        // name defeats as reveal's owner pins apply to the owner call.
        let element_field_pinned = matches!(context.probe.family, ProbeFamily::FieldConstruction)
            && activation.missing_discriminators.iter().any(|fact| {
                fact.flow_sink
                    .as_ref()
                    .is_some_and(|sink| sink.kind == FlowSinkKind::StructField)
            })
            && context.owner_fn.is_some_and(|owner| {
                // The pin matches the owner call by name. A second indexed
                // function of that name (a nested module's `super::rows`, a
                // test file's local `fn rows`) may be the one the test
                // calls, so only a unique name is read (#8059 review).
                context
                    .index
                    .functions()
                    .iter()
                    .filter(|function| function.name == owner.name)
                    .count()
                    == 1
                    && context.related_tests.iter().any(|(test, _)| {
                        !import_defeats(test, &owner.name)
                            && !cross_package_defeats(test, &owner.name)
                            && observes_returned_element_field(
                                context.probe,
                                owner,
                                test,
                                context.index,
                                pin_syntax,
                            )
                    })
            });
        if matches!(context.probe.family, ProbeFamily::FieldConstruction)
            && (reveal.owner_pin_credited || element_field_pinned)
        {
            activation.missing_discriminators.retain(|fact| {
                fact.flow_sink
                    .as_ref()
                    .is_none_or(|sink| sink.kind != FlowSinkKind::StructField)
            });
        }

        let discriminate =
            tuple_match::discrimination(context, &observe, &discriminate).unwrap_or(discriminate);
        let discriminate =
            side_flip::discrimination(context, &observe, &discriminate).unwrap_or(discriminate);
        // RIPR-SPEC-0227 rule 3 pairs its own oracles: both sides of the
        // guard's edge are asserted in one test, so the #4828 same-test
        // pairing below has nothing left to establish.
        let guard_side_flip = side_flip::guard::discrimination(context, &observe, &discriminate);
        let guard_side_flip_paired = guard_side_flip.is_some();
        let discriminate = guard_side_flip.unwrap_or(discriminate);
        // #4828: a boundary-class probe may not read `exposed` by taking a
        // boundary input from one test and a discriminating oracle from
        // another. Infection and discrimination stay independently scored;
        // only the combined `exposed` path requires same-test pairing on
        // the owner call that sits on the boundary. Pairing reuses
        // activation's `==` facts so named constants and helper hops that
        // already infected stay paired when the same test holds the oracle.
        // The wrapper-entry pairing reads only rows recomputed from the
        // asserting test (#6780 review): a run-wide row names no test.
        let one_test_activation = |test: &TestSummary| {
            activation_and_boundary_input(
                context.probe,
                context.owner_fn,
                &[test],
                &flow_sinks,
                context.helper_chain.as_ref(),
                context.index,
                context.workspace_complete,
                context.test_value_facts,
            )
            .activation
        };
        let discriminate = if matches!(context.probe.family, ProbeFamily::Predicate)
            && infect.state == StageState::Yes
            && discriminate.state == StageState::Yes
            && !guard_side_flip_paired
            && !has_same_test_boundary_oracle_pairing(
                context.probe,
                context.owner_fn,
                &test_summaries,
                &activation,
                &assertion_admitted,
                // The same owner-pin decision and binding defeats reveal
                // applied, so pairing cannot credit a pin reveal refused.
                &|test, assertion| {
                    matches!(assertion.kind, OracleKind::RelationalCheck)
                        && context.owner_fn.is_some_and(|owner| {
                            !import_defeats(test, &owner.name)
                                && !cross_package_defeats(test, &owner.name)
                        })
                        && owner_pin_admits(test, assertion)
                },
                // #6482: a check helper's borrowed assertion, under the
                // same name defeats as an inline one and the owner pin's
                // helper identity gates: the helper binds the owner's name
                // nowhere and the test never names it.
                &|test, assertion| {
                    let owner = context.owner_fn.filter(|owner| {
                        !import_defeats(test, &owner.name)
                            && !cross_package_defeats(test, &owner.name)
                    })?;
                    pin_syntax
                        .helper_loan(test, assertion, context.index)
                        .filter(|loan| {
                            helper_pins_owner_call(
                                &loan.helper,
                                &mask_comments_and_strings(&test.body),
                                &owner.name,
                            )
                        })
                },
                context
                    .helper_chain
                    .as_ref()
                    .map(|chain| WrapperEntryPairing {
                        chain,
                        test_activation: &one_test_activation,
                    }),
                context.index,
            ) {
            StageEvidence::new(
                StageState::Weak,
                Confidence::Medium,
                same_test_pairing_missing_summary(),
            )
        } else {
            discriminate
        };
        // The missing-field fact is the authority on whether an assertion
        // observes the constructed field. A token that merely coincides with
        // the field value (`Box` in `downcast_ref::<Box<dyn E>>()`) must not
        // make the finding `exposed` while that fact says nothing observes it.
        let discriminate = if matches!(context.probe.family, ProbeFamily::FieldConstruction)
            && discriminate.state == StageState::Yes
            && activation.missing_discriminators.iter().any(|fact| {
                fact.flow_sink
                    .as_ref()
                    .is_some_and(|sink| sink.kind == FlowSinkKind::StructField)
            }) {
            StageEvidence::new(
                StageState::Weak,
                Confidence::Medium,
                "Discriminator unconfirmed: no field-value assertion observes the constructed field",
            )
        } else {
            discriminate
        };
        // #7077: an exact pin on the field is no discriminator when every
        // test that pins it also pins a sibling field, bound to one operand,
        // to the same value: the field equals that operand there. Both
        // operands must be established primitive, so the dropped operator cannot
        // be an overloaded call with side effects (#7084 review).
        let discriminate = if matches!(context.probe.family, ProbeFamily::FieldConstruction)
            && discriminate.state == StageState::Yes
            && let Some(owner) = context.owner_fn
            && let Some(pin) = operand_only_pin(
                &context.probe.expression,
                &owner.name,
                owner.body.as_str(),
                &context.related_tests,
                context.index,
            ) {
            StageEvidence::new(StageState::Weak, Confidence::Medium, pin.summary())
        } else {
            discriminate
        };
        // #8480: the generic oracle path credits a whole-object equality
        // assertion on the returned value without consulting the owner
        // return pin, so a returned type whose `Drop` may exit while
        // unwinding from a failed assertion demotes here, at the final
        // semantic owner, exactly as the pin's own refusal does. The same
        // holds for the error-path and predicate families: their credited
        // shapes hold the returned value in the asserting test while it
        // decides, so the same unwind can mask the failure.
        let discriminate = if matches!(
            context.probe.family,
            ProbeFamily::ReturnValue | ProbeFamily::ErrorPath | ProbeFamily::Predicate
        ) && discriminate.state == StageState::Yes
            && context.owner_fn.is_some_and(|owner| {
                !OwnerReturnPin::returned_types_drop_plain(owner, context.index)
            }) {
            StageEvidence::new(
                StageState::Weak,
                Confidence::Medium,
                "Discriminator unconfirmed: a non-plain Drop on the returned type may exit while unwinding from a failed assertion (observation_unverified)",
            )
        } else {
            discriminate
        };
        // Tests kept only as suggested locations never run the owner, so
        // they neither activate the change nor observe it.
        let reach_ruled_out = reach.state == StageState::No
            && context
                .owner_fn
                .is_some_and(|owner| !context.owner_may_be_reached_unseen(owner));
        // A weak stage is replaced too: with no reaching test, an
        // unconfirmed oracle (#5830 withholds confirmation from tokens that
        // only coincide with the owner's) is no discriminator at all.
        let unreached = |stage: StageEvidence, verb: &str| {
            if reach_ruled_out && matches!(stage.state, StageState::Yes | StageState::Weak) {
                unreached_stage(verb)
            } else {
                stage
            }
        };
        let infect = unreached(infect, "activate");
        let observe = unreached(observe, "observe");
        let discriminate = unreached(discriminate, "discriminate");

        // #1715: a complete expression identifies the changed `?`, but an
        // equality for another error exit cannot certify that propagation path.
        // Retain the closing-line origin and refuse new token-only promotion
        // until same-test activation of this exact boundary is established.
        let discriminate = if context.propagation_origin.is_some()
            && discriminate.state == StageState::Yes
        {
            StageEvidence::new(
                StageState::Weak,
                Confidence::Low,
                format!(
                    "Complete propagation subject located; activation of the changed `?` exit is unestablished ({PROPAGATION_BOUNDARY_UNVERIFIED})"
                ),
            )
        } else {
            discriminate
        };

        let ripr = RiprEvidence {
            reach: reach.clone(),
            infect: infect.clone(),
            propagate: propagate.clone(),
            reveal: RevealEvidence {
                observe: observe.clone(),
                discriminate: discriminate.clone(),
            },
        };
        let mut evidence =
            evidence_summaries([&reach, &infect, &propagate, &observe, &discriminate]);
        // Disclose a refused related assertion whenever the refusal can
        // matter: the reveal is not fully established. See
        // `preferred_refusal_note` for which one.
        let owner_name = context.owner_fn.map_or("", |owner| owner.name.as_str());
        // A value guard is a limit only when the call it reads is the owner's.
        let is_limit = |refusal: &AssertionRefusal| {
            refusal.is_analyzer_limit()
                && refusal
                    .value_guard_subject()
                    .is_none_or(|subject| direct_call_name(subject) == Some(owner_name))
        };
        let assertion_refusal = (observe.summary == ASSERTION_CONTEXT_UNESTABLISHED
            || discriminate.state != StageState::Yes)
            .then(|| {
                let mut notes = Vec::new();
                for (test, _) in &context.related_tests {
                    for assertion in &test.assertions {
                        let Some(refusal) = pin_syntax.equality_assertion_refusal(
                            context.probe,
                            test,
                            assertion,
                            context.index,
                        ) else {
                            continue;
                        };
                        let calls_owner = body_contains_owner_call(&assertion.text, owner_name);
                        let note = AssertionRefusalNote {
                            location: format!(
                                "`{}!` in {} at {}:{}",
                                if context.probe.family == ProbeFamily::MatchArm {
                                    "assert"
                                } else {
                                    "assert_eq"
                                },
                                test.name,
                                test.file.display(),
                                assertion.line
                            ),
                            reason: refusal.describe(),
                            calls_owner,
                            is_analyzer_limit: is_limit(&refusal),
                        };
                        notes.push(note);
                    }
                }
                preferred_refusal_note(notes)
            })
            .flatten();
        if let Some(note) = &assertion_refusal {
            evidence.push(format!(
                "{ASSERTION_NOT_CREDITED_PREFIX}{}: {}",
                note.location, note.reason
            ));
        }
        let refusals_are_analyzer_limits = observe.summary == ASSERTION_CONTEXT_UNESTABLISHED && {
            // Only tests that could have credited an oracle: a refused
            // assertion in a name-only test cannot stand in for the missing
            // oracle of a reach-bearing one.
            let credits = oracle_crediting_relations(&context.related_tests);
            let mut refusals = context
                .related_tests
                .iter()
                .filter(|(_, reason)| credits(*reason))
                .flat_map(|(test, _)| {
                    test.assertions.iter().filter_map(|assertion| {
                        pin_syntax.equality_assertion_refusal(
                            context.probe,
                            test,
                            assertion,
                            context.index,
                        )
                    })
                });
            refusals.next().is_some_and(|first| is_limit(&first))
                && refusals.all(|refusal| is_limit(&refusal))
        };

        // #8051: the owner pin's own admission, spelled `debug_assert_eq!`.
        // Only a gap reads it, and only as a profile limit.
        let debug_assert_eq_pins: Vec<String> = owner_return_pin
            .as_ref()
            .map(|pin| {
                context
                    .related_tests
                    .iter()
                    .filter(|(test, reason)| {
                        *reason == RelationReason::DirectOwnerCall
                            && test.body.as_str().contains("debug_assert_eq")
                            && !crate::analysis::classify::is_ignored(test)
                    })
                    .filter(|(test, _)| {
                        let Some(facts) = context.index.files().get(&test.file) else {
                            return false;
                        };
                        pin_syntax
                            .debug_assert_eq_calls(
                                &test.file,
                                &facts.source,
                                test.start_line,
                                test.end_line,
                            )
                            .into_iter()
                            .any(|(line, text)| {
                                let assertion = OracleFact {
                                    line,
                                    text,
                                    kind: OracleKind::ExactValue,
                                    strength: OracleStrength::Strong,
                                    observed_tokens: Vec::new(),
                                    ok_value_observed: None,
                                };
                                pin.admits_debug_assert_eq(
                                    test,
                                    &assertion,
                                    context.index,
                                    &|file, name| {
                                        context.index.files().get(file).is_some_and(|facts| {
                                            context.test_file_imports_foreign_callee_name(
                                                file,
                                                &facts.source,
                                                name,
                                            )
                                        })
                                    },
                                    pin_syntax,
                                )
                            })
                    })
                    .map(|(test, _)| {
                        format!(
                            "`{}` ({}:{})",
                            test.name,
                            test.file.display().to_string().replace('\\', "/"),
                            test.start_line
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let related_call_budget_saturated = context
            .related_tests
            .iter()
            .any(|(test, _)| crate::analysis::extract::has_saturated_line(test.calls.iter()));

        Self {
            ripr,
            evidence,
            flow_sinks,
            propagation_witness,
            activation,
            related_tests,
            related_tests_matched_total: matched_total,
            reach,
            infect,
            propagate,
            observe,
            discriminate,
            reach_ruled_out,
            assertion_refusal,
            refusals_are_analyzer_limits,
            debug_assert_eq_pins,
            related_call_budget_saturated,
        }
    }

    pub(in crate::analysis) fn classify(&self, probe: &Probe) -> ExposureClass {
        // A changed line inside a function no test reaches has no static path
        // whatever its shape: "cannot classify, escalate" would send the
        // reader after mutation testing when the plain gap is a missing test.
        if self.reach_ruled_out {
            return ExposureClass::NoStaticPath;
        }
        let class = classify(
            &self.reach,
            &self.infect,
            &self.propagate,
            &self.observe,
            &self.discriminate,
            probe,
        );
        // RIPR-SPEC-0228: a field assignment is credited only through a
        // confirmed read of that field after the owner call. A weak reading
        // there means ripr could not tell whether a related assertion reads
        // the field (a helper, a projection, a getter it cannot resolve), so
        // it abstains as before instead of claiming a gap.
        if class == ExposureClass::WeaklyExposed
            && matches!(probe.family, ProbeFamily::FieldConstruction)
            && crate::analysis::classify::field_assignment_target(&probe.expression).is_some()
        {
            return ExposureClass::StaticUnknown;
        }
        class
    }

    pub(in crate::analysis) fn confidence(&self, class: &ExposureClass) -> f32 {
        confidence_score(
            &self.reach,
            &self.infect,
            &self.propagate,
            &self.observe,
            &self.discriminate,
            class,
        )
    }

    pub(in crate::analysis) fn propagation_witness(&self) -> Option<&PropagationWitnessDiagnostic> {
        self.propagation_witness.as_ref()
    }
}

pub(in crate::analysis) enum PropagationWitnessDiagnostic {
    Valid(PropagationWitnessV1),
    InvalidDigest(PropagationWitnessV1),
}

impl PropagationWitnessDiagnostic {
    fn from_witness(witness: PropagationWitnessV1) -> Self {
        if witness.digest_matches() {
            Self::Valid(witness)
        } else {
            Self::InvalidDigest(witness)
        }
    }

    pub(in crate::analysis) fn witness(&self) -> &PropagationWitnessV1 {
        match self {
            Self::Valid(witness) | Self::InvalidDigest(witness) => witness,
        }
    }

    pub(in crate::analysis) fn is_invalid(&self) -> bool {
        matches!(self, Self::InvalidDigest(_))
    }
}

fn unreached_stage(stage: &str) -> StageEvidence {
    StageEvidence::new(
        StageState::No,
        Confidence::Medium,
        format!("No test reaches the changed owner, so no test can {stage} this change"),
    )
}

fn evidence_summaries<'e>(stages: impl IntoIterator<Item = &'e StageEvidence>) -> Vec<String> {
    let mut summaries = stages
        .into_iter()
        .filter_map(|stage| (!stage.summary.is_empty()).then_some(stage.summary.clone()))
        .collect::<Vec<_>>();
    summaries.sort();
    summaries.dedup();
    summaries
}

/// Names the owner binds with `let` that are not also named in its
/// signature. Such a binding exists only inside the owner, so a test's
/// same-named local can never be it. A `let` that rebinds a parameter
/// (`let cache = cache;`) keeps the parameter's name out of this list.
/// Parser-backed facts only; a lexically indexed owner yields none.
fn owner_local_binding_names(owner: &FunctionSummary) -> Vec<String> {
    let signature = owner
        .body
        .split_once('{')
        .map_or(owner.body.as_str(), |(head, _)| head);
    let mut names = owner
        .let_bindings
        .iter()
        .map(|binding| binding.name.clone())
        .filter(|name| !contains_as_whole_word(signature, name))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// Names the owner's signature binds as parameters, without `mut` or `ref`.
fn owner_parameter_names(owner: &FunctionSummary) -> Vec<String> {
    let mut names = signature_parameters(owner)
        .into_iter()
        .map(|name| {
            name.trim_start_matches("ref ")
                .trim_start_matches("mut ")
                .trim()
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

/// Bound on the caller walk: deeper chains keep the assertion's credit.
const MAX_CALLER_DEPTH: usize = 6;

/// Indexed functions whose calls reach `owner` within `MAX_CALLER_DEPTH`
/// hops, as (impl type, name). A call counts only when its own syntax can
/// name the callee (see `call_names_function`), so `String::new()` or
/// `values.len()` never reaches an owner `Rect::new` or `Stack::len`. A
/// type-qualified expected-side call (`Money::new(8)`) matches only a
/// caller in that type's impl; any other call matches by name alone, so a
/// same-named function elsewhere can only withhold credit.
fn transitive_caller_names(
    owner: &FunctionSummary,
    index: &crate::analysis::facts::RustIndex,
) -> std::collections::BTreeSet<(Option<String>, String)> {
    let mut callers = std::collections::BTreeSet::new();
    let mut visited = std::collections::HashSet::from([&owner.id]);
    let mut frontier = vec![owner];
    for _ in 0..MAX_CALLER_DEPTH {
        let mut next = Vec::new();
        for function in index.functions().iter() {
            if visited.contains(&function.id) {
                continue;
            }
            let reaches = function.calls.iter().any(|call| {
                frontier
                    .iter()
                    .any(|callee| call_names_function(call, callee, function))
            });
            if reaches {
                visited.insert(&function.id);
                callers.insert((
                    crate::analysis::classify::impl_self_type_name(&function.id.0),
                    function.name.clone(),
                ));
                next.push(function);
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    callers
}

/// Run-scoped memo for #5830 transitive caller walks (#7024), mirroring
/// `TestValueFacts`. Entries are keyed by the owner's slot in the index the
/// memo was first queried with; an owner that is not an element of that
/// index, or a query against another index, is computed fresh and never
/// cached, so a key can only ever name the same caller set.
/// Caller sets by owner slot — the memo payload for `OwnerCallerNames`.
type OwnerCallerSets = BTreeMap<usize, BTreeSet<(Option<String>, String)>>;

#[derive(Clone, Debug, Default)]
pub(in crate::analysis) struct OwnerCallerNames {
    index_identity: SyncMemo<Option<(usize, usize, u64)>>,
    by_owner_slot: SyncMemo<OwnerCallerSets>,
    /// Walks the memo ran, so a test can tell a hit from a recompute.
    #[cfg(test)]
    walks: SyncMemo<usize>,
}

impl OwnerCallerNames {
    /// Whether a caller named `name` (optionally of type `ty`) reaches
    /// `owner`: a cache hit scans the cached set under its borrow and never
    /// clones. Misses walk once, then cache by move.
    pub(in crate::analysis) fn caller_reaches(
        &self,
        index: &RustIndex,
        owner: &FunctionSummary,
        ty: Option<&str>,
        name: &str,
    ) -> bool {
        let matches = |caller: &(Option<String>, String)| {
            caller.1 == name && ty.is_none_or(|ty| caller.0.as_deref() == Some(ty))
        };
        let Some(slot) = self.slot_key(index, owner) else {
            return transitive_caller_names(owner, index).iter().any(matches);
        };
        if let Some(cached) = self.by_owner_slot.borrow().get(&slot) {
            return cached.iter().any(matches);
        }
        let callers = self.walk(owner, index);
        let found = callers.iter().any(matches);
        self.by_owner_slot.borrow_mut().insert(slot, callers);
        found
    }

    /// `transitive_caller_names(owner, index)`, computed once per owner per run.
    #[cfg(test)]
    pub(in crate::analysis) fn callers_for(
        &self,
        index: &RustIndex,
        owner: &FunctionSummary,
    ) -> BTreeSet<(Option<String>, String)> {
        let Some(slot) = self.slot_key(index, owner) else {
            return transitive_caller_names(owner, index);
        };
        if let Some(cached) = self.by_owner_slot.borrow().get(&slot) {
            return cached.clone();
        }
        let callers = self.walk(owner, index);
        self.by_owner_slot
            .borrow_mut()
            .insert(slot, callers.clone());
        callers
    }

    /// The cache cells, so tests can inspect them between queries. A guard
    /// must not be held across a query: the memo is a lock, and re-entry
    /// panics in test builds.
    #[cfg(test)]
    pub(in crate::analysis) fn slots_for_test(&self) -> &SyncMemo<OwnerCallerSets> {
        &self.by_owner_slot
    }

    fn walk(
        &self,
        owner: &FunctionSummary,
        index: &RustIndex,
    ) -> BTreeSet<(Option<String>, String)> {
        #[cfg(test)]
        {
            *self.walks.borrow_mut() += 1;
        }
        transitive_caller_names(owner, index)
    }

    /// How many times the memo walked callers instead of serving a hit.
    #[cfg(test)]
    pub(in crate::analysis) fn walks_for_test(&self) -> usize {
        *self.walks.borrow()
    }

    fn slot_key(&self, index: &RustIndex, owner: &FunctionSummary) -> Option<usize> {
        let identity = index.storage_identity();
        {
            let mut bound = self.index_identity.borrow_mut();
            match *bound {
                None => *bound = Some(identity),
                Some(bound) if bound != identity => return None,
                Some(_) => {}
            }
        }
        index.function_slot(owner)
    }
}

/// Roots whose paths never name a workspace function.
const FOREIGN_PATH_ROOTS: [&str; 3] = ["std", "core", "alloc"];

/// Whether `call`, written inside `caller`, can name `callee` by syntax alone.
///
/// A free function is named by a bare call or a lowercase module path that
/// does not start at `std`/`core`/`alloc`. An associated function is named
/// by `<Type>::name`, by `Self::name` inside an impl of the same type, or by
/// `self.name(` inside such an impl. Any other method call (`values.len()`)
/// or type-qualified call (`String::new()`) is unresolved and does not count.
fn call_names_function(
    call: &crate::analysis::facts::CallFact,
    callee: &FunctionSummary,
    caller: &FunctionSummary,
) -> bool {
    if call.name != callee.name {
        return false;
    }
    let callee_type = crate::analysis::classify::impl_self_type_name(&callee.id.0);
    let same_impl = || {
        callee_type.is_some()
            && crate::analysis::classify::impl_self_type_name(&caller.id.0) == callee_type
    };
    // Call facts keep the raw source line; a name inside a comment or
    // string on that line is not a call (#6970 review).
    let text = crate::analysis::extract::mask_comments_and_strings(&call.text);
    call_name_prefixes(&text, &call.name).any(|prefix| {
        if let Some(receiver) = prefix.strip_suffix('.') {
            return receiver_is_self(receiver) && same_impl();
        }
        let Some(path) = prefix.strip_suffix("::") else {
            // A bare call may follow a keyword (`return tax(v)`), a field
            // label or any operator; only the declaration `fn tax(` is not
            // a call.
            return callee_type.is_none() && !ends_with_word(prefix, "fn");
        };
        let segments = trailing_path_segments(path);
        match (&callee_type, segments.last()) {
            (Some(ty), Some(last)) => *last == ty.as_str() || (*last == "Self" && same_impl()),
            (None, Some(last)) => {
                last.starts_with(|c: char| c.is_ascii_lowercase())
                    && segments
                        .first()
                        .is_some_and(|root| !FOREIGN_PATH_ROOTS.contains(root))
            }
            (_, None) => false,
        }
    })
}

/// The text before each whole-word `name` that is followed by `(` or `::<`.
fn call_name_prefixes<'a>(text: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    text.match_indices(name).filter_map(move |(at, _)| {
        let before = &text[..at];
        let after = text[at + name.len()..].trim_start();
        let word_start = !before.ends_with(|c: char| c.is_alphanumeric() || c == '_');
        let called = after.starts_with('(') || after.starts_with("::<");
        (word_start && called).then(|| before.trim_end())
    })
}

fn ends_with_word(text: &str, word: &str) -> bool {
    text.strip_suffix(word)
        .is_some_and(|rest| !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
}

fn receiver_is_self(receiver: &str) -> bool {
    receiver
        .trim_end()
        .strip_suffix("self")
        .is_some_and(|rest| !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '.'))
}

/// The `::`-separated identifier segments ending at the end of `path`, with
/// a trailing turbofish or generic list (`Stack::<u32>`) dropped.
fn trailing_path_segments(path: &str) -> Vec<&str> {
    let path = path.trim_end();
    let path = match path.strip_suffix('>') {
        Some(_) => path
            .rfind('<')
            .map_or("", |open| path[..open].trim_end_matches("::")),
        None => path,
    };
    let start = path
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == ':'))
        .map_or(0, |(at, c)| at + c.len_utf8());
    path[start..]
        .split("::")
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// Per-file defeat results for one probe, keyed by test file then callee.
type FileDefeatMemo = RefCell<BTreeMap<PathBuf, BTreeMap<String, bool>>>;

/// Returns the cached defeat for `(file, callee)`, computing it once.
fn memoized_file_defeat(
    memo: &FileDefeatMemo,
    file: &Path,
    callee: &str,
    compute: impl FnOnce() -> bool,
) -> bool {
    if let Some(cached) = memo
        .borrow()
        .get(file)
        .and_then(|by_callee| by_callee.get(callee))
    {
        return *cached;
    }
    let defeats = compute();
    memo.borrow_mut()
        .entry(file.to_path_buf())
        .or_default()
        .insert(callee.to_string(), defeats);
    defeats
}

/// The refused `assert_eq!` to disclose. One whose text calls the changed
/// owner comes first, since an unrelated refused assertion
/// (`if flag { assert_eq!(1, 1) }`) could not observe the change even if it
/// were credited. Among those, a refusal that is not an analyzer limit comes
/// first (#6903): it is what keeps the gap, so the next step must not offer
/// the static-limit reading a limit refusal earns.
fn preferred_refusal_note(notes: Vec<AssertionRefusalNote>) -> Option<AssertionRefusalNote> {
    let rank = |note: &AssertionRefusalNote| match (note.calls_owner, note.is_analyzer_limit) {
        (true, false) => 0,
        (true, true) => 1,
        (false, _) => 2,
    };
    // `min_by_key` keeps the first of equal ranks, so test order still
    // decides within a rank.
    notes.into_iter().min_by_key(rank)
}

/// True when `name` occurs as a whole identifier in the code of `text`,
/// ignoring comments and string contents.
fn mentions_identifier(text: &str, name: &str) -> bool {
    if name.is_empty() || !text.contains(name) {
        return false;
    }
    mask_comments_and_strings(text)
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == name)
}

/// The brace-delimited bodies of every `macro_rules!` item in `source`.
fn macro_rules_bodies(source: &str) -> Vec<String> {
    if !source.contains("macro_rules!") {
        return Vec::new();
    }
    let masked = mask_comments_and_strings(source);
    let mut bodies = Vec::new();
    let mut from = 0;
    while let Some(found) = masked[from..].find("macro_rules!") {
        let start = from + found;
        let Some(open) = masked[start..]
            .find(['{', '(', '['])
            .map(|offset| start + offset)
        else {
            break;
        };
        let (opener, closer) = match masked.as_bytes().get(open) {
            Some(b'{') => (b'{', b'}'),
            Some(b'(') => (b'(', b')'),
            _ => (b'[', b']'),
        };
        let mut depth = 0usize;
        let mut end = masked.len();
        for (offset, byte) in masked.as_bytes()[open..].iter().enumerate() {
            if *byte == opener {
                depth += 1;
            } else if *byte == closer {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = open + offset + 1;
                    break;
                }
            }
        }
        bodies.push(masked[open..end].to_string());
        from = end;
    }
    bodies
}

#[cfg(test)]
mod tests {
    use super::evidence_summaries;
    use super::owner_local_binding_names;
    use super::trailing_path_segments;
    use super::{ClassifiedProbeEvidence, ProbeContext, PropagationWitnessDiagnostic};
    use crate::analysis::classifier::finding::build_finding;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::facts::LetBindingFact;
    use crate::analysis::facts::{FunctionFact, FunctionSummary, ReturnFact, RustIndex};
    use crate::analysis::rust_index::{OracleFact, TestSummary, extract_identifier_tokens};
    use crate::domain::{
        Confidence, DeltaKind, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId,
        RelationReason, SourceLocation, StageEvidence, StageState, SymbolId,
    };
    use std::path::PathBuf;

    // --- #3731 review (G1): the cross-package same-name defeat ---

    /// The guarded-match harness text whose bare scrutinee binds the owner
    /// by name — the confirmation the cross-package gate decides on.
    fn guarded_oracle() -> OracleFact {
        let text = "match expect_response(..) { Ok(..) => .., Err(..) => Some(ParseError::InvalidData { .. }) }";
        OracleFact {
            line: 3,
            text: text.to_string(),
            kind: OracleKind::GuardedResultMatch,
            strength: OracleStrength::Strong,
            observed_tokens: extract_identifier_tokens(text),
            ok_value_observed: Some(true),
        }
    }

    fn harness_in(file: &str) -> TestSummary {
        TestSummary {
            name: "guards_the_result".to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 9,
            body: "match expect_response(&input, \"ready\") { .. }".into(),
            calls: Vec::new(),
            assertions: vec![guarded_oracle()],
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn same_named_function(file: &str) -> FunctionFact {
        FunctionFact {
            id: SymbolId(format!("{file}::expect_response")),
            name: "expect_response".to_string(),
            file: PathBuf::from(file),
            start_line: 1,
            end_line: 4,
            body: "fn expect_response() -> Result<u32, ParseError> { Ok(1) }".into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    /// The changed owner lives in package `alpha`; its guarded harness
    /// shares no changed-line token with the probe, so the reveal-side
    /// confirmation rides solely on the producer-owned bare binding.
    fn owner_harness_context(index: &RustIndex, test: TestSummary) -> ClassifiedProbeEvidence {
        let probe = Probe {
            id: ProbeId("probe:alpha:expect_response".to_string()),
            location: SourceLocation::new("crates/alpha/src/lib.rs", 10, 2),
            owner: Some(SymbolId(
                "crates/alpha/src/lib.rs::expect_response".to_string(),
            )),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("amount".to_string()),
            after: Some("amount + 1".to_string()),
            // No token of this expression appears in the guarded oracle
            // text, so only the owner binding can confirm.
            expression: "if trimmed != Some(expected_id.trim()).as_str() {".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let owner = FunctionSummary {
            id: SymbolId("crates/alpha/src/lib.rs::expect_response".to_string()),
            name: "expect_response".to_string(),
            file: PathBuf::from("crates/alpha/src/lib.rs"),
            start_line: 1,
            end_line: 20,
            body: "fn expect_response() -> Result<u32, ParseError> { Ok(1) }".into(),
            calls: Vec::new(),
            returns: vec![ReturnFact {
                line: 14,
                text: "Ok(amount)".to_string(),
            }],
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        let context = ProbeContext::new(
            &probe,
            Some(&owner),
            vec![(&test, RelationReason::DirectOwnerCall)],
            false,
            index,
            true,
        );
        ClassifiedProbeEvidence::gather(
            &context,
            "if trimmed != Some(expected_id.trim()).as_str() {",
            &crate::analysis::classify::TransitiveReachIndex::new(context.index),
        )
    }

    /// #8480 residual: the generic oracle path credits a whole-object
    /// equality assertion on the returned value without consulting the
    /// owner return pin, so a returned type whose Drop may exit during
    /// unwind must still demote `return_value` at the final owner.
    #[test]
    fn a_non_plain_drop_on_the_returned_type_keeps_return_value_weakly() -> Result<(), String> {
        use crate::domain::ExposureClass;

        let class_for = |drop_body: &str| -> Result<ExposureClass, String> {
            let source = format!(
                "#[derive(Debug, PartialEq)]\npub struct Cfg {{\n    pub n: u32,\n}}\n\nimpl Drop for Cfg {{\n    fn drop(&mut self) {{{drop_body}\n    }}\n}}\n\npub fn make(x: u32) -> Cfg {{\n    Cfg {{ n: x * 3 }}\n}}\n\n#[test]\nfn makes() {{\n    assert_eq!(make(4), Cfg {{ n: 12 }});\n}}\n"
            );
            let path = PathBuf::from("src/lib.rs");
            let facts = crate::analysis::syntax::ra::summarize_file_with_parser(&path, &source)?;
            let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
                functions: facts.functions.clone(),
                tests: facts.tests.clone(),
                files: [(path, facts)].into_iter().collect(),
                ..Default::default()
            });
            let owner = index
                .functions()
                .iter()
                .find(|function| function.name == "make")
                .ok_or("missing owner")?;
            let test = index
                .tests()
                .iter()
                .find(|test| test.name == "makes")
                .ok_or("missing test")?;
            let probe = Probe {
                id: ProbeId("probe:returned-type-drop".to_string()),
                location: SourceLocation::new("src/lib.rs", 11, 5),
                owner: Some(owner.id.clone()),
                family: ProbeFamily::ReturnValue,
                delta: DeltaKind::Value,
                before: Some("x * 2".to_string()),
                after: Some("x * 3".to_string()),
                expression: "Cfg { n: x * 3 }".to_string(),
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
            let evidence = ClassifiedProbeEvidence::gather(
                &context,
                &probe.expression,
                &crate::analysis::classify::TransitiveReachIndex::new(context.index),
            );
            Ok(evidence.classify(&probe))
        };
        assert_eq!(
            class_for("\n        std::process::exit(0);")?,
            ExposureClass::WeaklyExposed
        );
        assert_eq!(class_for("")?, ExposureClass::Exposed);
        Ok(())
    }

    /// #8480 exit-position narrowing: a non-plain Drop record built only
    /// as a local guard drops inside the owner, so a return_value finding
    /// for the plain record the owner returns stays exposed. The scalar
    /// form of this shape is covered by the
    /// `rust_returned_type_drop_local_guard` fixture.
    #[test]
    fn a_local_non_plain_drop_guard_keeps_return_value_exposed() -> Result<(), String> {
        use crate::domain::ExposureClass;

        let source = "#[derive(Debug, PartialEq)]\npub struct Cfg {\n    pub n: u32,\n}\n\nimpl Drop for Cfg {\n    fn drop(&mut self) {\n    }\n}\n\npub struct Guard {\n    pub n: u32,\n}\n\nimpl Drop for Guard {\n    fn drop(&mut self) {\n        self.n = self.n.wrapping_add(1);\n    }\n}\n\npub fn make(x: u32) -> Cfg { let _guard = Guard { n: 0 };\n    Cfg { n: x * 3 }\n}\n\n#[test]\nfn makes() {\n    assert_eq!(make(4), Cfg { n: 12 });\n}\n";
        let path = PathBuf::from("src/lib.rs");
        let facts = crate::analysis::syntax::ra::summarize_file_with_parser(&path, source)?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: facts.functions.clone(),
            tests: facts.tests.clone(),
            files: [(path, facts)].into_iter().collect(),
            ..Default::default()
        });
        let owner = index
            .functions()
            .iter()
            .find(|function| function.name == "make")
            .ok_or("missing owner")?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "makes")
            .ok_or("missing test")?;
        let probe = Probe {
            id: ProbeId("probe:local-drop-guard".to_string()),
            location: SourceLocation::new("src/lib.rs", 21, 5),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("x * 2".to_string()),
            after: Some("x * 3".to_string()),
            expression: "Cfg { n: x * 3 }".to_string(),
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
        let evidence = ClassifiedProbeEvidence::gather(
            &context,
            &probe.expression,
            &crate::analysis::classify::TransitiveReachIndex::new(context.index),
        );
        assert_eq!(evidence.classify(&probe), ExposureClass::Exposed);
        Ok(())
    }

    /// The memo computes a defeat once per (file, callee) and keeps
    /// different files and callees apart, so a cached answer never leaks
    /// from one test file to another.
    #[test]
    fn file_defeat_memo_computes_once_per_file_and_callee() {
        use std::cell::Cell;
        use std::path::Path;

        let memo = super::FileDefeatMemo::default();
        let calls = Cell::new(0);
        let lookup = |file: &str, callee: &str, answer: bool| {
            super::memoized_file_defeat(&memo, Path::new(file), callee, || {
                calls.set(calls.get() + 1);
                answer
            })
        };

        assert!(lookup("tests/a.rs", "score", true));
        // Cached: the second compute would answer false but never runs.
        assert!(lookup("tests/a.rs", "score", false));
        assert!(!lookup("tests/b.rs", "score", false));
        assert!(!lookup("tests/a.rs", "total", false));
        assert_eq!(calls.get(), 3);
    }

    /// G1 falsifier: two packages each define `expect_response`; the test
    /// lives in package `beta` and calls bare `expect_response` while the
    /// probe's owner is package `alpha`'s — the bare binding is ambiguous
    /// across packages, so the observation stays unverified. Pre-fix this
    /// confirmed through the bare name.
    #[test]
    fn cross_package_same_name_function_defeats_owner_confirmation() {
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![
                same_named_function("crates/alpha/src/lib.rs"),
                same_named_function("crates/beta/src/lib.rs"),
            ],
            ..Default::default()
        });
        let evidence = owner_harness_context(&index, harness_in("crates/beta/tests/protocol.rs"));
        assert_eq!(
            evidence.discriminate.state,
            StageState::Weak,
            "a same-named function in the test's own package must defeat the \
             bare binding: {:#?}",
            evidence.discriminate
        );
        assert!(
            evidence
                .discriminate
                .summary
                .contains("observation_unverified"),
            "the ambiguous binding leaves observation unverified: {:#?}",
            evidence.discriminate
        );
    }

    /// G1 positive control: the same harness in the owner's OWN package
    /// stays confirmed, and an unscopable test path (single-crate
    /// relative form) keeps today's behavior.
    #[test]
    fn same_package_harness_and_unscopable_paths_stay_confirmed() {
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: vec![
                same_named_function("crates/alpha/src/lib.rs"),
                same_named_function("crates/beta/src/lib.rs"),
            ],
            ..Default::default()
        });
        let own_package =
            owner_harness_context(&index, harness_in("crates/alpha/tests/protocol.rs"));
        assert_eq!(
            own_package.discriminate.state,
            StageState::Yes,
            "owner and test in the same package keep the confirmation: {:#?}",
            own_package.discriminate
        );
        let unscopable = owner_harness_context(&index, harness_in("tests/protocol.rs"));
        assert_eq!(
            unscopable.discriminate.state,
            StageState::Yes,
            "a test path without a package scope cannot establish the \
             cross-package ambiguity: {:#?}",
            unscopable.discriminate
        );
    }

    #[test]
    fn evidence_summaries_drop_empty_and_deduplicate_in_sorted_order() {
        let stages = [
            StageEvidence::new(StageState::Yes, Confidence::High, "z evidence"),
            StageEvidence::new(StageState::No, Confidence::Low, ""),
            StageEvidence::new(StageState::Weak, Confidence::Medium, "a evidence"),
            StageEvidence::new(StageState::Yes, Confidence::High, "z evidence"),
        ];

        assert_eq!(
            evidence_summaries(stages.iter()),
            vec!["a evidence".to_string(), "z evidence".to_string()]
        );
    }

    #[test]
    fn gather_retains_digest_valid_propagation_witness_for_internal_consumer() -> Result<(), String>
    {
        let probe = Probe {
            id: ProbeId("probe:fixture:1".to_string()),
            location: SourceLocation::new("src/lib.rs", 10, 2),
            owner: Some(SymbolId("owner:calculate".to_string())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("amount".to_string()),
            after: Some("amount + 1".to_string()),
            expression: "amount".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let owner = FunctionSummary {
            id: SymbolId("owner:calculate".to_string()),
            name: "calculate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 20,
            body: "fn calculate(amount: i32) -> Result<i32, Error> { Ok(amount) }".into(),
            calls: Vec::new(),
            returns: vec![ReturnFact {
                line: 14,
                text: "Ok(amount)".to_string(),
            }],
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        let index = RustIndex::default();
        let context = ProbeContext::new(&probe, Some(&owner), Vec::new(), false, &index, true);
        let evidence = ClassifiedProbeEvidence::gather(
            &context,
            "amount",
            &crate::analysis::classify::TransitiveReachIndex::new(context.index),
        );
        let Some(diagnostic) = evidence.propagation_witness() else {
            return Err("gather discarded the producer witness".to_string());
        };
        if diagnostic.is_invalid() || !diagnostic.witness().digest_matches() {
            return Err("gather retained a stale witness digest".to_string());
        }
        Ok(())
    }

    #[test]
    fn corrupt_digest_is_retained_as_rejected_diagnostic_outcome() -> Result<(), String> {
        let probe = Probe {
            id: ProbeId("probe:fixture:corrupt".to_string()),
            location: SourceLocation::new("src/lib.rs", 10, 2),
            owner: Some(SymbolId("owner:calculate".to_string())),
            family: ProbeFamily::ReturnValue,
            delta: DeltaKind::Value,
            before: Some("amount".to_string()),
            after: Some("amount + 1".to_string()),
            expression: "amount".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let owner = FunctionSummary {
            id: SymbolId("owner:calculate".to_string()),
            name: "calculate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 20,
            body: "fn calculate(amount: i32) -> Result<i32, Error> { Ok(amount) }".into(),
            calls: Vec::new(),
            returns: vec![ReturnFact {
                line: 14,
                text: "Ok(amount)".to_string(),
            }],
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        let index = RustIndex::default();
        let context = ProbeContext::new(&probe, Some(&owner), Vec::new(), false, &index, true);
        let mut evidence = ClassifiedProbeEvidence::gather(
            &context,
            "amount",
            &crate::analysis::classify::TransitiveReachIndex::new(context.index),
        );
        let Some(PropagationWitnessDiagnostic::Valid(witness)) =
            evidence.propagation_witness.as_mut()
        else {
            return Err("expected a valid witness before corruption".to_string());
        };
        witness.semantic_digest = "sha256:corrupt".to_string();
        let finding = build_finding(
            &context,
            crate::domain::ExposureClass::PropagationUnknown,
            evidence,
        );
        if !finding
            .evidence
            .iter()
            .any(|line| line == "propagation witness digest invalid; diagnostic witness withheld")
        {
            return Err("corrupt witness diagnostic was not emitted".to_string());
        }
        Ok(())
    }

    #[test]
    fn owner_local_bindings_exclude_names_the_signature_binds() {
        let binding = |name: &str| LetBindingFact {
            line: 1,
            name: name.to_string(),
        };
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::from_rows".to_string()),
            name: "from_rows".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 6,
            body: "fn from_rows(cache: &mut Cache, rows: Vec<u32>) -> Result<Table, String> {\n    let cache = cache;\n    let table = Table { rows };\n    table.validate()?;\n    Ok(table)\n}".into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: vec![binding("cache"), binding("table"), binding("table")],
            item: Default::default(),
            impl_context: Default::default(),
        };
        assert_eq!(owner_local_binding_names(&owner), vec!["table".to_string()]);
    }

    #[test]
    fn preferred_refusal_note_puts_a_non_limit_owner_call_first() {
        use super::{AssertionRefusalNote, preferred_refusal_note};
        let note = |name: &str, calls_owner, is_analyzer_limit| AssertionRefusalNote {
            location: name.to_string(),
            reason: String::new(),
            calls_owner,
            is_analyzer_limit,
        };
        let pick = |notes: Vec<AssertionRefusalNote>| {
            preferred_refusal_note(notes).map(|note| note.location)
        };
        // #6903: a limit refusal listed first must not front for the
        // `if flag { assert_eq!(owner(..), ..) }` that keeps the gap.
        assert_eq!(
            pick(vec![
                note("unrelated", false, false),
                note("limit", true, true),
                note("branch", true, false),
            ]),
            Some("branch".to_string())
        );
        assert_eq!(
            pick(vec![
                note("unrelated", false, false),
                note("limit", true, true)
            ]),
            Some("limit".to_string())
        );
        // Without an owner call the first refusal in test order is kept.
        assert_eq!(
            pick(vec![
                note("first", false, true),
                note("second", false, false)
            ]),
            Some("first".to_string())
        );
        assert_eq!(pick(Vec::new()), None);
    }

    // --- #5830 review (A1): the caller walk names the owner by syntax ---

    fn caller_names_for(source: &str, owner: &str) -> Result<Vec<String>, String> {
        use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let path = PathBuf::from("src/lib.rs");
        let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
        let functions = facts.functions.clone();
        let mut index = RustIndex::default();
        index.insert_file_only(path, facts);
        index.extend_functions(functions);
        let owner = index
            .functions()
            .iter()
            .find(|function| function.id.0.ends_with(owner))
            .cloned()
            .ok_or_else(|| format!("owner {owner} indexed"))?;
        Ok(super::transitive_caller_names(&owner, &index)
            .into_iter()
            .map(|(_, name)| name)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    #[test]
    fn caller_walk_follows_qualified_chains_and_skips_foreign_same_names() -> Result<(), String> {
        let source = "pub struct Stack { items: Vec<u32> }\n\
            impl Stack {\n\
                pub fn len(&self) -> usize { self.items.len() }\n\
                pub fn is_empty(&self) -> bool { self.len() == 0 }\n\
                pub fn fresh() -> usize { Self::len(&Stack { items: vec![] }) }\n\
            }\n\
            pub struct Other;\n\
            impl Other { pub fn peek(&self, s: &Stack) -> bool { self.len(s) } fn len(&self, _s: &Stack) -> bool { true } }\n\
            pub fn count_items(values: &[u32]) -> usize { values.len() }\n\
            pub fn std_len(values: &[u32]) -> usize { core::primitive::slice::len(values) }\n\
            pub fn typed(stack: &Stack) -> usize { Stack::len(stack) }\n\
            pub fn depth_two(stack: &Stack) -> usize { typed(stack) + 1 }\n\
            pub fn depth_three(stack: &Stack) -> usize { crate::depth_two(stack) }\n\
            pub fn cycle_a(stack: &Stack) -> usize { cycle_b(stack) + typed(stack) }\n\
            pub fn cycle_b(stack: &Stack) -> usize { cycle_a(stack) }\n";
        assert_eq!(
            caller_names_for(source, "Stack::len")?,
            [
                "cycle_a",
                "cycle_b",
                "depth_three",
                "depth_two",
                "fresh",
                "is_empty",
                "typed"
            ],
            "Stack::len, Self::len and self.len() in Stack reach the owner, chains \
             and cycles close transitively; values.len(), core::...::len and \
             self.len() inside another impl do not"
        );
        Ok(())
    }

    #[test]
    fn caller_walk_names_a_free_owner_by_bare_or_module_path_only() -> Result<(), String> {
        let source = "pub fn tax(subtotal: i64) -> i64 { subtotal * 8 / 100 }\n\
            pub fn reference(subtotal: i64) -> i64 { tax(subtotal) }\n\
            pub fn pathed(subtotal: i64) -> i64 { crate::tax(subtotal) }\n\
            pub struct Rate;\n\
            impl Rate { pub fn tax(&self) -> i64 { 0 } pub fn apply(&self) -> i64 { self.tax() } }\n\
            pub fn typed() -> i64 { Rate::tax(&Rate) }\n\
            pub fn std_rooted(v: &mut Vec<i64>) -> i64 { std::mem::take(v).len() as i64 }\n\
            pub fn returned(v: i64) -> i64 { return tax(v) }\n\
            pub fn matched(v: i64) -> i64 { match tax(v) { t => t } }\n\
            pub struct Out { t: i64 }\n\
            pub fn labelled(v: i64) -> Out { Out { t: tax(v) } }\n";
        assert_eq!(
            caller_names_for(source, "src/lib.rs::tax")?,
            ["labelled", "matched", "pathed", "reference", "returned"],
            "a free owner is reached by a bare or crate-path call, never by a \
             method call or a type-qualified call of the same name"
        );
        Ok(())
    }

    #[test]
    fn owner_caller_memo_agrees_with_the_walk_and_never_crosses_indexes() -> Result<(), String> {
        use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        fn indexed_source(source: &str) -> Result<RustIndex, String> {
            let path = PathBuf::from("src/lib.rs");
            let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
            let functions = facts.functions.clone();
            let mut index = RustIndex::default();
            index.insert_file_only(path, facts);
            index.extend_functions(functions);
            Ok(index)
        }
        // Borrowed from the arena, like `resolve_owner_function` in
        // production: a clone is not an arena element, so the memo's
        // pointer-based slot key would never resolve and the test would
        // exercise compute-fresh twice instead of a cache hit.
        fn find_owner<'index>(
            index: &'index RustIndex,
            owner: &str,
        ) -> Result<&'index FunctionSummary, String> {
            index
                .functions()
                .iter()
                .find(|function| function.id.0.ends_with(owner))
                .ok_or_else(|| format!("owner {owner} indexed"))
        }
        let source = "pub fn tax(subtotal: i64) -> i64 { subtotal * 8 / 100 }\n\
            pub fn reference(subtotal: i64) -> i64 { tax(subtotal) }\n";
        let index = indexed_source(source)?;
        let owner = find_owner(&index, "src/lib.rs::tax")?;
        let memo = super::OwnerCallerNames::default();
        let direct = super::transitive_caller_names(owner, &index);
        assert!(!direct.is_empty(), "the fixture owner has callers");
        assert_eq!(
            memo.callers_for(&index, owner),
            direct,
            "the first memo query walks and caches"
        );
        assert_eq!(
            memo.slots_for_test().borrow().len(),
            1,
            "the first query populated the cache"
        );
        // The repeat query must hit: the walk counter does not move.
        assert_eq!(memo.walks_for_test(), 1, "the first query walked once");
        assert_eq!(
            memo.callers_for(&index, owner),
            direct,
            "the repeat query serves the cached set"
        );
        assert_eq!(memo.walks_for_test(), 1, "the repeat query hit the cache");
        // The borrow-scoped predicate agrees with the walked set.
        for (ty, caller) in &direct {
            assert!(
                memo.caller_reaches(&index, owner, ty.as_deref(), caller),
                "the predicate finds walked caller {caller}"
            );
            assert!(
                !memo.caller_reaches(&index, owner, Some("NotTheType"), caller),
                "the predicate honors the type filter for {caller}"
            );
        }
        assert!(
            !memo.caller_reaches(&index, owner, None, "no_such_caller"),
            "the predicate misses a name the walk never found"
        );
        let lonely_source = "pub fn tax(subtotal: i64) -> i64 { subtotal * 8 / 100 }\n";
        let lonely_index = indexed_source(lonely_source)?;
        let lonely_owner = find_owner(&lonely_index, "src/lib.rs::tax")?;
        assert_eq!(
            memo.callers_for(&lonely_index, lonely_owner),
            super::transitive_caller_names(lonely_owner, &lonely_index),
            "a query against another index computes fresh instead of \
             serving the first index's cached set"
        );
        Ok(())
    }

    #[test]
    fn caller_walk_ignores_owner_names_in_comments_and_strings_on_the_call_line()
    -> Result<(), String> {
        let source = "pub struct Stack { items: Vec<u32> }\n\
            impl Stack { pub fn len(&self) -> usize { self.items.len() } }\n\
            pub fn count_items(values: &[u32]) -> usize { values.len() } // Stack::len()\n\
            pub fn labelled(values: &[u32]) -> (usize, &'static str) { (values.len(), \"Stack::len()\") }\n\
            pub fn typed(stack: &Stack) -> usize { Stack::len(stack) }\n";
        assert_eq!(
            caller_names_for(source, "Stack::len")?,
            ["typed"],
            "a `Stack::len()` inside a comment or string on the line of an \
             unrelated `values.len()` call does not reach the owner"
        );
        Ok(())
    }

    #[test]
    fn owner_parameters_are_read_from_a_multiline_signature() -> Result<(), String> {
        use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let source = "pub fn tax(\n    subtotal: i64,\n    mut rate: i64,\n) -> i64 {\n    rate += 0;\n    subtotal * rate / 100\n}\n\
            pub struct Input { pub base: i64, pub fee: i64 }\n\
            pub fn levy<F: Fn(i64) -> i64>(Input { base, fee: charge }: Input, f: F) -> i64 { f(base) + charge }\n";
        let facts = RaRustSyntaxAdapter.summarize_file(&PathBuf::from("src/lib.rs"), source)?;
        let owner = facts
            .functions
            .iter()
            .find(|function| function.name == "tax")
            .ok_or("tax indexed")?;
        assert_eq!(
            super::owner_parameter_names(owner),
            ["rate", "subtotal"],
            "parameters on lines after `fn tax(` are owner-scoped tokens"
        );
        let levy = facts
            .functions
            .iter()
            .find(|function| function.name == "levy")
            .ok_or("levy indexed")?;
        assert_eq!(
            super::owner_parameter_names(levy),
            ["base", "charge", "f", "fee"],
            "a destructuring pattern binds its names; generic `Fn(..)` bounds are skipped"
        );
        Ok(())
    }

    #[test]
    fn caller_walk_stops_after_six_hops() -> Result<(), String> {
        let mut source = "pub fn tax(subtotal: i64) -> i64 { subtotal * 8 / 100 }\n".to_string();
        let mut previous = "tax".to_string();
        for hop in 1..=7 {
            source.push_str(&format!(
                "pub fn hop{hop}(v: i64) -> i64 {{ {previous}(v) }}\n"
            ));
            previous = format!("hop{hop}");
        }
        assert_eq!(
            caller_names_for(&source, "src/lib.rs::tax")?,
            ["hop1", "hop2", "hop3", "hop4", "hop5", "hop6"],
            "callers six hops out reach the owner; the seventh hop does not"
        );
        Ok(())
    }
    #[test]
    fn gather_retains_selected_call_identity_without_completing_ledger_path() -> Result<(), String>
    {
        use crate::analysis::classify::resolve_chain;
        use crate::domain::{
            DeltaKind, Probe, ProbeFamily, ProbeId, RelationReason, SourceLocation, StageState,
        };
        let source = "struct PolicyError { detail: String }\nstruct Config { errors: Vec<PolicyError> }\nfn record(errors: &mut Vec<PolicyError>) {\n    errors.push(PolicyError { detail: \"diagnostic\".to_owned() });\n}\nfn collect(flag: bool) -> Vec<PolicyError> { let mut errors = Vec::new(); record(&mut errors); errors }\nimpl Config { fn parse(flag: bool) -> Self { let errors = collect(flag); Self { errors } } }\nfn sibling() { let ignored = collect(false); }\n#[test]\nfn direct_test() { let ignored = collect(false); }\n#[test]\nfn observes() { let malformed = Config::parse(true); check_detail(malformed); }\nfn check_detail(config: Config) { if config.errors[0].detail != \"diagnostic\" { std::panic::panic_any(\"wrong detail\"); } }\n";
        let path = PathBuf::from("src/lib.rs");
        let facts = crate::analysis::syntax::ra::summarize_file_with_parser(&path, source)?;
        let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
            functions: facts.functions.clone(),
            tests: facts.tests.clone(),
            files: [(path, facts)].into_iter().collect(),
            ..Default::default()
        });
        let owner = index
            .functions()
            .iter()
            .find(|function| function.name == "record")
            .ok_or("missing owner")?;
        let direct = index
            .tests()
            .iter()
            .find(|test| test.name == "direct_test")
            .ok_or("missing direct test")?;
        let chain = resolve_chain("record", &index, true, &[]);
        assert_eq!(chain.hops.len(), 1);
        let probe = Probe {
            id: ProbeId("probe:selected-ledger-caller".into()),
            location: SourceLocation::new("src/lib.rs", 4, 1),
            owner: Some(owner.id.clone()),
            family: ProbeFamily::FieldConstruction,
            delta: DeltaKind::Value,
            before: None,
            after: None,
            expression: "detail: \"diagnostic\".to_owned()".into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        };
        let context = ProbeContext::new(
            &probe,
            Some(owner),
            vec![(direct, RelationReason::HelperOwnerCall)],
            false,
            &index,
            true,
        )
        .with_helper_chain(Some(chain));
        let evidence = ClassifiedProbeEvidence::gather(
            &context,
            &probe.expression,
            &crate::analysis::classify::TransitiveReachIndex::new(context.index),
        );
        let diagnostic = evidence
            .propagation_witness()
            .ok_or("missing retained witness")?;
        assert!(!diagnostic.is_invalid());
        let witness = diagnostic.witness();
        assert_eq!(witness.selected_helper_calls.len(), 1);
        assert_eq!(witness.selected_helper_calls[0].test_name, "observes");
        assert_eq!(
            witness.selected_helper_calls[0].callee.0,
            "src/lib.rs::impl Config::parse"
        );
        assert_eq!(
            serde_json::to_value(witness).map_err(|error| error.to_string())?["completeness"],
            "partial"
        );
        assert!(
            serde_json::to_value(witness).map_err(|error| error.to_string())?["edges"]
                .as_array()
                .ok_or("missing edges")?
                .iter()
                .any(|edge| edge["status"] == "candidate")
        );
        assert_eq!(evidence.propagate.state, StageState::Unknown);
        assert!(witness.digest_matches());
        let serialized = serde_json::to_string(witness).map_err(|error| error.to_string())?;
        let round_trip: crate::analysis::classify::PropagationWitnessV1 =
            serde_json::from_str(&serialized).map_err(|error| error.to_string())?;
        assert_eq!(round_trip, *witness);
        let mut forged = witness.clone();
        forged.selected_helper_calls[0].callee = SymbolId("unrelated::parse".into());
        assert!(
            !forged.digest_matches(),
            "callee identity must be bound to semantic digest"
        );
        Ok(())
    }

    #[test]
    fn trailing_path_segments_steps_past_a_multi_byte_separator() {
        assert_eq!(trailing_path_segments("x→Stack::new"), vec!["Stack", "new"]);
    }
}
