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
the pull request the decision came from. To checkpoint the whole repository
instead, build it from a whole-tree `ripr check` with `ripr baseline create
--from-check` (see [Baselining the whole
repository](BASELINE_LEDGER_WORKFLOW.md#baselining-the-whole-repository)).
Full steps and review checklist:
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

## Why a new gap stayed advisory

`baseline-check` blocks a new gap only when the gate can hand the author a
complete, bounded repair route. If a new gap shows as `advisory` with a reason
such as `incomplete_repair_route: missing repair_target`, nothing is wrong with
the workflow: the gate names the fields it could not fill and stays advisory
rather than block on a repair it cannot describe. `repair_target` is the nearest
strong existing test to imitate (or a production caller), so a repository whose
only related tests are weak, such as `assert!(f(1) > 0)`, will not block until
a stronger test exists. Open `gate-decision.md` for the exact reason on each
candidate. The same applies when a test changed in the pull request next to the
gap: that candidate stays advisory with the reason stated.

## Fork and Dependabot pull requests

The generated workflow runs on `pull_request`, so a pull request from a fork or
from Dependabot gets a read-only `GITHUB_TOKEN`. The steps that write to the
pull request (the summary comment and the inline comments) carry a guard and
are skipped for those heads. The analysis (which also writes the gate
decision) and the `ripr-reports` artifact upload are not guarded and still
run, so a contributor and a maintainer can read the same `gate-decision.md`. SARIF upload needs `security-events: write`; it is
`continue-on-error`, and when it fails the `Explain failed SARIF upload` step
names the usual causes (a read-only token among them) without failing the job.
Nothing here makes a fork pull request a trusted run: the workflow does not use
`pull_request_target`, and it should not be changed to.

## Misconfiguration: baseline-check without a baseline

`baseline-check` with no baseline, or an invalid one, is a `config_error` and
the gate exits 2 (could not complete). It never acts as an empty baseline. The
message names the way out: use `visible-only` to start without blocking, then
run `ripr baseline create --from target/ripr/reports/gate-decision.json --out .ripr/gate-baseline.json`,
commit the result and pass it with `--baseline` (`RIPR_GATE_BASELINE`). A
`blocked` decision, by contrast, exits 3.

## GitLab merge requests

Steps 2 to 4 above are GitHub-only: they use the GitHub workflow's `RIPR_GATE_MODE`
and `RIPR_GATE_BASELINE` variables. For GitLab, `ripr init --ci gitlab` writes
`.gitlab-ci.yml` with one `ripr-advisory` job that runs only on merge-request
pipelines and has `allow_failure: true`, so it never blocks a merge request. It
runs `ripr check` and `ripr review-comments` and keeps `target/ripr/` as a job
artifact for 14 days. It does not post merge-request comments, render SARIF or
configure gate modes. For a merge request from a fork it fetches the target
project's branch as the diff base. It needs no secrets (only public release downloads), so
keep protected CI/CD variables out of merge-request pipelines. Details: [CI strategy](CI.md).

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
- [Coming from cargo-mutants](FROM_CARGO_MUTANTS.md): the `missed.txt` and
  exit-`2` equivalents, with a two-command `acknowledgeable` gate.
