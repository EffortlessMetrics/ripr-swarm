//! Reuse diff's same-test boundary/oracle gate for repository tests.
use super::{CompactGripContext, FunctionSummary, RepoSeam, TestSummary};
use crate::analysis::classify::{
    OwnerPinSyntax, OwnerReturnPin, ProbeContext, WrapperEntryPairing,
    activation_and_boundary_input, has_same_test_boundary_oracle_pairing,
    has_wrapper_boundary_oracle_pairing, helper_pins_owner_call, targeted_free_call_reaches_owner,
};
use crate::domain::{DeltaKind, Probe, ProbeFamily, ProbeId, SourceLocation};

pub(super) fn missing(
    seam: &RepoSeam,
    owner: Option<&FunctionSummary>,
    tests: &[&TestSummary],
    context: &CompactGripContext<'_>,
    syntax: &OwnerPinSyntax,
) -> bool {
    let index = context.index;
    let Some(owner) = owner
        .filter(|owner| owner.file == seam.file() && owner.id.0.replace('\\', "/") == seam.owner())
    else {
        return true;
    };
    let probe = Probe {
        id: ProbeId(format!("repo-boundary-pairing:{}", seam.id().as_str())),
        location: SourceLocation::new(seam.file().to_path_buf(), seam.display_line(), 1),
        owner: Some(owner.id.clone()),
        family: ProbeFamily::Predicate,
        delta: DeltaKind::Unknown,
        before: None,
        after: Some(seam.expression().to_string()),
        expression: seam.expression().to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    };
    let chain = context.boundary_chain(owner);
    let activation = activation_and_boundary_input(
        &probe,
        Some(owner),
        tests,
        &[],
        chain.as_ref(),
        index,
        false,
        None,
    );
    // An unresolved aggregate may contain a different call's failure.
    // Direct pairing remains withheld; a separately proven wrapper can only
    // pair through its own resolved activation below.
    let unresolved = activation.unresolved_boundary.is_some();
    let direct_activation = if unresolved {
        Default::default()
    } else {
        activation.activation
    };
    let probe_context = ProbeContext::new(&probe, Some(owner), Vec::new(), false, index, false);
    let entry_pin = std::cell::OnceCell::new();
    let entry_admitted =
        |test: &TestSummary, assertion: &crate::analysis::rust_index::OracleFact| {
            entry_pin
                .get_or_init(|| {
                    chain
                        .as_ref()
                        .and_then(|chain| chain.hops.last())
                        .and_then(|hop| {
                            OwnerReturnPin::establish_forwarding_entry(&probe, &hop.caller, index)
                        })
                })
                .as_ref()
                .is_some_and(|pin| {
                    pin.admits(
                        test,
                        assertion,
                        index,
                        &|file, name| {
                            index.files().get(file).is_none_or(|facts| {
                                probe_context.test_file_imports_foreign_callee_name(
                                    file,
                                    &facts.source,
                                    name,
                                )
                            })
                        },
                        syntax,
                    )
                })
        };
    let boolean_pin = super::BooleanBoundaryPin::establish(seam, Some(owner), index);
    let one_test_activation = |test: &TestSummary| {
        let entry = chain.as_ref().and_then(|chain| chain.hops.last());
        if !entry.is_some_and(|hop| {
            targeted_free_call_reaches_owner(test, &hop.caller, None, index, syntax)
        }) {
            return Default::default();
        }
        // A direct call elsewhere in this test must not suppress the
        // independently proven wrapper row. Keep the original test for
        // assertion admission and identity; only the activation input's
        // call facts are narrowed to the wrapper entry.
        let mut entry_only = (*test).clone();
        entry_only.calls.retain(|call| call.name != owner.name);
        let result = activation_and_boundary_input(
            &probe,
            Some(owner),
            &[&entry_only],
            &[],
            chain.as_ref(),
            index,
            false,
            None,
        );
        if result.unresolved_boundary.is_some() {
            Default::default()
        } else {
            result.activation
        }
    };
    let loan = |test: &TestSummary, assertion: &crate::analysis::rust_index::OracleFact| {
        let facts = index.files().get(&test.file)?;
        if probe_context.test_file_imports_foreign_callee_name(
            &test.file,
            &facts.source,
            &owner.name,
        ) {
            return None;
        }
        if let (Some(test_package), Some(owner_package)) = (
            super::related_tests::package_prefix(&test.file),
            super::related_tests::package_prefix(&owner.file),
        ) && test_package != owner_package
            && index.functions().iter().any(|function| {
                function.name == owner.name
                    && super::related_tests::package_prefix(&function.file).as_deref()
                        == Some(test_package.as_str())
            })
        {
            return None;
        }
        syntax.helper_loan(test, assertion, index).filter(|loan| {
            helper_pins_owner_call(
                &loan.helper,
                &super::strip_comments_and_strings(&test.body),
                &owner.name,
            )
        })
    };

    if unresolved {
        return !chain.as_ref().is_some_and(|chain| {
            has_wrapper_boundary_oracle_pairing(
                owner,
                tests,
                WrapperEntryPairing {
                    chain,
                    test_activation: &one_test_activation,
                    entry_admitted: &entry_admitted,
                    entry_loan: &loan,
                },
                &|test, assertion| syntax.admits_equality_assertion(&probe, test, assertion, index),
                index,
            )
        });
    }

    !has_same_test_boundary_oracle_pairing(
        &probe,
        Some(owner),
        tests,
        &direct_activation,
        &|test, assertion| syntax.admits_equality_assertion(&probe, test, assertion, index),
        &|test, assertion| {
            boolean_pin
                .as_ref()
                .is_some_and(|pin| pin.admits(test, assertion, index, syntax))
        },
        &loan,
        chain.as_ref().map(|chain| WrapperEntryPairing {
            chain,
            test_activation: &one_test_activation,
            entry_admitted: &entry_admitted,
            entry_loan: &loan,
        }),
        index,
    )
}
