use super::is_process_policy_candidate;

#[test]
fn native_qualification_action_sources_are_process_policy_candidates() {
    for directory in [
        ".github/actions/qualification-budget/",
        ".github/actions/verify-merge-group-subject/",
    ] {
        for relative in [
            "action.yml",
            "action.yaml",
            "budget.py",
            "nested/test_budget.py",
        ] {
            let path = format!("{directory}{relative}");
            assert!(is_process_policy_candidate(&path), "{path}");
        }
    }
    for path in [
        "crates/sample/src/lib.rs",
        "extension/main.ts",
        "tools/python/sample/run.py",
    ] {
        assert!(is_process_policy_candidate(path), "{path}");
    }
}

#[test]
fn native_qualification_process_scope_does_not_expand_to_other_actions_or_formats() {
    for path in [
        ".github/actions/qualification-budget/README.md",
        ".github/actions/qualification-budget/script.sh",
        ".github/actions/qualification-budget/script.js",
        ".github/actions/qualification-budget/action.yml.bak",
        ".github/actions/qualification-budget-other/test_budget.py",
        ".github/actions/verify-merge-group-subject-other/action.yml",
        ".github/actions/verify-merge-group-subject/README.md",
        ".github/actions/verify-merge-group-subject/script.ps1",
        ".github/actions/pr-candidate-admission/candidate_admission.py",
        ".github/actions/other/action.yml",
        ".github/workflows/qualification.yml",
        "fixtures/example.py",
        "other/.github/actions/qualification-budget/test_budget.py",
    ] {
        assert!(!is_process_policy_candidate(path), "{path}");
    }
}
