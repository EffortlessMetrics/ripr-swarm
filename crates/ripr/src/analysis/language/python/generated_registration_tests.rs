use super::generated_registration;
use super::source_facts::extract_source_facts;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn registration_source(import: &str, base: &str, prefix: &str, suffix: &str) -> String {
    format!(
        "{import}\n{prefix}class Machine({base}):\n    pass\nTestMachine = Machine.TestCase\n{suffix}"
    )
}

#[test]
fn framework_aliases_retain_only_generated_registration_facts() {
    for (import, base) in [
        (
            "from hypothesis.stateful import RuleBasedStateMachine",
            "RuleBasedStateMachine",
        ),
        (
            "from hypothesis.stateful import RuleBasedStateMachine as State",
            "State",
        ),
        (
            "import hypothesis.stateful as stateful",
            "stateful.RuleBasedStateMachine",
        ),
        (
            "from hypothesis import stateful as hs",
            "hs.RuleBasedStateMachine",
        ),
        (
            "import hypothesis.stateful",
            "hypothesis.stateful.RuleBasedStateMachine",
        ),
    ] {
        let source = registration_source(
            import,
            base,
            "",
            "TestMachine.settings = settings(max_examples=5)\n",
        );
        let facts = extract_source_facts(Path::new("tests/test_counter.py"), &source);
        assert_eq!(facts.generated_registrations.len(), 1, "{source}");
        assert_eq!(facts.limitations.len(), 1, "{source}");
        assert!(facts.tests.is_empty(), "no synthesized test methods");
        assert_eq!(facts.generated_registrations[0].name, "TestMachine");
    }
}

#[test]
fn unrelated_shadowed_rebound_or_unsupported_shapes_gain_no_framework_authority() {
    let import = "from hypothesis.stateful import RuleBasedStateMachine";
    for source in [
        registration_source("", "RuleBasedStateMachine", "", ""),
        registration_source(
            "from local import RuleBasedStateMachine",
            "RuleBasedStateMachine",
            "",
            "",
        ),
        registration_source(
            import,
            "RuleBasedStateMachine",
            "RuleBasedStateMachine = object\n",
            "",
        ),
        registration_source(import, "RuleBasedStateMachine", "", "Machine = other\n"),
        registration_source(import, "RuleBasedStateMachine", "", "TestMachine = other\n"),
        registration_source(
            import,
            "RuleBasedStateMachine",
            "",
            "Machine.TestCase = other\n",
        ),
        registration_source(import, "RuleBasedStateMachine", "", "del TestMachine\n"),
        "class Machine:\n    TestCase = object\nTestMachine = Machine.TestCase\n".to_string(),
        format!("{import}\nclass Machine(RuleBasedStateMachine):\n    pass\n"),
        format!(
            "{import}\nclass Machine(RuleBasedStateMachine):\n    pass\nclass Child(Machine):\n    pass\nTestChild = Child.TestCase\n"
        ),
        format!(
            "{import}\nclass Machine(RuleBasedStateMachine):\n    pass\nTestMachine = factory(Machine)\n"
        ),
        format!(
            "{import}\nclass Machine(RuleBasedStateMachine):\n    TestCase = object\nTestMachine = Machine.TestCase\n"
        ),
        registration_source(
            "import hypothesis",
            "hypothesis.stateful.RuleBasedStateMachine",
            "",
            "",
        ),
        "# TestMachine = Machine.TestCase\ntext = 'TestMachine = Machine.TestCase'\n".to_string(),
        format!(
            "class Machine(RuleBasedStateMachine):\n    pass\n{import}\nTestMachine = Machine.TestCase\n"
        ),
    ] {
        let facts = extract_source_facts(Path::new("tests/test_counter.py"), &source);
        assert!(facts.generated_registrations.is_empty(), "{source}");
        assert!(
            facts.limitations.is_empty(),
            "{source}: {:?}",
            facts.limitations
        );
    }
}

#[test]
fn generated_registration_relevance_keeps_unknowns_but_excludes_a_closed_unrelated_graph() {
    let source = registration_source(
        "from hypothesis.stateful import RuleBasedStateMachine\nfrom other import increment",
        "RuleBasedStateMachine",
        "",
        "",
    );
    let file = PathBuf::from("tests/test_counter.py");
    let facts = extract_source_facts(&file, &source);
    assert_eq!(facts.generated_registrations.len(), 1);
    let registration = &facts.generated_registrations[0];
    let mut sources = HashMap::from([
        (file.clone(), source),
        (
            PathBuf::from("counter.py"),
            "def increment(value):\n    return value + 2\n".to_string(),
        ),
        (
            PathBuf::from("other.py"),
            "def increment(value):\n    return value + 1\n".to_string(),
        ),
    ]);
    let changed = [PathBuf::from("counter.py")];
    assert!(!generated_registration::relevant(
        &file,
        registration,
        &changed,
        &sources
    ));
    sources.insert(
        PathBuf::from("other.py"),
        "from counter import increment\n".to_string(),
    );
    assert!(generated_registration::relevant(
        &file,
        registration,
        &changed,
        &sources
    ));
    sources.remove(Path::new("other.py"));
    assert!(generated_registration::relevant(
        &file,
        registration,
        &changed,
        &sources
    ));
}

