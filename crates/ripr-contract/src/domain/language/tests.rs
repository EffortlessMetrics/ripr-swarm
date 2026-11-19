use super::*;

#[test]
fn all_is_a_gap_free_ordinal_sequence() {
    // The exhaustive match below forces an ordinal arm for a new variant.
    // Catches duplicates, gaps and a stale `ALL`. It cannot enumerate the
    // enum: a variant added to the match but to neither `COUNT` nor `ALL`
    // is not detected, only forced to be classified here.
    const COUNT: usize = 5;
    fn ordinal(language: LanguageId) -> usize {
        match language {
            LanguageId::Rust => 0,
            LanguageId::TypeScript => 1,
            LanguageId::JavaScript => 2,
            LanguageId::Python => 3,
            LanguageId::Perl => 4,
        }
    }
    let ordinals: Vec<usize> = LanguageId::ALL.iter().copied().map(ordinal).collect();
    assert_eq!(ordinals, (0..COUNT).collect::<Vec<_>>());
}

/// #4323: a lost string-literal continuation left ~18-space runs inside
/// this gloss, which rendered as mid-sentence gaps in human and JSON output.
#[test]
fn wrapper_error_binding_gloss_has_no_whitespace_runs() {
    let gloss = StaticLimitKind::WrapperErrorBindingUnresolved.describe();
    assert!(!gloss.contains("  "), "{gloss}");
    assert!(
        gloss.contains("boxed wrapper (`map_err(Into::into)`), so"),
        "{gloss}"
    );
}

#[test]
fn pytest_verify_command_accepts_module_and_legacy_bare_forms() {
    assert!(is_pytest_verify_command(
        "python -m pytest tests/test_pricing.py::test_boundary"
    ));
    assert!(is_pytest_verify_command("pytest tests/test_pricing.py"));
    assert!(!is_pytest_verify_command("python -m pytestx tests"));
    assert!(!is_pytest_verify_command("pytestx tests"));
    assert!(!is_pytest_verify_command("python -m unittest tests.test_x"));
    assert!(!is_pytest_verify_command("python -m pytest"));
}

#[test]
fn pytest_verify_interpreter_names_path_or_project_venv_python_only() {
    let node = " -m pytest tests/test_pricing.py::test_boundary";
    for interpreter in PYTEST_VERIFY_INTERPRETERS {
        let command = format!("{interpreter}{node}");
        assert_eq!(pytest_verify_interpreter(&command), Some(interpreter));
        assert!(is_pytest_verify_command(&command));
    }
    for other in [
        "python3 -m pytest tests",
        "/usr/bin/python -m pytest tests",
        "env/bin/python -m pytest tests",
        ".venv/bin/python -m pytest ",
        ".venv/bin/python -m unittest tests.test_x",
        ".venv/bin/pythonx -m pytest tests",
        "pytest tests",
    ] {
        assert_eq!(pytest_verify_interpreter(other), None, "{other}");
    }
}

#[test]
fn language_id_wire_strings_are_stable() {
    assert_eq!(LanguageId::Rust.as_str(), "rust");
    assert_eq!(LanguageId::TypeScript.as_str(), "typescript");
    assert_eq!(LanguageId::JavaScript.as_str(), "javascript");
    assert_eq!(LanguageId::Python.as_str(), "python");
    assert_eq!(LanguageId::Perl.as_str(), "perl");
}

#[test]
fn unavailable_adapter_recovery_names_the_feature_for_non_perl_languages() {
    assert_eq!(
        LanguageId::TypeScript.unavailable_adapter_recovery(),
        format!(
            "rebuild ripr with Cargo feature `{}` to analyze TypeScript and JavaScript files",
            LanguageId::TypeScript.required_feature()
        )
    );
    for language in [LanguageId::JavaScript, LanguageId::Python] {
        let recovery = language.unavailable_adapter_recovery();
        assert_eq!(
            recovery,
            format!(
                "rebuild ripr with Cargo feature `{}` to analyze {} files",
                language.required_feature(),
                language.as_str()
            )
        );
        assert!(
            !recovery.contains("perl-ripr-facts"),
            "only Perl names the external exporter: {recovery}"
        );
    }
    let perl = LanguageId::Perl.unavailable_adapter_recovery();
    assert!(perl.contains("lang-perl") && perl.contains(PERL_FACT_EXPORTER));
}

