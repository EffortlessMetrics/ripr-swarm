# Adopting RIPR In CI

This is the short, ordered path for a team adding RIPR to GitHub CI for the
first time. Each step links to the page that owns the detail; this page does not
restate their contracts.

```text
1. generate the workflow          (advisory, never fails the job)
2. visible-only                   (runs the gate, still never blocks)
3. create a reviewed baseline     (its own PR)
4. baseline-check                 (blocks only new policy-eligible gaps)
```

All findings use static vocabulary. A baseline checkpoints reviewed static
findings; it is not a suppression and not runtime mutation evidence.

## 1. Generate the workflow

```bash
ripr init --ci github
```

Add `--dry-run` to print the workflow without writing it. The generated
workflow reads two repository variables and leaves both unset, so it is
advisory only: it summarizes findings and uploads reports, and the job never
fails.

```text
RIPR_GATE_MODE=
RIPR_GATE_BASELINE=
```

Details: [CI strategy](CI.md), generated workflow section.

## 2. Turn on visible-only

Set the repository variable (Settings > Secrets and variables > Actions >
Variables):

```text
RIPR_GATE_MODE=visible-only
```

`visible-only` runs the gate and writes
`target/ripr/reports/gate-decision.json` and `gate-decision.md` without
blocking the job. Review them; do not baseline output that has not been reviewed. A
repository variable is not carried by a pull request, so push or re-run a pull
request after setting it.

Other `RIPR_GATE_MODE` values are `acknowledgeable`, `baseline-check` and
`calibrated-gate`. Decide when to leave `visible-only` with
[blocking readiness](BLOCKING_READINESS.md).

Optional: `RIPR_COMMENT_MODE` is `off` (default), `plan` or `inline` and only
controls PR review comments; it does not change gate behavior.

## 3. Create a baseline

Download the `ripr-reports` artifact from a representative `visible-only` run,
then checkpoint its reviewed gate decision:

```bash
ripr baseline create \
  --from target/ripr/reports/gate-decision.json \
  --out .ripr/gate-baseline.json
```

`--from` is the downloaded `gate-decision.json`; the path above is where the
workflow writes it. The command refuses to overwrite an existing output unless
`--force` is passed. Review `.ripr/gate-baseline.json` before committing it in
its own PR, still in `visible-only`. The baseline is scoped to
the pull request the decision came from. Full steps and review checklist:
[baseline ledger workflow](BASELINE_LEDGER_WORKFLOW.md).

## 4. Enable baseline-check

After the baseline PR is merged, set both variables:

```text
RIPR_GATE_MODE=baseline-check
RIPR_GATE_BASELINE=.ripr/gate-baseline.json
```

Findings already in the baseline stay visible and non-blocking; new
policy-eligible gaps block. Repair a blocked finding with a focused test, or
acknowledge it visibly as the baseline ledger workflow describes.

## Misconfiguration: baseline-check without a baseline

`baseline-check` with no baseline, or an invalid one, is a `config_error` and
the gate exits 2 (could not complete). It never acts as an empty baseline. The
message names the way out: use `visible-only` to start without blocking, then
run `ripr baseline create --from target/ripr/reports/gate-decision.json --out .ripr/gate-baseline.json`,
commit the result and pass it with `--baseline` (`RIPR_GATE_BASELINE`). A
`blocked` decision, by contrast, exits 3.

## Rolling back

Unset `RIPR_GATE_MODE` to return to advisory-only. The job is
`continue-on-error` unless the mode names a blocking mode.

## Where to go next

- [CI strategy](CI.md): workflow reference, variables and artifacts.
- [Baseline ledger workflow](BASELINE_LEDGER_WORKFLOW.md): baseline creation,
  diff and shrink-only updates.
- [Blocking readiness](BLOCKING_READINESS.md): when each gate mode is ready.
- [Calibrated gate policy](CALIBRATED_GATE_POLICY.md): the mode after
  `baseline-check`.
