//! Bounded source contract and real domain-entry tests; no Windows/CI execution.

const LEGACY: &str = include_str!("../../fixtures/convergence/windows-subset/qualification.yml");
const STAGED: &str = include_str!("../../.github/workflows/windows-subset-qualification.yml");
const PRECHECK: &str =
    "      - name: Verify admitted integration source before product execution\n";

fn evidence_step(source: &str) -> Result<&str, String> {
    let (_, step) = source
        .split_once("      - name: Fail on missing or empty subset evidence\n")
        .ok_or("missing evidence guard")?;
    step.split_once("      - name: Require selector consumer receipt, log and native status\n")
        .map(|(body, _)| body)
        .ok_or_else(|| "missing selector guard".to_owned())
}

fn normalized_job(source: &str) -> Result<String, String> {
    let (_, job) = source
        .split_once("  windows-advisory-subset:\n")
        .ok_or_else(|| "missing Windows subset job".to_owned())?;
    let mut job = job.to_owned();
    if let Some((before, tail)) = job.split_once(PRECHECK) {
        // Frozen predecessors name the tag; mainline pins the commit.
        let (toolchain, after) = [
            "      - uses: dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591 # 1.95\n",
            "      - uses: dtolnay/rust-toolchain@1.95\n",
        ]
        .iter()
        .find_map(|toolchain| tail.split_once(toolchain).map(|(_, after)| (*toolchain, after)))
        .ok_or_else(|| "precheck displaced the native toolchain".to_owned())?;
        job = format!("{before}{toolchain}{after}");
    }
    let trust = "github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository";
    job = job.replace(&format!("${{{{ {trust} }}}}"), "\"true\"")
        .replace(&format!("        if: {trust}\n"), "")
        .replace("${{ github.event.pull_request.head.sha || github.sha }}", "${{ inputs.subject_sha }}")
        .replace("          fetch-depth: 0\n", "")
        .replace("            target/windows-subset-source.json\n", "")
        .replace("          name: ripr-windows-advisory-subset-${{ inputs.request_id }}\n", "          name: ripr-windows-advisory-subset\n")
        .replace("      # Bind native subset/consumer proof to the immutable PR head; the required\n      # Rust lane separately owns its integration checkout.",
            "      # Native merge-ready preflight admits H/B/T before invoking this inactive template.\n      # Product execution consumes exact T; it has no admission/check-write authority.");
    // Extraction changes the embedded source-test owner, not the consumer oracle.
    job = job.replace(
        "WORKFLOW = Path.cwd() / '.github/workflows/windows-advisory.yml'",
        "WORKFLOW = Path.cwd() / '.github/workflows/windows-subset-qualification.yml'",
    );
    // The prototype stays frozen. Only the four-pair guard is deliberately
    // superseded; windows_subset_evidence replays its actual replacement.
    // All native commands, selector receipt oracle and cleanup remain compared.
    job = job.replace(evidence_step(STAGED)?, evidence_step(LEGACY)?);
    job = job.replace(
        "mkdir -p target || exit 1; set +e; cargo",
        "mkdir -p target; set +e; cargo",
    );
    for label in [
        "subset-gap-artifacts",
        "subset-cargo-home",
        "subset-cache-warning",
        "subset-clippy",
    ] {
        job = job.replace(
            &format!(
                " || {{ echo \"subset evidence status write failed: {label}\" >&2; exit 1; }}; if"
            ),
            "; if",
        );
    }
    job = job.replace(
        "          try { $consumer | python - 2>&1 | Tee-Object -FilePath target/subset-selector-literal.log -ErrorAction Stop\n            $nativeExit = $LASTEXITCODE; Set-Content -LiteralPath target/subset-selector-literal.status -Value $nativeExit -ErrorAction Stop\n          } catch { [Console]::Error.WriteLine($_); exit 1 }",
        "          $consumer | python - 2>&1 | Tee-Object -FilePath target/subset-selector-literal.log\n          $nativeExit = $LASTEXITCODE\n          Set-Content -LiteralPath target/subset-selector-literal.status -Value $nativeExit",
    );
    Ok(job)
}

