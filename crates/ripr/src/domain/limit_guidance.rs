//! Classification-keyed next steps for named static limitations and stop
//! reasons.
//!
//! `describe()` on [`StaticLimitKind`] and [`StopReason`] says what ripr could
//! not resolve. `next_step()` says what the reader can do about it, so no
//! limitation is left as a bare diagnosis. Every variant is matched
//! exhaustively: a new limitation or stop reason does not compile until it has
//! guidance, and the tests below reject empty, duplicate and non-imperative
//! text.
//!
//! These are static-evidence suggestions. None claims that the suggested test
//! exists, would pass, or would change the classification.

use super::{StaticLimitKind, StopReason};

impl StaticLimitKind {
    /// Every kind, so tests can walk the vocabulary. The ordinal test keeps
    /// this list in step with the enum.
    #[cfg(test)]
    pub(crate) const ALL: [StaticLimitKind; 29] = [
        StaticLimitKind::DynamicDispatch,
        StaticLimitKind::Metaprogramming,
        StaticLimitKind::MissingImportGraph,
        StaticLimitKind::DecoratorIndirection,
        StaticLimitKind::MockedModule,
        StaticLimitKind::OpaqueCustomAssertionHelper,
        StaticLimitKind::PropertyBasedTest,
        StaticLimitKind::UnresolvedPytestFixture,
        StaticLimitKind::UnsupportedSyntax,
        StaticLimitKind::CrossLanguageOracleVisibilityUnresolved,
        StaticLimitKind::RustTransitiveReachUnresolved,
        StaticLimitKind::RustIntegrationPublicApiPathUnresolved,
        StaticLimitKind::RustMacroReachUnresolved,
        StaticLimitKind::RustMacroWrappedTestCallUnresolved,
        StaticLimitKind::RustMacroWrappedAssertionUnresolved,
        StaticLimitKind::RustValuePropagationUnresolved,
        StaticLimitKind::RustFieldMatchSelectorUnresolved,
        StaticLimitKind::RustSubprocessBinaryReachUnresolved,
        StaticLimitKind::WrapperErrorBindingUnresolved,
        StaticLimitKind::PythonTransitiveReachUnresolved,
        StaticLimitKind::RustAssertionContextUnresolved,
        StaticLimitKind::RustLogCaptureUnresolved,
        StaticLimitKind::RustPanicOutcomeUnresolved,
        StaticLimitKind::RustDebugAssertionProfileUnresolved,
        StaticLimitKind::RustCallFactBudget,
        StaticLimitKind::RustFrameworkOracleUnresolved,
        StaticLimitKind::RustPinnedShapeUnresolved,
        StaticLimitKind::RustImplicitCallUnresolved,
        StaticLimitKind::RustCallerLoopSelectorUnresolved,
    ];

