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
            .filter(|name| *name != "Select wheel applicability from immutable event history")
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
    for marker in [
        "needs: [build-wheel, source-contract]",
        "needs.build-wheel.result == 'success' && needs.source-contract.result == 'success'",
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
    for step in [
        Step::DownloadOrdinaryRunner,
        Step::ProvisionOrdinaryRust,
        Step::OrdinaryJourney,
        Step::UploadOrdinaryEvidence,
    ] {
        let (_, source) = consume
            .split_once(&format!("      - name: {}\n", step.name()))
            .ok_or("missing pip-only step")?;
        let metadata = source
            .split("        run:")
            .next()
            .unwrap_or(source)
            .split("      - ")
            .next()
            .unwrap_or(source);
        assert!(
            metadata.contains("matrix.client == 'pip'"),
            "{} is no longer pip-only",
            step.name()
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