// Keep the prototype frozen and forward only the reviewed quotation repair.
// Expected literals are pinned here; they are never read from STAGED at runtime.
// The native Windows replay separately confirms the producer's double quotes.
fn expected_job_with_recovery_quote_repair() -> Result<String, String> {
    let mut expected = normalized_job(LEGACY)?;
    for (before, after) in [
        (
            r###"              def test_windows_recovery_matches_published_slashes_and_keeps_literal(self):
                  # Inert Windows-shaped sentinels: no real drive or directory is accessed.
                  drive = 'Q' + ':'
                  root = drive + r'\synthetic-selector-fixture literal'
                  diff = root + r'\fixture change.diff'
                  expected = f"ripr check --root '{drive}/synthetic-selector-fixture literal' --diff '{drive}/synthetic-selector-fixture literal/fixture change.diff' --json"
                  self.assertEqual(self.ns['recovery_command'](root, diff, windows=True), expected)
                  stderr = ('No finding. Retry `' + expected + '`.').encode()
                  self.assertEqual(self.ns['printed_recovery'](stderr, root, diff, windows=True), expected)

"###,
            r###"              def test_windows_recovery_matches_published_slashes_and_keeps_literal(self):
                  # Inert Windows-shaped sentinels: no real drive or directory is accessed.
                  drive = 'Q' + ':'
                  root = drive + r'\synthetic-selector-fixture literal'
                  diff = root + r'\fixture change.diff'
                  expected = f'ripr check --root "{drive}/synthetic-selector-fixture literal" --diff "{drive}/synthetic-selector-fixture literal/fixture change.diff" --json'
                  self.assertEqual(self.ns['recovery_command'](root, diff, windows=True), expected)
                  stderr = ('No finding. Retry `' + expected + '`.').encode()
                  self.assertEqual(self.ns['printed_recovery'](stderr, root, diff, windows=True), expected)
                  stale = expected.replace('"', "'")
                  for wrong in (stale, expected.replace('--json', '--no-cache'),
                                expected.replace('fixture change.diff', 'wrong.diff'),
                                expected.replace('synthetic-selector-fixture literal', 'wrong root', 1)):
                      with self.subTest(command=wrong), self.assertRaises(RuntimeError):
                          self.ns['printed_recovery'](('`' + wrong + '`').encode(), root, diff, windows=True)

"###,
        ),
        (
            r###"              return f"ripr check --root '{paths[0]}' --diff '{paths[1]}' --json""###,
            r###"              quote = '"' if windows else "'"
              return f"ripr check --root {quote}{paths[0]}{quote} --diff {quote}{paths[1]}{quote} --json""###,
        ),
    ] {
        assert_eq!(
            expected.matches(before).count(),
            1,
            "frozen quotation anchor drift"
        );
        expected = expected.replacen(before, after, 1);
    }
    Ok(expected)
}

#[test]
fn production_producer_preserves_all_native_commands_oracles_and_cleanup() -> Result<(), String> {
    assert!(
        STAGED.contains(
            "WORKFLOW = Path.cwd() / '.github/workflows/windows-subset-qualification.yml'"
        )
    );
    // The retained predecessor names action tags; mainline pins their commits.
    let unpinned = normalized_job(STAGED)?
        .replace(
            "dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591 # 1.95",
            "dtolnay/rust-toolchain@1.95",
        )
        .replace(
            "Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2",
            "Swatinem/rust-cache@v2",
        )
        // The embedded Python assertions search the workflow for the pinned
        // `uses:` spelling, so they carry the commit with no trailing comment.
        .replace(
            "uses: Swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0'",
            "uses: Swatinem/rust-cache@v2'",
        );
    assert_eq!(unpinned, expected_job_with_recovery_quote_repair()?);
    Ok(())
}

#[test]
fn subset_command_removal_is_detected() -> Result<(), String> {
    let bad = STAGED.replace(
        "Run Cargo-home identity subset",
        "Removed Cargo-home identity subset",
    );
    assert_ne!(bad, STAGED);
    assert_ne!(
        normalized_job(&bad)?,
        expected_job_with_recovery_quote_repair()?
    );
    Ok(())
}

#[test]
fn recovery_quote_scope_and_cleanup_drift_still_break_preservation() -> Result<(), String> {
    let expected = expected_job_with_recovery_quote_repair()?;
    for (before, after) in [
        (r###"quote = '"' if windows else "'""###, "quote = chr(39)"),
        (
            "expected.replace('--json', '--no-cache')",
            "expected.replace('--json', '--wrong')",
        ),
        (
            "receipt[\"producer_cleanup\"] = remove_aliases(ownership)",
            "receipt[\"producer_cleanup\"] = {}",
        ),
    ] {
        assert_eq!(STAGED.matches(before).count(), 1, "control anchor drift");
        let changed = STAGED.replacen(before, after, 1);
        assert_ne!(changed, STAGED);
        assert_ne!(normalized_job(&changed)?, expected);
    }
    Ok(())
}

#[test]
fn production_callee_has_only_call_admission_and_no_write_authority() {
    let header = STAGED.split("\njobs:\n").next().unwrap_or("");
    assert!(header.contains("on:\n  workflow_call:\n"));
    for forbidden in [
        "  pull_request:",
        "  push:",
        "  workflow_dispatch:",
        "  merge_group:",
        "checks: write",
        "actions: write",
        "pull-requests: write",
    ] {
        assert!(
            !header.contains(forbidden),
            "unexpected admission/authority {forbidden}"
        );
    }
    for field in [
        "request_id",
        "head_sha",
        "subject_sha",
        "workflow_generation",
    ] {
        assert!(header.contains(&format!(
            "      {field}:\n        type: string\n        required: true"
        )));
    }
    assert!(
        !include_str!("../../.github/workflows/windows-advisory.yml")
            .contains("fixtures/convergence/windows-subset/qualification.yml")
    );
}

#[test]
fn source_precheck_precedes_candidate_execution_and_keeps_source_receipt() -> Result<(), String> {
    let check = STAGED
        .find(PRECHECK)
        .ok_or_else(|| "missing source precheck".to_owned())?;
    let toolchain = STAGED
        .find("      - uses: dtolnay/rust-toolchain@035fceb3c12fb0c6a9461b52775e16473133e591 # 1.95\n")
        .ok_or_else(|| "missing toolchain".to_owned())?;
    assert!(check < toolchain);
    for clause in [
        "$actual -cne $env:ADMITTED_SUBJECT",
        "$tree -cne $env:ADMITTED_TREE",
        "$parents -cne",
        "$env:GITHUB_WORKFLOW_SHA -cne $env:ADMITTED_GENERATION",
        "producer_attempt = $env:GITHUB_RUN_ATTEMPT",
        "target/windows-subset-source.json",
    ] {
        assert!(STAGED.contains(clause), "missing source binding {clause}");
    }
    Ok(())
}
