//! Source drift checks, not a YAML execution or hosted qualification claim.
use super::*;
const WORKFLOW: &str =
    include_str!("../../../../../.github/workflows/python-wheel-qualification.yml");

fn job(name: &str) -> Result<&'static str, String> {
    let (_, source) = WORKFLOW
        .split_once(&format!("\n  {name}:\n"))
        .ok_or_else(|| format!("missing source job {name}"))?;
    Ok(source
        .split("\n  source-contract:")
        .next()
        .unwrap_or(source)
        .split("\n  build-wheel:")
        .next()
        .unwrap_or(source)
        .split("\n  consume:")
        .next()
        .unwrap_or(source)
        .split("\n  mcp-evidence-pair:")
        .next()
        .unwrap_or(source))
}

#[test]
fn source_named_steps_match_fixed_contract_in_order() -> Result<(), String> {
    for definition in definitions() {
        let source = job(definition.job_key)?;
        let actual: Vec<_> = source
            .lines()
            .filter_map(|line| line.strip_prefix("      - name: "))
            .filter(|name| {
                ![
                    "Select wheel applicability from immutable event history",
                    "Record completed Python wheel client execution",
                    "Retain completed Python wheel client observation",
                ]
                .contains(name)
            })
            .filter(|name| {
                definition.cell != Cell::Uv
                    || ![
                        Step::DownloadOrdinaryRunner.name(),
                        Step::ProvisionOrdinaryRust.name(),
                        Step::OrdinaryJourney.name(),
                        Step::UploadOrdinaryEvidence.name(),
                    ]
                    .contains(name)
            })
            .collect();
        assert_eq!(
            actual,
            definition
                .steps
                .iter()
                .map(|step| step.name())
                .collect::<Vec<_>>(),
            "{:?}",
            definition.cell
        );
    }
    Ok(())
}

#[test]
fn source_keeps_both_clients_and_pip_only_native_journey() -> Result<(), String> {
    let consume = job("consume")?;
    validate_consume_guards(consume)?;
    for marker in [
        "fail-fast: false",
        "client: [pip, uv]",
        "runs-on: ubuntu-22.04",
        "test ! -e \"${GITHUB_WORKSPACE}/.git\"",
        "RIPR_ORDINARY_MODE=installed",
        "--exact --list",
        "--exact --nocapture",
        "test \"${native_status}\" -eq 0",
        ".positive_native.executed == 1",
        "name: ripr-rs-python-wheel-${{ env.CANDIDATE_SHA }}",
        "name: ripr-python-ordinary-runner-${{ env.CANDIDATE_SHA }}",
    ] {
        assert!(
            consume.contains(marker),
            "missing source constraint: {marker}"
        );
    }
    let applicability = job("applicability")?;
    assert!(applicability.contains("printf 'qualify=true\\n'"));
    assert!(applicability.contains("!contains(toJSON(inputs), '\"subject_sha\"')"));
    for name in ["source-contract", "build-wheel"] {
        let source = job(name)?;
        assert!(source.contains("needs: applicability"));
        assert!(source.contains("needs.applicability.outputs.qualify != 'false'"));
    }
    Ok(())
}

const CONSUME_GUARD: &str = "${{ always() && !cancelled() && needs.build-wheel.result == 'success' && needs.source-contract.result == 'success' }}";
const PIP_GUARD: &str = "${{ matrix.client == 'pip' }}";
const UPLOAD_GUARD: &str =
    "${{ always() && matrix.client == 'pip' && steps.ordinary.outcome != 'skipped' }}";
const DOWNLOAD_GUARD: &str =
    "github.event_name == 'merge_group' && inputs.budget_artifact_id != ''";

// This accepts only plain direct keys in the current layout, not arbitrary
// YAML. Unsupported syntax must fail, never disappear as an absent guard.
// Indentation separates metadata from shell strings and nested action inputs.
fn fields<'a>(source: &'a str, indent: usize, name: &str) -> Result<Vec<&'a str>, String> {
    let mut values = Vec::new();
    let prefix = " ".repeat(indent);
    for line in source.lines() {
        let Some(line) = line.strip_prefix(&prefix) else {
            continue;
        };
        if line.is_empty() || line.starts_with('#') || line.starts_with(char::is_whitespace) {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .ok_or("unsupported direct metadata syntax")?;
        if !key
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err("unsupported direct metadata syntax".into());
        }
        if key == name {
            values.push(value.trim());
        }
    }
    Ok(values)
}

