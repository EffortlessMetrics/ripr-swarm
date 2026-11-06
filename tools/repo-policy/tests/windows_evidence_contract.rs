//! Source checks and bounded Linux replay of literal Bash evidence steps.
//! These do not execute Cargo, PowerShell or Windows.
use repo_policy::windows_evidence::{
    CARGO_EVIDENCE_OWNER, GROUP_OWNER, JOB, PRODUCERS, Producer, RUNNER_LABEL,
    SELECTOR_EVIDENCE_OWNER, SOURCE_OWNER, SOURCE_PATH, SUMMARY_OWNER, SUMMARY_PATH, UPLOAD_OWNER,
};

const WORKFLOW: &str = include_str!("../../../.github/workflows/windows-subset-qualification.yml");

// Freeze the complete executable decisions, not just condition or diagnostic
// tokens. Independent Linux replay below also challenges these expected bodies.
const SUMMARY_BODY: &str = r#"{
  printf '## Windows advisory subset (#4938)\n\n'
  printf 'Advisory only: not a required check and does not gate merges. Native exits (see terminal observation for verdicts):\n\n'
  for label in __LABELS__; do
  if [ "$label" = subset-selector-literal ] && [ "$SELECTOR_CONSUMER_ENABLED" != true ]; then continue; fi
    status_file="target/${label}.status"
    if [ ! -f "$status_file" ]; then
      echo "- ${label}: not_run (no status file)"
    elif [ "$(cat "$status_file")" = "0" ]; then
      echo "- ${label}: exit 0"
    else
      echo "- ${label}: nonzero exit ($(cat "$status_file"))"
    fi
  done
} | tee target/windows-subset-verdict.md | tee -a "$GITHUB_STEP_SUMMARY""#;

const CARGO_GUARD_BODY: &str = r#"for label in __LABELS__; do
  for suffix in status log; do
    if [ ! -f "target/${label}.${suffix}" ] || [ ! -s "target/${label}.${suffix}" ]; then
      echo "${label}: missing or empty ${suffix} evidence" >&2
      exit 1
    fi
  done
  if ! LC_ALL=C awk 'NR != 1 || $0 !~ /^(0|[1-9][0-9]*)$/ || length($0) > 3 || $0 + 0 > 255 { exit 1 } END { if (NR != 1) exit 1 }' "target/${label}.status"; then
    echo "${label}: malformed native exit status" >&2
    exit 1
  fi
  status="$(cat "target/${label}.status")" || exit 1
  if [ "$label" != subset-clippy ] && [ "$status" = "0" ]; then
    executed="$(awk '/^test result:/{for (i = 1; i <= NF; i++) if ($i == "passed;" || $i == "failed;" || $i == "measured;") executed += $(i - 1)} END {print executed + 0}' "target/${label}.log")"
    if [ "$executed" -eq 0 ]; then
      echo "${label}: cargo exited 0 but no test executed; the name filter no longer matches any test" >&2
      exit 1
    fi
  fi
done"#;

fn labels(producers: &[Producer]) -> String {
    producers
        .iter()
        .map(|producer| producer.contract().label)
        .collect::<Vec<_>>()
        .join(" ")
}

fn bash_body(source: &str, name: &str) -> Result<String, String> {
    let owner = step(source, name)?;
    let (_, body) = owner
        .split_once("        shell: bash\n        run: |\n")
        .ok_or_else(|| format!("missing explicit Bash body: {name}"))?;
    let lines = body.lines().collect::<Vec<_>>();
    let end = lines
        .iter()
        .take_while(|line| line.is_empty() || line.starts_with("          "))
        .count();
    require(
        lines[end..]
            .iter()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#')),
        "unexpected step metadata after Bash body",
    )?;
    Ok(lines[..end]
        .iter()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_owned())
}

fn change_step(name: &str, before: &str, after: &str) -> Result<String, String> {
    let original = step(WORKFLOW, name)?;
    require(
        original.matches(before).count() == 1,
        "mutation missed its unique subject",
    )?;
    Ok(WORKFLOW.replacen(original, &original.replacen(before, after, 1), 1))
}

fn forced_pass_summary() -> Result<String, String> {
    change_step(
        SUMMARY_OWNER,
        "elif [ \"$(cat \"$status_file\")\" = \"0\" ]; then",
        "elif true; then",
    )
}

fn omitted_cargo_home_summary() -> Result<String, String> {
    change_step(
        SUMMARY_OWNER,
        "for label in subset-gap-artifacts subset-cargo-home subset-cache-warning subset-clippy subset-selector-literal; do",
        "for label in subset-gap-artifacts subset-cache-warning subset-clippy subset-selector-literal; do",
    )
}

