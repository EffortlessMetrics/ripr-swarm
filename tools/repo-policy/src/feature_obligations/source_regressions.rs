//! Independent review counterexamples against the original 6261cc3 oracle.
use super::{CI, VSCODE, source_agrees};

fn must_reject(changed: &str, expected_reason: &str) -> Result<(), String> {
    source_agrees(CI, VSCODE)?;
    assert_ne!(changed, CI, "counterexample did not change source");
    let Err(error) = source_agrees(changed, VSCODE) else {
        return Err("source oracle accepted changed workflow semantics".to_owned());
    };
    assert!(error.starts_with(expected_reason), "wrong refusal: {error}");
    Ok(())
}

fn insert_after_perl_run(insertion: &str) -> String {
    let marker = "        run: cargo check -p ripr --features lang-perl\n";
    replace_once(marker, &format!("{marker}{insertion}"))
}

fn replace_once(before: &str, after: &str) -> String {
    assert_eq!(CI.matches(before).count(), 1, "ambiguous mutation target");
    CI.replacen(before, after, 1)
}

#[test]
fn quoted_if_cannot_hide_a_skipped_proof_command() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("        \"if\": false\n"),
        "unsupported nonplain YAML key",
    )
}

#[test]
fn escaped_if_cannot_hide_a_skipped_proof_command() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("        \"\\x69f\": false\n"),
        "unsupported nonplain YAML key",
    )
}

#[test]
fn explicit_multiline_if_cannot_hide_a_skipped_proof_command() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("        ? if\n        : false\n"),
        "unsupported nonplain YAML key",
    )
}

#[test]
fn quoted_continue_on_error_cannot_hide_failure_masking() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("        \"continue-on-error\": true\n"),
        "unsupported nonplain YAML key",
    )
}

#[test]
fn additional_quoted_dynamic_run_cannot_escape_the_denominator() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("      - \"run\": ${{ inputs.dynamic_command }}\n"),
        "unsupported nonplain YAML key",
    )
}

#[test]
fn aggregate_subshell_failure_cannot_be_swallowed() -> Result<(), String> {
    must_reject(
        &replace_once(
            "          require \"$SELECT_VSCODE\" \"$VSCODE_RESULT\"",
            "          (require \"$SELECT_VSCODE\" \"$VSCODE_RESULT\") || true",
        ),
        "complete aggregate step:",
    )
}

#[test]
fn classifier_result_cannot_be_overwritten_before_output() -> Result<(), String> {
    must_reject(
        &replace_once(
            "          classify\n",
            "          classify\n          rust=false\n",
        ),
        "complete surface step:",
    )
}

#[test]
fn alternate_shell_cannot_echo_instead_of_execute_proof() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("        shell: echo {0}\n"),
        "command shell:",
    )
}

#[test]
fn step_working_directory_cannot_change_the_subject() -> Result<(), String> {
    must_reject(
        &insert_after_perl_run("        working-directory: /tmp\n"),
        "unsupported command step key: working-directory",
    )
}

#[test]
fn job_default_directory_cannot_change_the_subject() -> Result<(), String> {
    must_reject(
        &replace_once(
            "  release-proof:\n",
            "  release-proof:\n    defaults:\n      run:\n        working-directory: /tmp\n",
        ),
        "unsupported job defaults",
    )
}

#[test]
fn unsupported_aliases_duplicate_keys_and_contexts_refuse_at_their_owner() -> Result<(), String> {
    for insertion in [
        "        'if': false\n",
        "        \"i\\u0066\": false\n",
        "        <<: *skip_defaults\n",
    ] {
        must_reject(
            &insert_after_perl_run(insertion),
            "unsupported nonplain YAML key",
        )?;
    }
    must_reject(
        &insert_after_perl_run("        run: echo success\n"),
        "duplicate YAML key: run",
    )?;
    must_reject(
        &replace_once(
            "\njobs:\n",
            "\ndefaults:\n  run:\n    shell: echo {0}\njobs:\n",
        ),
        "unsupported workflow key: defaults",
    )?;
    must_reject(
        &replace_once(
            "  ci-result:\n",
            "  ci-result:\n    defaults:\n      run:\n        shell: echo {0}\n",
        ),
        "unsupported aggregate job key: defaults",
    )?;
    must_reject(
        &replace_once(
            "      - name: Detect Rust surface change\n",
            "      - name: Detect Rust surface change\n        if: false\n",
        ),
        "complete surface step:",
    )?;
    Ok(())
}