fn require_field(
    source: &str,
    indent: usize,
    owner: &str,
    name: &str,
    expected: Option<&str>,
) -> Result<(), String> {
    let expected: Vec<_> = expected.into_iter().collect();
    if fields(source, indent, name).map_err(|error| format!("{owner}: {error}"))? != expected {
        return Err(format!("{owner}: unexpected {name}"));
    }
    Ok(())
}

fn consume_steps(source: &str) -> Result<Vec<&str>, String> {
    let (_, steps) = source
        .split_once("    steps:\n")
        .ok_or("consume steps missing")?;
    let steps = steps
        .strip_prefix("      - ")
        .ok_or("consume step layout differs")?;
    Ok(steps.split("\n      - ").collect())
}

fn named_step<'a>(steps: &[&'a str], step: Step) -> Result<&'a str, String> {
    let named: Vec<_> = steps
        .iter()
        .copied()
        .filter(|source| source.lines().next() == Some(format!("name: {}", step.name()).as_str()))
        .collect();
    let [source] = named.as_slice() else {
        return Err(format!("ambiguous consume step: {}", step.name()));
    };
    Ok(source)
}

fn wheel_download<'a>(steps: &[&'a str]) -> Result<&'a str, String> {
    let mut downloads = Vec::new();
    // Inspect action-input keys only on anonymous download actions. Applying
    // the input indentation to shell bodies would mistake commands for keys.
    for source in steps
        .iter()
        .copied()
        .filter(|source| source.starts_with("uses: actions/download-artifact@"))
    {
        if fields(source, 10, "name")? == ["ripr-rs-python-wheel-${{ env.CANDIDATE_SHA }}"] {
            downloads.push(source);
        }
    }
    let [download] = downloads.as_slice() else {
        return Err("ambiguous consume wheel download".into());
    };
    Ok(download)
}

fn step_guard(step: Step) -> Option<&'static str> {
    match step {
        Step::DownloadDeadlineVerifier => Some(DOWNLOAD_GUARD),
        Step::EnterDeadline => Some("github.event_name == 'merge_group'"),
        Step::DownloadOrdinaryRunner | Step::ProvisionOrdinaryRust | Step::OrdinaryJourney => {
            Some(PIP_GUARD)
        }
        Step::UploadOrdinaryEvidence => Some(UPLOAD_GUARD),
        Step::CompleteDeadline => {
            Some("${{ always() && !cancelled() && github.event_name == 'merge_group' }}")
        }
        _ => None,
    }
}

fn validate_consume_guards(source: &str) -> Result<(), String> {
    let (header, _) = source
        .split_once("    steps:\n")
        .ok_or("consume steps missing")?;
    require_field(header, 4, "consume", "if", Some(CONSUME_GUARD))?;
    require_field(
        header,
        4,
        "consume",
        "needs",
        Some("[build-wheel, source-contract]"),
    )?;
    require_field(header, 4, "consume", "continue-on-error", None)?;
    let steps = consume_steps(source)?;
    for step in Cell::Pip.definition().steps {
        let source = named_step(&steps, *step)?;
        require_field(source, 8, step.name(), "if", step_guard(*step))?;
        require_field(source, 8, step.name(), "continue-on-error", None)?;
    }
    let download = wheel_download(&steps)?;
    // The shared wheel download is unguarded and inherits the admitted job's
    // default success condition. A conditional skip cannot masquerade as it.
    require_field(download, 8, "wheel download", "if", None)?;
    require_field(download, 8, "wheel download", "continue-on-error", None)
}