fn disabled_zero_test_guard() -> Result<String, String> {
    change_step(
        CARGO_EVIDENCE_OWNER,
        "echo \"${label}: cargo exited 0 but no test executed; the name filter no longer matches any test\" >&2\n                exit 1",
        "echo \"${label}: cargo exited 0 but no test executed; the name filter no longer matches any test\" >&2\n                true",
    )
}

fn step<'a>(source: &'a str, name: &str) -> Result<&'a str, String> {
    let marker = format!("      - name: {name}\n");
    let mut matches = source.match_indices(&marker);
    let start = matches
        .next()
        .ok_or_else(|| format!("missing step {name}"))?
        .0;
    if matches.next().is_some() {
        return Err(format!("duplicate step {name}"));
    }
    Ok(source[start..]
        .split("\n      - ")
        .next()
        .unwrap_or_default())
}

fn require(condition: bool, reason: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(reason.into())
    }
}

fn check_source(source: &str) -> Result<(), String> {
    require(
        source.contains(&format!(
            "\n  {JOB}:\n    name: {JOB}\n    runs-on: {RUNNER_LABEL}\n"
        )),
        "Windows job identity drift",
    )?;
    let job_marker = format!("\n  {JOB}:\n");
    let header = source
        .split_once(&job_marker)
        .ok_or("missing Windows job")?
        .1
        .split("    steps:\n")
        .next()
        .unwrap_or_default();
    require(
        header.contains("    env:\n      SELECTOR_CONSUMER_ENABLED: \"true\"\n"),
        "the complete five-producer summary requires selector evidence enabled",
    )?;
    for producer in PRODUCERS {
        let contract = producer.contract();
        let owner = step(source, contract.producer.name)?;
        require(
            contract.producer.id.is_none() && contract.evidence_owner.id.is_none(),
            "do not invent step ids",
        )?;
        if let Some(command) = contract.command {
            // Match the executable wrapper, not an arbitrary command mention.
            let expected = format!(
                "          mkdir -p target || exit 1; set +e; {command} 2>&1 | tee {}; pipe_status=(\"${{PIPESTATUS[@]}}\"); printf '%s\\n' \"${{pipe_status[0]}}\" > {} || {{ echo \"subset evidence status write failed: {}\" >&2; exit 1; }}; if [ \"${{pipe_status[1]}}\" != \"0\" ]; then echo \"subset evidence capture failed: tee exited ${{pipe_status[1]}} writing {}\" >&2; exit 1; fi; exit 0",
                contract.log, contract.status, contract.label, contract.log
            );
            require(
                owner.contains("        shell: bash\n        run: |\n"),
                "Cargo shell drift",
            )?;
            require(
                owner.lines().any(|line| line == expected),
                "Cargo advisory/capture ownership drift",
            )?;
        } else {
            require(
                producer == Producer::SelectorRecovery,
                "unexpected inline producer",
            )?;
            require(
                owner.contains("        shell: pwsh\n"),
                "selector must retain native PowerShell",
            )?;
            let tail = owner
                .split("          '@\n")
                .last()
                .unwrap_or_default()
                .lines()
                .filter(|line| !line.trim_start().starts_with('#'))
                .collect::<Vec<_>>()
                .join("\n");
            require(tail.contains("try { $consumer | python - 2>&1 | Tee-Object -FilePath target/subset-selector-literal.log -ErrorAction Stop"), "selector capture owner drift")?;
            require(tail.contains("$nativeExit = $LASTEXITCODE; Set-Content -LiteralPath target/subset-selector-literal.status -Value $nativeExit -ErrorAction Stop"), "selector native status owner drift")?;
            require(
                tail.contains("} catch { [Console]::Error.WriteLine($_); exit 1 }")
                    && tail.trim_end().ends_with("exit 0"),
                "selector advisory failure must remain separate from capture failure",
            )?;
        }
        require(
            step(source, contract.evidence_owner.name)?.contains("        shell: bash\n"),
            "missing independent evidence owner",
        )?;
    }
    require(
        bash_body(source, CARGO_EVIDENCE_OWNER)?
            == CARGO_GUARD_BODY.replace("__LABELS__", &labels(&PRODUCERS[..4])),
        "Cargo evidence guard decision or complete producer set drift",
    )?;
    let selector_guard = step(source, SELECTOR_EVIDENCE_OWNER)?;
    require(
        selector_guard.contains("if status != (0 if receipt[\"state\"] == \"PASS\" else 1):")
            && selector_guard.contains("if logged != receipt"),
        "selector status/receipt/log owner drift",
    )?;
    require(
        selector_guard.contains(
            "if receipt[\"state\"] == \"FAIL\":\n              if not receipt.get(\"error\"):",
        ),
        "captured selector failure must retain its reason",
    )?;
    require(
        bash_body(source, SUMMARY_OWNER)?
            == SUMMARY_BODY.replace("__LABELS__", &labels(&PRODUCERS)),
        "summary's complete five-producer status mapping or output ownership drift",
    )?;
    let source_guard = step(source, SOURCE_OWNER)?;
    let group_guard = step(source, GROUP_OWNER)?;
    require(
        group_guard.contains("        if: inputs.proof_profile == 'merge-group'\n")
            && group_guard.contains("        uses: ./.github/actions/verify-merge-group-subject\n"),
        "native composition verification owner missing",
    )?;
    for binding in [
        "head = $env:ADMITTED_HEAD; base = $env:ADMITTED_BASE;",
        "subject = $actual; tree = $tree; workflow_generation = $env:ADMITTED_GENERATION;",
        "producer_run = $env:GITHUB_RUN_ID; producer_attempt = $env:GITHUB_RUN_ATTEMPT;",
        "workflow_ref = $env:GITHUB_WORKFLOW_REF; workflow_sha = $env:GITHUB_WORKFLOW_SHA",
    ] {
        require(
            source_guard.contains(binding),
            "source receipt identity field drift",
        )?;
    }
    require(
        source_guard.contains(&format!("Set-Content -LiteralPath {SOURCE_PATH}")),
        "source receipt output drift",
    )?;
    let upload = step(source, UPLOAD_OWNER)?;
    require(
        upload.contains("        if: always()\n"),
        "failed evidence must remain uploadable",
    )?;
    require(
        upload.contains("          name: ripr-windows-advisory-subset-${{ inputs.request_id }}\n"),
        "upload name custody drift",
    )?;
    let paths: Vec<_> = upload
        .split("          path: |\n")
        .nth(1)
        .unwrap_or_default()
        .lines()
        .take_while(|line| line.starts_with("            "))
        .map(str::trim)
        .collect();
    let mut expected = vec![
        SOURCE_PATH,
        "${{ steps.terminal_record.outcome == 'success' && 'target/windows-subset-terminal.json' || '' }}",
    ];
    for producer in &PRODUCERS[..4] {
        let contract = producer.contract();
        expected.extend([contract.log, contract.status]);
    }
    expected.push(SUMMARY_PATH);
    let selector = Producer::SelectorRecovery.contract();
    expected.extend([selector.log, selector.status, "target/selector-literal/"]);
    require(paths == expected, "upload evidence path denominator drift")?;
    Ok(())
}