    /// What to do so ripr can settle this limitation. One imperative
    /// sentence naming the concrete test shape or setting.
    pub(crate) fn next_step(&self) -> &'static str {
        match self {
            StaticLimitKind::DynamicDispatch => {
                "Add a test that calls the changed owner on the concrete type, not through a \
                 trait object or `dyn` value, and assert on its result."
            }
            StaticLimitKind::Metaprogramming => {
                "Call the changed owner directly from a test instead of through the macro or \
                 generated code, and assert on its result."
            }
            StaticLimitKind::MissingImportGraph => {
                "Import the owner in the test by a static, absolute path (not a dynamic, \
                 computed or unresolved relative import) so ripr can connect the test to it."
            }
            StaticLimitKind::DecoratorIndirection => {
                "Test the undecorated function directly, and assert on the decorated result with \
                 an exact value so a wrong result fails."
            }
            StaticLimitKind::MockedModule => {
                "Add a test that runs the real implementation, with no mock replacing the \
                 changed module, and assert on its result."
            }
            StaticLimitKind::OpaqueCustomAssertionHelper => {
                "Assert inline on the changed value with the framework's own assertion \
                 (`assert_eq!`, `assert`, `expect`), or make the helper call one ripr can see."
            }
            StaticLimitKind::PropertyBasedTest => {
                "Add one example-based test with a concrete input at the changed boundary and an \
                 exact assertion; ripr cannot see the inputs a property test generates."
            }
            StaticLimitKind::UnresolvedPytestFixture => {
                "Define the fixture by a static name in the test file or its `conftest.py`, or \
                 pass the input value inline in the test."
            }
            StaticLimitKind::UnsupportedSyntax => {
                "Move the changed logic into a plain function and test that function, or report \
                 the syntax in an issue so ripr can model it."
            }
            StaticLimitKind::CrossLanguageOracleVisibilityUnresolved => {
                "Check that the external-language test (the binding or TypeScript test) asserts \
                 the changed behavior; adding a same-language test does not settle this."
            }
            StaticLimitKind::RustTransitiveReachUnresolved => {
                "Call the changed owner directly from a test, or shorten the helper chain to \
                 five calls or fewer, so ripr can trace the test to the change."
            }
            StaticLimitKind::RustIntegrationPublicApiPathUnresolved => {
                "Call the changed owner directly from a test, or assert on its result through \
                 the public API in the same test body so the path is visible."
            }
            StaticLimitKind::RustMacroReachUnresolved => {
                "Call the changed owner directly from a plain `#[test]` body instead of through \
                 the macro, and assert on its result."
            }
            StaticLimitKind::RustMacroWrappedTestCallUnresolved => {
                "Call the changed owner from the `#[test]` body itself, outside the macro, and \
                 assert on its result."
            }
            StaticLimitKind::RustMacroWrappedAssertionUnresolved => {
                "Replace the assertion-like macro with `assert_eq!` or `assert!` on the changed \
                 value, or add one beside it."
            }
            StaticLimitKind::RustValuePropagationUnresolved => {
                "Assert the changed value itself with `assert_eq!`, rather than only a string or \
                 character operation derived from it."
            }
            StaticLimitKind::RustFieldMatchSelectorUnresolved => {
                "Call the owner with a literal value for the selecting field and `assert_eq!` on \
                 its return value."
            }
            StaticLimitKind::RustSubprocessBinaryReachUnresolved => {
                "Add a unit or library-level test that calls the changed owner directly; ripr \
                 cannot map the spawned binary back to it."
            }
            StaticLimitKind::WrapperErrorBindingUnresolved => {
                "Assert the specific error variant the callee returns (`matches!` or a `match` \
                 arm), not only that an error occurred."
            }
            StaticLimitKind::PythonTransitiveReachUnresolved => {
                "Call the changed method directly on an instance of its class in a test and \
                 assert on the result."
            }
            StaticLimitKind::RustAssertionContextUnresolved => {
                "Add an ungated test that uses the standard assertion macro on the changed value, \
                 and remove any glob import that rebinds that macro."
            }
            StaticLimitKind::RustLogCaptureUnresolved => {
                "Assert on the return value or a returned error in addition to the captured log \
                 records."
            }
            StaticLimitKind::RustPanicOutcomeUnresolved => {
                "Pin the panic with `#[should_panic(expected = \"...\")]` naming the message, or \
                 assert on a returned value, so a panic from an earlier call cannot pass it."
            }
            StaticLimitKind::RustDebugAssertionProfileUnresolved => {
                "Use `assert_eq!` instead of `debug_assert_eq!` in the test, so the check also \
                 runs under `cargo test --release`."
            }
            StaticLimitKind::RustCallFactBudget => {
                "Split the test line with many calls into separate statements so each call and \
                 assertion is visible to ripr."
            }
            StaticLimitKind::RustFrameworkOracleUnresolved => {
                "Add a direct `assert_eq!` on the changed value beside the environment, mock, \
                 expect-test or snapshot check."
            }
            StaticLimitKind::RustPinnedShapeUnresolved => {
                "Move the changed code out of the closure, returned-value statement or \
                 `let ... else` exit into a directly returned form, so the test's exact \
                 `assert_eq!` pins it."
            }
            StaticLimitKind::RustImplicitCallUnresolved => {
                "Call the changed method explicitly by its fully qualified path in the test, \
                 instead of relying on an operator, `Drop`, `Deref` or other implicit trait \
                 call, and assert on its result."
            }
            StaticLimitKind::RustCallerLoopSelectorUnresolved => {
                "Add a test that drives the iterator with one concrete input and asserts the \
                 changed arm's item directly, rather than only comparing the collected sequence."
            }
        }
    }
}

impl StopReason {
    /// Every reason, for the same walk as [`StaticLimitKind::ALL`].
    #[cfg(test)]
    pub(crate) fn all() -> [StopReason; 15] {
        [
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
        ]
    }