#[test]
fn exact_consume_guards_reject_hostile_disjunctions_and_duplicates() -> Result<(), String> {
    let consume = job("consume")?;
    validate_consume_guards(consume)?;
    let steps = consume_steps(consume)?;
    let mut targets = vec![(
        "consume",
        consume.split_once("    steps:\n").ok_or("header")?.0,
        4,
        CONSUME_GUARD,
    )];
    for step in Cell::Pip.definition().steps {
        if let Some(guard) = step_guard(*step) {
            targets.push((step.name(), named_step(&steps, *step)?, 8, guard));
        }
    }
    for (owner, block, indent, guard) in targets {
        let weakened = if let Some(expression) = guard.strip_suffix(" }}") {
            format!("{expression} || true }}}}")
        } else {
            format!("{guard} || true")
        };
        let line = format!("{}if: {guard}", " ".repeat(indent));
        assert_eq!(block.matches(&line).count(), 1, "{owner}");
        for replacement in [
            format!("{}if: {weakened}", " ".repeat(indent)),
            format!("{line}\n{}if: ${{{{ true }}}}", " ".repeat(indent)),
            format!("{line}\n{}continue-on-error: true", " ".repeat(indent)),
        ] {
            let expected = if replacement.contains("continue-on-error") {
                "continue-on-error"
            } else {
                "if"
            };
            let changed = consume.replacen(block, &block.replacen(&line, &replacement, 1), 1);
            assert_eq!(
                validate_consume_guards(&changed),
                Err(format!("{owner}: unexpected {expected}"))
            );
        }
    }
    // Keeping the full guard while pruning its DAG is also unsupported.
    let changed = consume.replacen(
        "needs: [build-wheel, source-contract]",
        "needs: [build-wheel]",
        1,
    );
    assert_eq!(
        validate_consume_guards(&changed),
        Err("consume: unexpected needs".into())
    );
    Ok(())
}

#[test]
fn unguarded_shared_download_and_journeys_cannot_be_conditionally_skipped() -> Result<(), String> {
    let consume = job("consume")?;
    let steps = consume_steps(consume)?;
    for step in [
        Step::Install,
        Step::PythonJourney,
        Step::ReinstallAndUninstall,
    ] {
        let block = named_step(&steps, step)?;
        let changed = consume.replacen(block, &block.replacen('\n', "\n        if: false\n", 1), 1);
        assert_eq!(
            validate_consume_guards(&changed),
            Err(format!("{}: unexpected if", step.name()))
        );
    }
    let download = wheel_download(&steps)?;
    let changed = consume.replacen(
        download,
        &download.replacen('\n', "\n        if: false\n", 1),
        1,
    );
    assert_eq!(
        validate_consume_guards(&changed),
        Err("wheel download: unexpected if".into())
    );
    Ok(())
}

fn assert_unsupported_metadata(step: Step, inserted: &str) -> Result<(), String> {
    let consume = job("consume")?;
    validate_consume_guards(consume)?;
    let steps = consume_steps(consume)?;
    let block = named_step(&steps, step)?;
    let anchor = "        shell: bash\n";
    assert_eq!(block.matches(anchor).count(), 1);
    // After any existing `if`, so a quoted duplicate is a later conflicting
    // declaration rather than an earlier value hidden by the legitimate guard.
    let changed_block = block.replacen(anchor, &format!("{anchor}{inserted}\n"), 1);
    let changed = consume.replacen(block, &changed_block, 1);
    assert_eq!(
        validate_consume_guards(&changed),
        Err(format!(
            "{}: unsupported direct metadata syntax",
            step.name()
        ))
    );
    Ok(())
}

#[test]
fn unsupported_double_quoted_if_cannot_skip_a_journey() -> Result<(), String> {
    assert_unsupported_metadata(Step::PythonJourney, "        \"if\": false")
}

#[test]
fn unsupported_single_quoted_if_cannot_skip_a_journey() -> Result<(), String> {
    assert_unsupported_metadata(Step::PythonJourney, "        'if': false")
}

#[test]
fn unsupported_escaped_quoted_if_cannot_skip_a_journey() -> Result<(), String> {
    assert_unsupported_metadata(Step::PythonJourney, "        \"\\x69f\": false")
}

#[test]
fn unsupported_explicit_mapping_key_cannot_skip_a_journey() -> Result<(), String> {
    assert_unsupported_metadata(Step::PythonJourney, "        ? if\n        : false")
}

#[test]
fn unsupported_double_quoted_continue_on_error_cannot_hide_failure() -> Result<(), String> {
    assert_unsupported_metadata(Step::PythonJourney, "        \"continue-on-error\": true")
}

#[test]
fn unsupported_single_quoted_continue_on_error_cannot_hide_failure() -> Result<(), String> {
    assert_unsupported_metadata(Step::PythonJourney, "        'continue-on-error': true")
}

#[test]
fn unsupported_quoted_duplicates_cannot_override_a_real_guard() -> Result<(), String> {
    for key in ["\"if\"", "'if'", "\"\\x69f\""] {
        assert_unsupported_metadata(Step::OrdinaryJourney, &format!("        {key}: false"))?;
    }
    Ok(())
}