#[test]
fn generated_registration_does_not_make_a_parsed_file_an_opaque_fixture_source() {
    let source = registration_source(
        "from hypothesis.stateful import RuleBasedStateMachine",
        "RuleBasedStateMachine",
        "",
        "\ndef test_plain():\n    assert 1 == 1\n",
    );
    let facts = extract_source_facts(Path::new("tests/test_counter.py"), &source);
    assert_eq!(facts.tests.len(), 1);
    assert_eq!(facts.generated_registrations.len(), 1);
    assert!(!super::fixtures::side_effects_of(&facts).opaque);
    assert_eq!(facts.tests[0].assertions.len(), 1);
}

#[test]
fn generated_registration_diff_disclosure_is_scoped_and_preserves_independent_credit()
-> Result<(), String> {
    use super::*;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-registration-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(root.as_path().join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.as_path().join("counter.py"),
        "def increment(value):\n    return value + 2\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        root.as_path().join("other.py"),
        "def increment(value):\n    return value + 1\n",
    )
    .map_err(|error| error.to_string())?;
    let options = AnalysisOptions {
        root: root.as_path().to_path_buf(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let changed = ChangedFile {
        path: PathBuf::from("counter.py"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 2,
            new_side_line: 2,
            text: "    return value + 2".to_string(),
        }],
        removed_lines: Vec::new(),
    };
    for (module, expected_limits) in [("counter", 1), ("other", 0), ("unavailable", 1)] {
        let source = registration_source(
            &format!(
                "from hypothesis.stateful import RuleBasedStateMachine\nfrom {module} import increment"
            ),
            "RuleBasedStateMachine",
            "",
            "",
        );
        std::fs::write(root.as_path().join("tests/test_counter.py"), source)
            .map_err(|error| error.to_string())?;
        let result = PythonAdapter.analyze_diff(
            &options,
            &OraclePolicy::default(),
            std::slice::from_ref(&changed),
        )?;
        assert_eq!(
            result.limitations.len(),
            expected_limits,
            "{module}: {:?}",
            result.limitations
        );
        assert!(!result.findings.is_empty());
        assert!(
            result
                .findings
                .iter()
                .all(|finding| finding.class == ExposureClass::NoStaticPath)
        );
        assert!(
            result
                .findings
                .iter()
                .all(|finding| finding.related_tests.is_empty())
        );
        if expected_limits > 0 {
            assert!(
                result.limitations[0]
                    .bounded_detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("hypothesis_generated_registration")
            );
        }
    }
    std::fs::write(
        root.as_path().join("tests/test_plain.py"),
        "from counter import increment\ndef test_plain():\n    assert increment(0) == 2\n",
    )
    .map_err(|error| error.to_string())?;
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &[changed])?;
    assert_eq!(result.limitations.len(), 1);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed),
        "{:?}",
        result.findings
    );
    let config = crate::config::tests_only_parse("[languages]\nenabled = [\"python\"]\n")?;
    let repo_output = crate::app::check_workspace_repo_with_config(
        crate::CheckInput {
            root: root.clone(),
            base: None,
            diff_file: None,
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        },
        &config,
    )?;
    let rendered = crate::render_check(&repo_output, &crate::OutputFormat::Json)?;
    let json: serde_json::Value =
        serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
    assert!(rendered.contains("hypothesis_generated_registration"));
    let runs = json
        .get("language_runs")
        .and_then(serde_json::Value::as_array)
        .ok_or("missing repository language-run disclosure")?;
    assert!(runs.iter().any(|run| {
        run.get("language").and_then(serde_json::Value::as_str) == Some("python")
            && run.get("status").and_then(serde_json::Value::as_str) == Some("partial")
            && run
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|reason| reason.contains("hypothesis_generated_registration"))
    }));
    std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(())
}

#[test]
fn generated_registration_refuses_rebound_framework_members_and_class_lookup() {
    for source in [
        registration_source(
            "import hypothesis.stateful as hs",
            "hs.RuleBasedStateMachine",
            "hs.RuleBasedStateMachine = object\n",
            "",
        ),
        registration_source(
            "from hypothesis import stateful as hs",
            "hs.RuleBasedStateMachine",
            "hs.RuleBasedStateMachine = object\n",
            "",
        ),
        registration_source(
            "import hypothesis.stateful",
            "hypothesis.stateful.RuleBasedStateMachine",
            "hypothesis.stateful = other\n",
            "",
        ),
        registration_source(
            "from hypothesis.stateful import RuleBasedStateMachine",
            "RuleBasedStateMachine",
            "",
            "Machine.__bases__ = (object,)\n",
        ),
    ] {
        let facts = extract_source_facts(Path::new("tests/test_counter.py"), &source);
        assert!(facts.generated_registrations.is_empty(), "{source}");
        assert!(facts.limitations.is_empty(), "{source}");
    }
}