    /// What to do about this stop. One imperative sentence.
    pub(crate) fn next_step(&self) -> &'static str {
        match self {
            StopReason::MaxDepthReached => {
                "Call the changed owner directly from a test so ripr does not have to walk a \
                 deep call chain."
            }
            StopReason::ExternalCrateBoundary => {
                "Test the changed behavior at a function inside this workspace, and assert on \
                 its result there."
            }
            StopReason::DynamicDispatchUnresolved => {
                "Call the changed owner on the concrete type in a test, not through a trait \
                 object, and assert on its result."
            }
            StopReason::ProcMacroOpaque => {
                "Call the changed owner directly from a test, outside the macro, and assert on \
                 its result."
            }
            StopReason::FixtureOpaque => {
                "Pass a literal input in the test, or assert on a value derived from the fixture \
                 with an exact `assert_eq!`."
            }
            StopReason::FeatureUnknown => {
                "Add a test that builds without the gating feature and asserts on the changed \
                 owner's result."
            }
            StopReason::AsyncBoundaryOpaque => {
                "Await the changed function in the test and assert on its output, rather than \
                 only spawning it."
            }
            StopReason::NoChangedRustLine => {
                "Make sure the changed line sits inside a function (not a macro body or item \
                 attribute); if it does, report the shape in an issue."
            }
            StopReason::InfectionEvidenceUnknown => {
                "Add a test input that reaches the changed value, such as a boundary value, \
                 then assert on the result."
            }
            StopReason::PropagationEvidenceUnknown => {
                "Assert on the value the change flows into (the return value or an observable \
                 field), not only on success."
            }
            StopReason::StaticProbeUnknown => {
                "Add a test that calls the changed code and asserts an exact result."
            }
            StopReason::TransitiveReachUnresolved => {
                "Call the changed owner directly from a test so the path needs no name-matched \
                 hops."
            }
            StopReason::MacroReachUnresolved => {
                "Call the changed owner directly from a plain `#[test]` body instead of through \
                 the macro."
            }
            StopReason::GapEvidenceUnresolved => {
                "Make the related test's assertion a plain `assert_eq!` on the changed value in \
                 a test that always runs, so ripr can tie it to this change."
            }
            StopReason::HelperIdentityUnresolved => {
                "Call the changed function by its full path in the test so the call binds to one \
                 function; rename a same-named function only if the path is not enough."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A next step starts with an imperative verb the reader can act on.
    fn is_actionable(step: &str) -> bool {
        const VERBS: [&str; 17] = [
            "Add", "Call", "Import", "Test", "Assert", "Define", "Move", "Check", "Replace", "Use",
            "Pin", "Split", "Make", "Run", "Await", "Rename", "Pass",
        ];
        step.ends_with('.')
            && step.len() <= 260
            && VERBS.iter().any(|verb| {
                step.strip_prefix(verb)
                    .is_some_and(|rest| rest.starts_with(' '))
            })
    }

    #[test]
    fn static_limit_kinds_all_is_a_gap_free_ordinal_sequence() {
        // The exhaustive match forces an arm for a new variant; `ALL` must
        // list each variant once, in order.
        fn ordinal(kind: StaticLimitKind) -> usize {
            match kind {
                StaticLimitKind::DynamicDispatch => 0,
                StaticLimitKind::Metaprogramming => 1,
                StaticLimitKind::MissingImportGraph => 2,
                StaticLimitKind::DecoratorIndirection => 3,
                StaticLimitKind::MockedModule => 4,
                StaticLimitKind::OpaqueCustomAssertionHelper => 5,
                StaticLimitKind::PropertyBasedTest => 6,
                StaticLimitKind::UnresolvedPytestFixture => 7,
                StaticLimitKind::UnsupportedSyntax => 8,
                StaticLimitKind::CrossLanguageOracleVisibilityUnresolved => 9,
                StaticLimitKind::RustTransitiveReachUnresolved => 10,
                StaticLimitKind::RustIntegrationPublicApiPathUnresolved => 11,
                StaticLimitKind::RustMacroReachUnresolved => 12,
                StaticLimitKind::RustMacroWrappedTestCallUnresolved => 13,
                StaticLimitKind::RustMacroWrappedAssertionUnresolved => 14,
                StaticLimitKind::RustValuePropagationUnresolved => 15,
                StaticLimitKind::RustFieldMatchSelectorUnresolved => 16,
                StaticLimitKind::RustSubprocessBinaryReachUnresolved => 17,
                StaticLimitKind::WrapperErrorBindingUnresolved => 18,
                StaticLimitKind::PythonTransitiveReachUnresolved => 19,
                StaticLimitKind::RustAssertionContextUnresolved => 20,
                StaticLimitKind::RustLogCaptureUnresolved => 21,
                StaticLimitKind::RustPanicOutcomeUnresolved => 22,
                StaticLimitKind::RustDebugAssertionProfileUnresolved => 23,
                StaticLimitKind::RustCallFactBudget => 24,
                StaticLimitKind::RustFrameworkOracleUnresolved => 25,
                StaticLimitKind::RustPinnedShapeUnresolved => 26,
                StaticLimitKind::RustImplicitCallUnresolved => 27,
                StaticLimitKind::RustCallerLoopSelectorUnresolved => 28,
            }
        }
        const COUNT: usize = 29;
        assert_eq!(StaticLimitKind::ALL.len(), COUNT);
        let ordinals: Vec<usize> = StaticLimitKind::ALL.iter().copied().map(ordinal).collect();
        assert_eq!(
            ordinals,
            (0..StaticLimitKind::ALL.len()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn stop_reasons_all_is_a_gap_free_ordinal_sequence() {
        fn ordinal(reason: &StopReason) -> usize {
            match reason {
                StopReason::MaxDepthReached => 0,
                StopReason::ExternalCrateBoundary => 1,
                StopReason::DynamicDispatchUnresolved => 2,
                StopReason::ProcMacroOpaque => 3,
                StopReason::FixtureOpaque => 4,
                StopReason::FeatureUnknown => 5,
                StopReason::AsyncBoundaryOpaque => 6,
                StopReason::NoChangedRustLine => 7,
                StopReason::InfectionEvidenceUnknown => 8,
                StopReason::PropagationEvidenceUnknown => 9,
                StopReason::StaticProbeUnknown => 10,
                StopReason::TransitiveReachUnresolved => 11,
                StopReason::MacroReachUnresolved => 12,
                StopReason::GapEvidenceUnresolved => 13,
                StopReason::HelperIdentityUnresolved => 14,
            }
        }
        const COUNT: usize = 15;
        let all = StopReason::all();
        assert_eq!(all.len(), COUNT);
        let ordinals: Vec<usize> = all.iter().map(ordinal).collect();
        assert_eq!(ordinals, (0..all.len()).collect::<Vec<_>>());
    }

    #[test]
    fn every_static_limit_names_an_actionable_next_step() {
        for kind in StaticLimitKind::ALL {
            let step = kind.next_step();
            assert!(is_actionable(step), "{}: {step}", kind.as_str());
            assert!(!step.contains("  "), "{}: {step}", kind.as_str());
            assert_ne!(step, kind.describe(), "{}", kind.as_str());
        }
    }

    #[test]
    fn every_stop_reason_names_an_actionable_next_step() {
        for reason in StopReason::all() {
            let step = reason.next_step();
            assert!(is_actionable(step), "{}: {step}", reason.as_str());
            assert!(!step.contains("  "), "{}: {step}", reason.as_str());
        }
    }

    #[test]
    fn next_steps_are_distinct_per_kind() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in StaticLimitKind::ALL {
            assert!(
                seen.insert(kind.next_step()),
                "{} repeats another kind's next step",
                kind.as_str()
            );
        }
    }

    #[test]
    fn next_steps_are_distinct_per_stop_reason() {
        let mut seen = std::collections::BTreeSet::new();
        for reason in StopReason::all() {
            assert!(
                seen.insert(reason.next_step()),
                "{} repeats another stop reason's next step",
                reason.as_str()
            );
        }
    }

    #[test]
    fn helper_identity_next_step_binds_by_full_path() {
        let step = StopReason::HelperIdentityUnresolved.next_step();
        assert!(step.contains("full path"), "{step}");
    }

    #[test]
    fn next_steps_make_no_runtime_or_coverage_claim() {
        let banned = ["will pass", "is covered", "guarantee"];
        let stops = StopReason::all();
        let steps = StaticLimitKind::ALL
            .iter()
            .map(StaticLimitKind::next_step)
            .chain(stops.iter().map(StopReason::next_step));
        for step in steps {
            for word in banned {
                assert!(!step.contains(word), "{step}");
            }
        }
    }
}
