use crate::analysis::classify::{
    ARM_UNSELECTED_REASON_PREFIX, ASSERTION_CONTEXT_UNESTABLISHED, ArmSelector, EffectStateCarrier,
    HELPER_RESULT_NOT_FORWARDED, OwnerPinSyntax, OwnerReturnPin, ProbeContext,
    PropagationWitnessV1, ReturnOracleAdmission, TransitiveReachIndex, WrapperEntryPairing,
    activation_and_boundary_input, body_contains_owner_call, callee_is_unique,
    chain_forwards_to_observed_hops, chain_passes_effect_target_to_observed_hops, classify,
    confidence_score, contains_as_whole_word, current_path_witness,
    has_same_test_boundary_oracle_pairing, helper_only_reach,
    infection_evidence_with_boundary_input, local_flow_sinks, operand_only_pin,
    oracle_crediting_relations, owner_may_be_reached_unseen, package_prefix,
    propagation_evidence_with_witness, reach_evidence, reveal_outcome,
    same_test_pairing_missing_summary, signature_parameters,
};
use crate::analysis::facts::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::domain::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod side_flip;
mod tuple_match;

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

impl ClassifiedProbeEvidence {
    pub(in crate::analysis) fn gather(context: &ProbeContext<'_>, reveal_expression: &str) -> Self {
        let test_summaries = context.related_test_summaries();
        // Without a resolved owner (a line in a `macro_rules!` template, an
        // impl the index did not attribute) there is no name to search for,
        // so nothing rules reach out.
        let reach = reach_evidence(&context.related_tests, context.owner_fn, || {
            context
                .owner_fn
                .is_none_or(|owner| owner_may_be_reached_unseen(owner, context.index))
        });
        let flow_sinks = local_flow_sinks(context.probe, context.owner_fn);
        let propagation_witness = current_path_witness(context.probe, &flow_sinks)
            .map(PropagationWitnessDiagnostic::from_witness);
        let gathered = activation_and_boundary_input(
            context.probe,
            context.owner_fn,
            &test_summaries,
            &flow_sinks,
            context.helper_chain.as_ref(),
            context.index,
            context.workspace_complete,
            context.test_value_facts,
        );
        let mut activation = gathered.activation;
        // #3731 review (F11, G1): the changed owner's package scope, computed
        // once — the cross-package same-name defeats compare each related
        // test's package against it.
        let owner_package = context
            .owner_fn
            .and_then(|owner| package_prefix(&owner.file));
        // RIPR-SPEC-0229: an unselected-arm discriminator reads each related
        // test's owner calls as inputs to the changed owner. A test whose
        // file imports a foreign same-named function, or whose own package
        // defines one, may be calling that function instead, so the same
        // identity defeats reveal applies withhold the named arm here.
        if let Some(owner) = context.owner_fn
            && activation
                .missing_discriminators
                .iter()
                .any(|fact| fact.reason.starts_with(ARM_UNSELECTED_REASON_PREFIX))
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
                .retain(|fact| !fact.reason.starts_with(ARM_UNSELECTED_REASON_PREFIX));
        }
        let infect = infection_evidence_with_boundary_input(
            context.probe,
            &test_summaries,
            &activation,
            gathered.unresolved_boundary.as_deref(),
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
                    && if effect_family {
                        !chain_passes_effect_target_to_observed_hops(
                            owner,
                            &context.probe.expression,
                            chain,
                            &context.related_tests,
                        )
                    } else {
                        !chain_forwards_to_observed_hops(&owner.name, chain, &context.related_tests)
                    } =>
            {
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
        let fallback_pin_syntax = OwnerPinSyntax::default();
        let pin_syntax = context.owner_pin_syntax.unwrap_or(&fallback_pin_syntax);
        // Every diff-classifier consumer of a covered equality uses this same
        // decision. Keep the original TestSummary and assertion cardinality;
        // filtering a clone would manufacture singleton matching fallbacks.
        let assertion_admitted = |test: &TestSummary, assertion: &OracleFact| {
            pin_syntax.admits_equality_assertion(context.probe, test, assertion, context.index)
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
                selector.in_workspace(
                    context.index,
                    context
                        .related_tests
                        .iter()
                        .map(|(test, _)| test.file.as_path()),
                )
            });
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
        // Built lazily: only a match arm beside an owner-calling test asks
        // whether a same-file test may run the owner (#6297).
        let proximity_reach = TransitiveReachIndex::new(context.index);
        let owner_reach = std::cell::OnceCell::new();
        let owner_locals = context
            .owner_fn
            .map(owner_local_binding_names)
            .unwrap_or_default();
        let owner_parameters = context
            .owner_fn
            .map(owner_parameter_names)
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
            owner_return_pin.as_ref().is_some_and(|pin| {
                pin.admits(
                    test,
                    assertion,
                    context.index,
                    &|file, name| {
                        context.index.files().get(file).is_some_and(|facts| {
                            context.test_file_imports_foreign_callee_name(file, &facts.source, name)
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
                proximity_may_reach_owner: &|test| {
                    context.owner_fn.is_none_or(|owner| {
                        owner_reach
                            .get_or_init(|| proximity_reach.owner_reach(&owner.name))
                            .test_may_reach(test)
                    })
                },
                owner_parameters: &owner_parameters,
                expected_reaches_owner: &expected_reaches_owner,
                effect_state_carried: &|test, assertion| {
                    effect_carrier
                        .as_ref()
                        .is_none_or(|carrier| carrier.admits(test, assertion))
                },
            },
            arm_selector.as_ref(),
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
        if matches!(context.probe.family, ProbeFamily::FieldConstruction)
            && reveal.owner_pin_credited
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
                context
                    .helper_chain
                    .as_ref()
                    .map(|chain| WrapperEntryPairing {
                        chain,
                        test_activation: &one_test_activation,
                    }),
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
        // to the same value: the field equals that operand there.
        let discriminate = if matches!(context.probe.family, ProbeFamily::FieldConstruction)
            && discriminate.state == StageState::Yes
            && let Some(owner) = context.owner_fn
            && let Some(pin) = operand_only_pin(
                &context.probe.expression,
                &owner.name,
                owner.body.as_str(),
                &context.related_tests,
            ) {
            StageEvidence::new(StageState::Weak, Confidence::Medium, pin.summary())
        } else {
            discriminate
        };
        // Tests kept only as suggested locations never run the owner, so
        // they neither activate the change nor observe it.
        let reach_ruled_out = reach.state == StageState::No
            && context
                .owner_fn
                .is_some_and(|owner| !owner_may_be_reached_unseen(owner, context.index));
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
        // Disclose a refused related `assert_eq!` whenever the refusal can
        // matter: the reveal is not fully established. See
        // `preferred_refusal_note` for which one.
        let owner_name = context.owner_fn.map_or("", |owner| owner.name.as_str());
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
                                "`assert_eq!` in {} at {}:{}",
                                test.name,
                                test.file.display(),
                                assertion.line
                            ),
                            reason: refusal.describe(),
                            calls_owner,
                            is_analyzer_limit: refusal.is_analyzer_limit(),
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
            refusals
                .next()
                .is_some_and(|first| first.is_analyzer_limit())
                && refusals.all(|refusal| refusal.is_analyzer_limit())
        };

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
        }
    }

    pub(in crate::analysis) fn classify(&self, probe: &Probe) -> ExposureClass {
        // A changed line inside a function no test reaches has no static path
        // whatever its shape: "cannot classify, escalate" would send the
        // reader after mutation testing when the plain gap is a missing test.
        if self.reach_ruled_out {
            return ExposureClass::NoStaticPath;
        }
        classify(
            &self.reach,
            &self.infect,
            &self.propagate,
            &self.observe,
            &self.discriminate,
            probe,
        )
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
    index_identity: Cell<Option<(usize, usize, u64)>>,
    by_owner_slot: RefCell<OwnerCallerSets>,
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
        let callers = transitive_caller_names(owner, index);
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
        let callers = transitive_caller_names(owner, index);
        self.by_owner_slot
            .borrow_mut()
            .insert(slot, callers.clone());
        callers
    }

    /// The cache cells, so tests can hold a shared borrow across a repeat
    /// query: a hit only borrows, while a recompute's `borrow_mut` panics.
    #[cfg(test)]
    pub(in crate::analysis) fn slots_for_test(&self) -> &std::cell::RefCell<OwnerCallerSets> {
        &self.by_owner_slot
    }

    fn slot_key(&self, index: &RustIndex, owner: &FunctionSummary) -> Option<usize> {
        let identity = index.storage_identity();
        match self.index_identity.get() {
            None => self.index_identity.set(Some(identity)),
            Some(bound) if bound != identity => return None,
            Some(_) => {}
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
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .map_or(0, |at| at + 1);
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

#[cfg(test)]
mod tests {
    use super::evidence_summaries;
    use super::owner_local_binding_names;
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
        )
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
        let evidence = ClassifiedProbeEvidence::gather(&context, "amount");
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
        let mut evidence = ClassifiedProbeEvidence::gather(&context, "amount");
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
        // The repeat query must hit: a shared borrow is held across it, so
        // a recompute's `borrow_mut` panics instead of silently re-walking.
        {
            let _guard = memo.slots_for_test().borrow();
            assert_eq!(
                memo.callers_for(&index, owner),
                direct,
                "the repeat query serves the cached set"
            );
        }
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
}