#[test]
fn definitions_follow_actual_windows_producers_and_separate_verdict_owners() -> Result<(), String> {
    check_source(WORKFLOW)
}

#[test]
fn hostile_source_changes_cannot_silently_promote_or_drop_advisory_evidence() {
    for (before, after) in [
        ("runs-on: windows-latest", "runs-on: ubuntu-latest"),
        (
            "SELECTOR_CONSUMER_ENABLED: \"true\"",
            "SELECTOR_CONSUMER_ENABLED: \"false\"",
        ),
        (
            "cargo test -p ripr --lib lsp::gap_artifacts",
            "cargo test -p ripr --lib wrong_filter",
        ),
        ("pipe_status[0]", "pipe_status[1]"),
        ("fi; exit 0", "fi; exit 101"),
        ("$nativeExit = $LASTEXITCODE", "$nativeExit = 0"),
        ("if [ \"$executed\" -eq 0 ]; then", "if false; then"),
        ("if logged != receipt", "if False"),
        (
            "Advisory only: not a required check and does not gate merges.",
            "Tests passed and gate merges.",
        ),
        ("            target/subset-clippy.status\n", ""),
        (
            "producer_attempt = $env:GITHUB_RUN_ATTEMPT",
            "producer_attempt = 1",
        ),
    ] {
        assert!(
            WORKFLOW.contains(before),
            "mutation missed its subject: {before}"
        );
        assert!(
            check_source(&WORKFLOW.replace(before, after)).is_err(),
            "mutation persisted: {before}"
        );
    }
}