#[test]
fn language_feature_availability_matches_build() {
    assert_eq!(LanguageId::Rust.is_available(), cfg!(feature = "lang-rust"));
    assert_eq!(
        LanguageId::TypeScript.is_available(),
        cfg!(feature = "lang-typescript")
    );
    assert_eq!(
        LanguageId::JavaScript.is_available(),
        cfg!(feature = "lang-typescript")
    );
    assert_eq!(
        LanguageId::Python.is_available(),
        cfg!(feature = "lang-python")
    );
    assert_eq!(LanguageId::Perl.is_available(), cfg!(feature = "lang-perl"));
    assert_eq!(LanguageId::JavaScript.required_feature(), "lang-typescript");
    assert_eq!(LanguageId::Python.required_feature(), "lang-python");
    assert_eq!(LanguageId::Perl.required_feature(), "lang-perl");
}

#[test]
fn unavailable_adapter_notice_restates_recovery_owner_only_when_missing() {
    for language in [
        LanguageId::Rust,
        LanguageId::TypeScript,
        LanguageId::JavaScript,
        LanguageId::Python,
        LanguageId::Perl,
    ] {
        assert_eq!(
            language.unavailable_adapter_notice().is_some(),
            !language.is_available(),
            "{language:?}"
        );
        if let Some(notice) = language.unavailable_adapter_notice() {
            let recovery = language.unavailable_adapter_recovery();
            assert_eq!(
                notice,
                format!("{recovery}."),
                "the pilot notice must restate the recovery owner's wording"
            );
        }
    }
    if !cfg!(feature = "lang-perl") {
        let perl = LanguageId::Perl.unavailable_adapter_notice();
        assert!(
            perl.as_deref().is_some_and(|text| {
                text.contains("lang-perl")
                    && text.contains(PERL_FACT_EXPORTER)
                    && text.contains("not yet published")
            }),
            "the pilot notice must name both Perl prerequisites: {perl:?}"
        );
    }
}

#[test]
fn enablement_tip_omits_default_rust_when_rust_is_not_detected() {
    assert_eq!(
        enablement_tip_languages(&[LanguageId::Rust], false, [LanguageId::TypeScript]),
        vec![LanguageId::TypeScript]
    );
    assert_eq!(
        format_languages_enabled_toml(&["typescript"]),
        r#"enabled = ["typescript"]"#
    );
}

#[test]
fn enablement_tip_keeps_rust_when_rust_is_detected() {
    assert_eq!(
        enablement_tip_languages(&[LanguageId::Rust], true, [LanguageId::TypeScript]),
        vec![LanguageId::Rust, LanguageId::TypeScript]
    );
    assert_eq!(
        format_languages_enabled_toml(&["rust", "typescript"]),
        r#"enabled = ["rust", "typescript"]"#
    );
}

#[test]
fn enablement_tip_keeps_other_enabled_preview_languages_without_inventing_rust() {
    assert_eq!(
        enablement_tip_languages(
            &[LanguageId::Rust, LanguageId::Python],
            false,
            [LanguageId::TypeScript]
        ),
        vec![LanguageId::Python, LanguageId::TypeScript]
    );
    assert_eq!(
        enablement_tip_languages(
            &[LanguageId::Rust, LanguageId::Python],
            true,
            [LanguageId::TypeScript]
        ),
        vec![LanguageId::Rust, LanguageId::Python, LanguageId::TypeScript]
    );
}