#[test]
fn upload_and_source_receipt_remain_distinct_from_runtime_custody() -> Result<(), String> {
    let upload = step(WORKFLOW, UPLOAD_OWNER)?;
    assert!(upload.contains("          if-no-files-found: error"));
    assert!(upload.contains("        id: subset_upload"));
    let receipt = step(WORKFLOW, SOURCE_OWNER)?
        .split("          @{ proof_profile")
        .nth(1)
        .ok_or("missing receipt")?;
    assert!(!receipt.contains("runner_os"));
    assert!(!receipt.contains("job_id"));
    assert!(!receipt.contains("artifact_id"));
    assert!(!WORKFLOW.contains("windows_evidence"));
    Ok(())
}

#[test]
fn source_contract_refuses_forced_pass_summary() -> Result<(), String> {
    require(
        check_source(&forced_pass_summary()?).is_err(),
        "forced-PASS summary persisted",
    )
}

#[test]
fn source_contract_refuses_omitted_cargo_home_summary() -> Result<(), String> {
    require(
        check_source(&omitted_cargo_home_summary()?).is_err(),
        "omitted Cargo-home summary persisted",
    )
}

#[test]
fn source_contract_refuses_noop_zero_test_guard() -> Result<(), String> {
    require(
        check_source(&disabled_zero_test_guard()?).is_err(),
        "no-op zero-test guard persisted",
    )
}

#[cfg(target_os = "linux")]
#[path = "support/windows_evidence_replay.rs"]
mod replay;

#[test]
fn terminal_observation_is_after_producers_and_guards_and_before_upload() -> Result<(), String> {
    for (name, id) in [
        (SOURCE_OWNER, "source_guard"),
        (GROUP_OWNER, "group_guard"),
        (PRODUCERS[0].contract().producer.name, "gap"),
        (PRODUCERS[1].contract().producer.name, "cargo_home"),
        (PRODUCERS[2].contract().producer.name, "cache_warning"),
        (PRODUCERS[3].contract().producer.name, "clippy"),
        (PRODUCERS[4].contract().producer.name, "selector"),
        (CARGO_EVIDENCE_OWNER, "cargo_guard"),
        (SELECTOR_EVIDENCE_OWNER, "selector_guard"),
        (UPLOAD_OWNER, "subset_upload"),
    ] {
        require(
            step(WORKFLOW, name)?.contains(&format!("        id: {id}\n")),
            "outcome ID points at wrong owner",
        )?;
    }
    let terminal = step(WORKFLOW, "Record post-execution Windows observation")?;
    require(
        terminal.contains("        id: terminal_record\n"),
        "terminal outcome ID missing",
    )?;
    let upload = step(WORKFLOW, UPLOAD_OWNER)?;
    let custody = step(WORKFLOW, "Record observed upload outcome")?;
    let terminal_at = WORKFLOW.find(terminal).ok_or("terminal step missing")?;
    for name in [CARGO_EVIDENCE_OWNER, SELECTOR_EVIDENCE_OWNER, SUMMARY_OWNER] {
        require(
            WORKFLOW
                .find(step(WORKFLOW, name)?)
                .ok_or("guard missing")?
                < terminal_at,
            "terminal record precedes evidence",
        )?;
    }
    require(
        terminal_at < WORKFLOW.find(upload).ok_or("upload missing")?,
        "record is not uploaded after production",
    )?;
    require(
        WORKFLOW.find(upload).ok_or("upload missing")?
            < WORKFLOW.find(custody).ok_or("custody missing")?,
        "artifact identity precedes upload",
    )?;
    for binding in [
        "steps.gap.outcome",
        "steps.cargo_home.outcome",
        "steps.cache_warning.outcome",
        "steps.clippy.outcome",
        "steps.selector.outcome",
        "steps.cargo_guard.outcome",
        "steps.selector_guard.outcome",
        "steps.group_guard.outcome",
        "steps.source_guard.outcome",
    ] {
        require(terminal.contains(binding), "missing observed outcome")?;
    }
    require(
        terminal.contains("\"authority\": \"not_established\"")
            && terminal.contains("\"candidate_authored\": True"),
        "authority limitation missing",
    )?;
    require(
        custody.contains("steps.subset_upload.outputs.artifact-id")
            && custody.contains("steps.subset_upload.outcome"),
        "upload observation missing",
    )?;
    Ok(())
}

#[test]
fn unconditional_terminal_upload_is_rejected() -> Result<(), String> {
    let conditional = "${{ steps.terminal_record.outcome == 'success' && 'target/windows-subset-terminal.json' || '' }}";
    require(
        WORKFLOW.matches(conditional).count() == 1,
        "terminal upload condition is missing or duplicated",
    )?;
    let stale_upload = WORKFLOW.replacen(conditional, "target/windows-subset-terminal.json", 1);
    require(
        check_source(&stale_upload).is_err(),
        "unconditional stale terminal upload accepted",
    )
}