#[test]
fn rust_markers_at_sees_cargo_toml_and_rs_and_ignores_nested_only_rust() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root =
        std::env::temp_dir().join(format!("ripr-rust-markers-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).map_err(|err| format!("create src: {err}"))?;
    assert!(
        !rust_markers_at(&root),
        "empty root must not look like Rust"
    );
    std::fs::write(root.join("src/lib.rs"), "pub fn f() {}\n")
        .map_err(|err| format!("write lib.rs: {err}"))?;
    assert!(rust_markers_at(&root), ".rs under src/ is a Rust marker");
    std::fs::remove_file(root.join("src/lib.rs")).map_err(|err| format!("remove lib.rs: {err}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .map_err(|err| format!("write Cargo.toml: {err}"))?;
    assert!(rust_markers_at(&root), "root Cargo.toml is a Rust marker");
    std::fs::remove_file(root.join("Cargo.toml"))
        .map_err(|err| format!("remove Cargo.toml: {err}"))?;
    std::fs::create_dir_all(root.join("rust"))
        .map_err(|err| format!("create nested rust: {err}"))?;
    std::fs::write(
        root.join("rust/Cargo.toml"),
        "[package]\nname = \"nested\"\nversion = \"0.1.0\"\n",
    )
    .map_err(|err| format!("write nested Cargo.toml: {err}"))?;
    assert!(
        !rust_markers_at(&root),
        "nested rust/Cargo.toml is not a root marker"
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn language_status_wire_strings_are_stable() {
    assert_eq!(LanguageStatus::Stable.as_str(), "stable");
    assert_eq!(LanguageStatus::Preview.as_str(), "preview");
}

#[test]
fn owner_kind_wire_strings_are_stable() {
    assert_eq!(OwnerKind::Function.as_str(), "function");
    assert_eq!(OwnerKind::Method.as_str(), "method");
    assert_eq!(OwnerKind::ClassMethod.as_str(), "class_method");
    assert_eq!(OwnerKind::ArrowFunction.as_str(), "arrow_function");
    assert_eq!(OwnerKind::Component.as_str(), "component");
    assert_eq!(OwnerKind::ModuleFunction.as_str(), "module_function");
}

#[test]
fn static_limit_kind_wire_strings_are_stable() {
    assert_eq!(
        StaticLimitKind::DynamicDispatch.as_str(),
        "dynamic_dispatch"
    );
    assert_eq!(StaticLimitKind::Metaprogramming.as_str(), "metaprogramming");
    assert_eq!(
        StaticLimitKind::MissingImportGraph.as_str(),
        "missing_import_graph"
    );
    assert_eq!(
        StaticLimitKind::DecoratorIndirection.as_str(),
        "decorator_indirection"
    );
    assert_eq!(StaticLimitKind::MockedModule.as_str(), "mocked_module");
    assert_eq!(
        StaticLimitKind::UnsupportedSyntax.as_str(),
        "unsupported_syntax"
    );
    assert_eq!(
        StaticLimitKind::OpaqueCustomAssertionHelper.as_str(),
        "opaque_custom_assertion_helper"
    );
    assert_eq!(
        StaticLimitKind::PropertyBasedTest.as_str(),
        "property_based_test"
    );
    assert_eq!(
        StaticLimitKind::UnresolvedPytestFixture.as_str(),
        "unresolved_pytest_fixture"
    );
    assert_eq!(
        StaticLimitKind::CrossLanguageOracleVisibilityUnresolved.as_str(),
        "cross_language_oracle_visibility_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustTransitiveReachUnresolved.as_str(),
        "rust_transitive_reach_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustIntegrationPublicApiPathUnresolved.as_str(),
        "rust_integration_public_api_path_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustValuePropagationUnresolved.as_str(),
        "rust_value_propagation_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustSubprocessBinaryReachUnresolved.as_str(),
        "rust_subprocess_binary_reach_unresolved"
    );
    assert_eq!(
        StaticLimitKind::WrapperErrorBindingUnresolved.as_str(),
        "wrapper_error_binding_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustMacroReachUnresolved.as_str(),
        "rust_macro_reach_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustMacroWrappedTestCallUnresolved.as_str(),
        "rust_macro_wrapped_test_call_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustMacroWrappedAssertionUnresolved.as_str(),
        "rust_macro_wrapped_assertion_unresolved"
    );
    assert_eq!(
        StaticLimitKind::PythonTransitiveReachUnresolved.as_str(),
        "python_transitive_reach_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustAssertionContextUnresolved.as_str(),
        "rust_assertion_context_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustLogCaptureUnresolved.as_str(),
        "rust_log_capture_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustPanicOutcomeUnresolved.as_str(),
        "rust_panic_outcome_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustDebugAssertionProfileUnresolved.as_str(),
        "rust_debug_assertion_profile_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustCallFactBudget.as_str(),
        "rust_call_fact_budget"
    );
    assert_eq!(
        StaticLimitKind::RustFrameworkOracleUnresolved.as_str(),
        "rust_framework_oracle_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustPinnedShapeUnresolved.as_str(),
        "rust_pinned_shape_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustImplicitCallUnresolved.as_str(),
        "rust_implicit_call_unresolved"
    );
    assert_eq!(
        StaticLimitKind::RustIndirectOwnerCallUnresolved.as_str(),
        "rust_indirect_owner_call_unresolved"
    );
    assert_eq!(
        StaticLimitKind::ProofHarnessNotEvaluated.as_str(),
        "proof_harness_not_evaluated"
    );
    assert_eq!(
        StaticLimitKind::RustInherentReceiverUnestablished.as_str(),
        "rust_inherent_receiver_unestablished"
    );
    assert_eq!(
        StaticLimitKind::TypescriptWorkspaceDiscoveryTruncated.as_str(),
        "typescript_workspace_discovery_truncated"
    );
    assert_eq!(
        StaticLimitKind::TypescriptWorkspaceReadLimited.as_str(),
        "typescript_workspace_read_limited"
    );
    assert_eq!(
        StaticLimitKind::RustFutureNeverPolled.as_str(),
        "rust_future_never_polled"
    );
    assert_eq!(
        StaticLimitKind::ActivationBoundaryInputUnresolved.as_str(),
        "activation_boundary_input_unresolved"
    );
}

#[test]
fn static_limit_kind_describe_is_present_and_distinct() {
    let kinds = [
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
        StaticLimitKind::RustIndirectOwnerCallUnresolved,
        StaticLimitKind::RustCallerLoopSelectorUnresolved,
        StaticLimitKind::ProofHarnessNotEvaluated,
        StaticLimitKind::RustInherentReceiverUnestablished,
        StaticLimitKind::TypescriptWorkspaceDiscoveryTruncated,
        StaticLimitKind::TypescriptWorkspaceReadLimited,
        StaticLimitKind::RustFutureNeverPolled,
        StaticLimitKind::ActivationBoundaryInputUnresolved,
    ];
    // Every variant has a non-empty, distinct explanation. Conservative
    // static-language vocabulary is enforced repo-wide by
    // `cargo xtask check-static-language` (which scans this prose too), so it
    // is not re-checked here with literal forbidden terms.
    let mut seen = std::collections::HashSet::new();
    for kind in kinds {
        let described = kind.describe();
        assert!(described.len() > 20, "describe too short for {kind:?}");
        assert!(
            seen.insert(described),
            "duplicate describe text for {kind:?}"
        );
    }
}

#[test]
fn rust_transitive_reach_description_matches_depth_contract() {
    assert!(
        StaticLimitKind::RustTransitiveReachUnresolved
            .describe()
            .contains("depth greater than 5"),
        "transitive-reach limitation text must match RIPR-SPEC-0114's depth-5 bound"
    );
}

#[test]
fn python_transitive_reach_description_is_named_limitation_not_coverage() {
    let described = StaticLimitKind::PythonTransitiveReachUnresolved.describe();
    assert!(described.contains("may"));
    assert!(described.contains("named limitation"));
    assert!(!described.contains("covers"));
    assert!(!described.contains("tested"));
}

#[test]
fn display_names_use_product_casing_and_round_trip_wire_strings() {
    let pairs: Vec<(&str, &str)> = LanguageId::ALL
        .into_iter()
        .map(|language| (language.as_str(), language.display_name()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("rust", "Rust"),
            ("typescript", "TypeScript"),
            ("javascript", "JavaScript"),
            ("python", "Python"),
            ("perl", "Perl"),
        ]
    );
    for language in LanguageId::ALL {
        assert_eq!(
            LanguageId::display_name_for_wire(language.as_str()),
            Some(language.display_name())
        );
    }
    assert_eq!(LanguageId::display_name_for_wire("cobol"), None);
}

#[test]
fn rust_markers_reach_one_child_directory_and_no_deeper() -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-rust-markers-child-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("tools/nested"))
        .map_err(|err| format!("create dirs: {err}"))?;
    std::fs::write(root.join("tools/nested/deep.rs"), "fn f() {}\n")
        .map_err(|err| format!("write deep.rs: {err}"))?;
    let grandchild_only = rust_markers_at(&root);
    std::fs::write(root.join("tools/build.rs"), "fn main() {}\n")
        .map_err(|err| format!("write build.rs: {err}"))?;
    let child = rust_markers_at(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert!(!grandchild_only, "a grandchild .rs is not a marker");
    assert!(child, "a .rs in one child directory is a marker");
    Ok(())
}

#[test]
fn only_a_match_arm_rewrites_the_assertion_context_spelling() {
    use super::super::ProbeFamily;
    let kind = StaticLimitKind::RustAssertionContextUnresolved;
    let match_arm = kind.describe_for_family(&ProbeFamily::MatchArm);
    assert!(match_arm.contains("`assert!`"));
    assert!(!match_arm.contains("`assert_eq!`"));
    assert_eq!(
        kind.describe_for_family(&ProbeFamily::Predicate),
        kind.describe()
    );
    let other = StaticLimitKind::DynamicDispatch;
    assert_eq!(
        other.describe_for_family(&ProbeFamily::MatchArm),
        other.describe()
    );
}
