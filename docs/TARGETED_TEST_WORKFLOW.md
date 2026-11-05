# Targeted Test Workflow

Use this workflow when you want to turn RIPR seam evidence into one focused
test and leave a reviewable receipt.

The loop is:

```text
before static seam evidence
-> pick one weak or missing discriminator
-> copy the targeted test brief or seam packet
-> add one focused test
-> after static seam evidence
-> targeted-test outcome receipt
-> agent verify and agent receipt when an agent or reviewer needs a focused packet
-> optional SARIF, badge, and runtime calibration checks
```

RIPR is still a static test-grip tool. The receipt includes a reviewer-native
review receipt that says what changed, what RIPR flagged before, which focused
proof signals moved outside RIPR, what remains weak or unknown, and what
reviewers should inspect or avoid inferring. It does not claim runtime mutation
confirmation, coverage adequacy, merge approval, automatic source edits,
generated tests, or provider/model execution.

## Inputs and Outputs

The workflow uses these artifacts:

| Artifact | Purpose |
| --- | --- |
| `repo-exposure.json` | Before/after classified seam evidence. |
| `repo-exposure.md` | Human-readable seam list for picking the next gap. |
| `agent-seam-packets.json` | Tool-readable work orders for targeted tests. |
| LSP seam diagnostics and hovers | Editor path for inspecting evidence and copying briefs. |
| `ripr outcome` receipt | Advisory receipt comparing before and after snapshots. |
| `agent-verify.json` | Agent-facing before/after comparison generated from the same snapshots. |
| `analysis-outcome.json` | Producer check JSON placed beside the verify file; the receipt validates its root, completeness and applicable current input identity. |
| `agent-receipt.json` | Focused static receipt for one seam; inspect its status and binding before review handoff. |
| `repo-sarif` / `sarif-policy` | Optional CI/code-scanning projection with the same seam semantics. |
| repo badge artifacts | Optional public count projection under badge policy. |
| `mutation-calibration.{json,md}` | Optional runtime calibration join when cargo-mutants data exists. |
| [`fixtures/EXAMPLE_CORPUS.md`](../fixtures/EXAMPLE_CORPUS.md) | Public defaults-first example corpus with CLI, LSP, receipt, and optional calibration artifacts. |
| [`fixtures/CALIBRATION_CORPUS.md`](../fixtures/CALIBRATION_CORPUS.md) | Controlled fixture index for trying the loop on known seam scenarios. |

`ripr pilot`, `ripr check`, `ripr outcome`, and
`ripr calibrate cargo-mutants` work from an installed `ripr` binary. Cockpit
and badge-endpoint commands shown here remain repo-local `cargo xtask`
automation today.

`ripr init` is optional. Run it only if you want to commit repo-local policy
before starting the loop — it materializes the built-in defaults into
`ripr.toml` so the team can review, version, and tune them. Missing config is
the normal first-run state and uses the same defaults.

[RIPR-SPEC-0009](specs/RIPR-SPEC-0009-defaults-first-adoption.md) tracks the
remaining public CLI path for this loop. `ripr outcome` owns the before/after
receipt, and `ripr calibrate cargo-mutants` owns the advisory runtime
calibration import.

Use a local scratch directory for before/after snapshots. The loop's snapshot
commands redirect into this directory, and every shell opens the redirect
before `ripr` runs, so create it first — in the shell that will run the
snapshots (a fresh workspace has no `target/` yet):

```bash
mkdir -p target/ripr/workflow
```

```cmd
mkdir target\ripr\workflow
```

```powershell
New-Item -ItemType Directory -Force -Path target/ripr/workflow | Out-Null
```

## 0. Generate a Pilot Packet

For a first run, start with the installed CLI:

```bash
ripr pilot
```

Missing `ripr.toml` is a healthy state. The command uses built-in conservative
defaults and writes:

```text
target/ripr/pilot/repo-exposure.json
target/ripr/pilot/repo-exposure.md
target/ripr/pilot/agent-seam-packets.json
target/ripr/pilot/pilot-summary.json
target/ripr/pilot/pilot-summary.md
```

Use `pilot-summary.md` as the front panel: it lists the top actionable seam,
why RIPR ranked it, the targeted test brief, and the after-snapshot command.
If analysis exceeds the pilot budget, the summary is still written with
`status: partial` and a retry command so the first-run path is explicit.
If you want a custom workflow directory or stronger mode:

```bash
ripr pilot --root . --out target/ripr/workflow --mode ready --timeout-ms 120000
```

## 1. Capture the Before Snapshot

From an installed `ripr` binary:

```bash
ripr check --root . --mode ready --format repo-exposure-json > target/ripr/workflow/before.repo-exposure.json
ripr check --root . --mode ready --format repo-exposure-md > target/ripr/workflow/before.repo-exposure.md
ripr check --root . --mode ready --format agent-seam-packets-json > target/ripr/workflow/agent-seam-packets.json
```

From this repository, the same surfaces can be generated through Cargo:

```bash
cargo run -p ripr -- check --root . --mode ready --format repo-exposure-json > target/ripr/workflow/before.repo-exposure.json
cargo run -p ripr -- check --root . --mode ready --format repo-exposure-md > target/ripr/workflow/before.repo-exposure.md
cargo run -p ripr -- check --root . --mode ready --format agent-seam-packets-json > target/ripr/workflow/agent-seam-packets.json
```

For normal repo-local dogfooding, the wrapper commands write bounded summary
and packet artifacts under `target/ripr/reports/`:

```bash
cargo xtask repo-exposure-summary-report
cargo xtask agent-seam-packets .
cargo xtask operator-cockpit
```

Use `cargo xtask repo-exposure-report` only for explicit full evidence
inspection. It writes full `repo-exposure.{json,md}` artifacts and is not the
ordinary local badge, receipt, or packet-queue route on this repo.

`operator-cockpit` reads the artifacts that already exist under
`target/ripr/reports/`, `target/ripr/pilot/`, and `target/ripr/agent/`, then
writes `operator-cockpit.{json,md}`. It joins repo exposure, the LSP cockpit,
before/after snapshots, agent verify, agent receipt, SARIF policy, badges,
targeted-test receipts, and optional calibration into one next-action view.
Missing inputs remain visible with the command that should generate them.
`operator-cockpit-report` remains an alias for existing repo automation.

Archive the JSON snapshot you want to compare before adding the test. The
targeted outcome report compares two files; it does not rerun analysis.

A small dogfood example lives in
[Targeted test boundary-gap case study](case-studies/TARGETED_TEST_BOUNDARY_GAP.md).
It shows a focused test that changed rendered static evidence while the seam
class stayed `weakly_gripped`.
The checked example corpus index lives in
[`fixtures/EXAMPLE_CORPUS.md`](../fixtures/EXAMPLE_CORPUS.md).

## 2. Pick One Seam

Open `target/ripr/workflow/before.repo-exposure.md` for a targeted before
snapshot, or use `target/ripr/reports/repo-exposure-summary.json` for bounded
repo planning counts.

Pick one headline seam with a class such as:

- `weakly_gripped`
- `ungripped`
- `reachable_unrevealed`
- `activation_unknown`
- `observation_unknown`
- `discrimination_unknown`

Prefer the first seam where the missing discriminator is concrete:

- equality boundary value missing;
- exact error variant assertion missing;
- exact return value assertion missing;
- side-effect observer missing;
- field or object assertion missing.

Avoid starting with `opaque` unless the next task is to inspect a helper,
fixture, macro, or dynamic boundary that hides evidence from static analysis.

## 3. Copy the Work Order

### Editor Path

In VS Code, seam diagnostics use the built-in saved-workspace defaults when no
`ripr.toml` exists. `ripr.toml`, initialization options, or extension settings
can still tune or disable them. Save the file before refreshing; the current
LSP model is saved-workspace analysis.

For the selected seam diagnostic:

- Hover the diagnostic to read the evidence path and classification reason.
- Use `Write targeted test: copy brief` for a human-readable work order.
- Use `Inspect Test Gap - Copy Context` when a coding agent wants structured JSON.
- Use `Write targeted test: copy suggested assertion` when a concrete
  assertion example exists.
- Use `Write targeted test: open best related test` to jump to the strongest
  imitation target.

The repo-local cockpit report verifies that the editor fixture exposes these
actions without opening VS Code:

```bash
cargo xtask lsp-cockpit-report
```

### CLI / Agent Path

Use `target/ripr/workflow/agent-seam-packets.json` or
`target/ripr/reports/agent-seam-packets.json` and match the selected
`seam_id`.

For a `write_targeted_test` packet, pass the whole packet or the targeted test
brief to the agent. It should use:

- `recommended_test` for suggested placement and name;
- `candidate_values` and `missing_discriminators` for the input to exercise;
- `assertion_shape` for the oracle style;
- `nearest_strong_test_to_imitate` and `patterns_to_imitate` for local style;
- `patterns_to_avoid` to avoid broad smoke-only assertions.

The agent must still derive expected values from code or product behavior. RIPR
does not invent expected outcomes.

If you need a resumable attempt with retained evidence, prepare it **before**
editing. For an eligible Rust seam, use the producer's printed start command;
the ordinary form is:

```bash
ripr agent repair --root . --seam-id <seam_id> --phase before
```

Save the exact `--attempt ... --phase after` command it prints. The transaction
captures its own before evidence. A manual snapshot or a CI packet alone does
not prepare that attempt. See [Repair attempt identity](REPAIR_ATTEMPT.md).

## 4. Add One Focused Test

Keep the code change narrow:

- add one test or one small test case;
- imitate the best related test's layout and assertion style;
- exercise the missing discriminator directly;
- assert the affected value, variant, field, object, or side effect;
- avoid broad `is_ok()`, `is_err()`, snapshot-only, or smoke-only assertions
  unless the packet explicitly says that is the strongest visible oracle.

Do not edit production code in this step unless the test cannot be expressed
without a legitimate testability seam change. If production code changes too,
the before/after receipt may report new or removed seam IDs instead of a simple
movement for the original seam.

Run the project's focused test or output check separately and retain its actual
command and result. The static comparison below does not execute that test.

## 5. Capture the After Snapshot

If you prepared an attempt in step 3, run its exact after-phase command after
the edit and project check. It writes the after, verify and receipt artifacts;
inspect the attempt's status and retained result. Do not overwrite its retained
files with the manual commands below.

For a manual snapshot comparison, after saving the new test, rerun the same repo exposure command:

```bash
ripr check --root . --mode ready --format repo-exposure-json > target/ripr/workflow/after.repo-exposure.json
ripr check --root . --mode ready --format repo-exposure-md > target/ripr/workflow/after.repo-exposure.md
```

For a repo checkout:

```bash
cargo run -p ripr -- check --root . --mode ready --format repo-exposure-json > target/ripr/workflow/after.repo-exposure.json
cargo run -p ripr -- check --root . --mode ready --format repo-exposure-md > target/ripr/workflow/after.repo-exposure.md
```

## 6. Generate the Receipt

For a prepared attempt, inspect `ripr agent status --root . --attempt
<repair-attempt-id> --json` and its retained result, whether the after phase
completed or refused. Keep the whole attempt directory. The command blocks
below apply to the separate manual snapshot route.

For that manual route, compare before and after:

```bash
ripr outcome \
  --before target/ripr/workflow/before.repo-exposure.json \
  --after target/ripr/workflow/after.repo-exposure.json
```

Use JSON or file output when needed:

```bash
ripr outcome \
  --before target/ripr/workflow/before.repo-exposure.json \
  --after target/ripr/workflow/after.repo-exposure.json \
  --format json \
  --out target/ripr/workflow/targeted-test-outcome.json
```

### Manual agent verification and receipts

Use this route for an ordinary seam and snapshots deliberately captured around
the test edit. Keep the same repository, RIPR version, mode, configuration and
intended scope. A pilot snapshot may be limited or lack the identity required
by the consumer; do not assume that parseable JSON is a comparable baseline.
If the before snapshot is missing or refused, retain the limitation. A fresh
post-edit snapshot cannot stand in for the original before evidence.

For static verification without a receipt:

```bash
mkdir -p target/ripr/agent &&
ripr agent verify \
  --root . \
  --before target/ripr/workflow/before.repo-exposure.json \
  --after target/ripr/workflow/after.repo-exposure.json \
  --json > target/ripr/agent/agent-verify.json
```

Inspect the command's native exit and JSON before consuming the file. This
compares saved static evidence; it does not execute the project test. Gap-bound
Python or TypeScript subjects have a separate `--gap-id` verification and
closure contract; follow the gap's printed route instead of substituting its
ID into this ordinary-seam recipe.

An optional manual compatibility receipt also reads a producer check document
named **`analysis-outcome.json` in the verify file's directory**. Repo-exposure
JSON is a different document; copying or renaming it does not supply this
input. From the same unchanged post-edit workspace, produce it with:

```bash
mkdir -p target/ripr/agent &&
RIPR_CHECK_FINDINGS_BYTES=0 ripr check --root . --mode ready --format json \
  > target/ripr/agent/analysis-outcome.json
```

This is a Bash example. The environment setting disables findings-document
truncation; it does not remove analysis limitations. Preserve any explicit
`--base REF` and configuration used for the intended comparison. Use the live
input selection that the producer records; an arbitrary saved diff is not a
substitute for the current input the receipt revalidates. Do not change source,
Git state or configuration between this capture and receipt generation.

Before continuing, require a successful producer exit, a complete
`analysis_outcome`, and the intended outcome kind and counts. If
`document_complete` is present it must be `true`; a
`limited_findings_bound` limitation is not a full downstream input.
`no_scope`, `no_changed_lines` and `no_behavioral_candidates` can be complete
outcomes without the behavior you intended to inspect. Read them as reported,
not as a successful repair. Follow any emitted analysis recovery before
retrying; do not edit the JSON to change its status or identity.

The manual unbound receipt below is available only when the default workflow
packet `target/ripr/workflow/agent-packet.json` and default attempt directory
`target/ripr/repair-attempts/` are both absent. When either exists, receipt
issuance enters attempt binding: a missing compatibility packet, no matching
attempt, more than one match, or an attempt without a current eligible after
verdict refuses. Inspect
`ripr agent status` and follow its recovery. For regeneration of an already
eligible attempt, select its exact `--attempt` and retained inputs as described
in the [operator guide](LLM_OPERATOR_GUIDE.md). Do not delete evidence to force
an unbound receipt or create a post-edit before phase to imply an earlier one.

Once those conditions hold, regenerate verify and issue the manual receipt:

```bash
ripr agent verify \
  --root . \
  --before target/ripr/workflow/before.repo-exposure.json \
  --after target/ripr/workflow/after.repo-exposure.json \
  --json > target/ripr/agent/agent-verify.json &&
ripr agent receipt \
  --root . \
  --verify-json target/ripr/agent/agent-verify.json \
  --seam-id <seam_id> \
  --json \
  --out target/ripr/agent/agent-receipt.json
```

Read the receipt itself even when the process exits 0:

| Observation | Meaning and next action |
| --- | --- |
| `status: advisory`, `analysis_outcome_status: complete` | Valid complete static producer outcome. Inspect the selected seam, movement and limitations; retain the inputs with the receipt. This is not a project test result or a durable attempt receipt. |
| `status: incomplete`, `analysis_outcome_status: missing` | The sibling `analysis-outcome.json` is absent. Produce it at the exact path above; a file under `target/ripr/workflow/` does not satisfy a verify file under `target/ripr/agent/`. |
| `status: incomplete`, `analysis_outcome_status: incomplete` | The producer did not complete. Resolve its named limitation and rerun the affected producer, verification and receipt. |
| `status: invalid` | Inspect `analysis_outcome_error`: malformed, wrong-root, incompatible, findings-bounded or stale-input evidence must be replaced by an authentic matching producer result. |
| Nonzero exit before a receipt is written | Resolve the named path, snapshot, seam or attempt-binding failure. File presence or a previous receipt is not evidence that this invocation succeeded. |

Only an advisory receipt with the intended subject can accompany the manual
static evidence in review. Keep the original before/after files and the
producer outcome together; the compatibility output under `target/ripr/agent/`
does not replace an attempt's retained terminal evidence. For durable work,
prefer preparing the attempt before editing, as in step 3.

The generated CI workflow prepares the before side of this loop only: it seeds
`target/ripr/workflow/before.repo-exposure.json` from the pilot snapshot and
writes the workflow, brief, and packet, with compatibility copies of the packet
and brief under `target/ripr/agent/`. There is no test edit between two
snapshots of one CI checkout, so it writes no after snapshot, verify JSON, or
receipt. An uploaded CI packet is not a prepared local attempt: establish the
local source identity and prepare before the local test edit when you need a
durable attempt. Otherwise retain the manual comparison with its limitations.

For one eligible Rust seam, `ripr agent repair --seam-id <seam-id> --phase before` and
`ripr agent repair --attempt <repair-attempt-id> --phase after` run this whole
before/verify/receipt sequence as one transaction; see
[Repair attempt identity](REPAIR_ATTEMPT.md).

Interpretation:

| Bucket | Meaning |
| --- | --- |
| `moved` | Matched seam changed to an equal or stronger static grip class. |
| `unchanged` | Matched seam stayed in the same class. Inspect the evidence delta and the new test's oracle. |
| `regressed` | Matched seam moved to a lower-ranked static grip class. Review the test edit, config, and snapshot inputs. |
| `new` | A seam appears only in the after snapshot, usually because production code or analysis scope changed. |
| `removed` | A seam appears only in the before snapshot, usually because production code changed or seam identity changed. |

The cleanest receipt is one moved seam with evidence deltas such as:

- missing discriminator no longer reported;
- new observed value visible;
- stronger related oracle visible.

An unchanged seam is still useful evidence. It usually means the test missed the
original seam, the assertion remained too broad, the test reached through an
opaque helper, or the before/after snapshots did not use the same root, mode, or
config.

## 7. Align CI, SARIF, Badges, and Calibration

These checks are optional for the local targeted-test loop. They make the same
story visible in the surfaces a reviewer may see.

### SARIF / CI Policy

Generate repo-scoped seam SARIF:

```bash
ripr check --root . --mode ready --format repo-sarif > target/ripr/workflow/after.repo-sarif.json
```

Run the advisory policy report:

```bash
cargo xtask sarif-policy \
  --current target/ripr/workflow/after.repo-sarif.json \
  --mode advisory
```

Use `--mode baseline-check` or `--mode fail-on-new-warning` only when the
repository has deliberately adopted an opt-in baseline policy.

To generate the advisory GitHub Actions workflow, run:

```bash
ripr init --ci github
```

The generated workflow uploads a `ripr-reports` artifact containing the pilot
packet, report artifacts, and repo badge JSON. SARIF rendering/upload is
enabled by the workflow's `RIPR_UPLOAD_SARIF` setting and can be set to
`"false"` without removing the report artifact path. The copyable recipe and
policy details are in [CI strategy](CI.md#copyable-ripr-advisory-workflow).

### Badges

For this repository's checked-in badge endpoints:

```bash
cargo xtask repo-badge-artifacts
cargo xtask badge-basis
cargo xtask check-badge-endpoints
```

Public badge counts are unresolved actionable canonical repair items under the
badge policy. Seam-native inventory remains available in `badge-basis`,
repo-exposure, seam-inventory, and evidence-quality reports as internal
analyzer-health pressure. Public badges are not coverage or runtime mutation
confirmation.

### Runtime Calibration

ripr does not run cargo-mutants. Install it, produce a `mutants.out`
directory, then join that output to the after static snapshot. The importer
was validated against cargo-mutants 27.1.0 `outcomes.json` / `mutants.json`
layouts ([RIPR-SPEC-0006](specs/RIPR-SPEC-0006-mutation-calibration.md)):

```bash
cargo install cargo-mutants --locked
cargo mutants
ripr calibrate cargo-mutants \
  --mutants-json mutants.out \
  --repo-exposure-json target/ripr/workflow/after.repo-exposure.json
```

`cargo mutants` writes `mutants.out/` in the crate root by default. Pass that
directory, `mutants.out/outcomes.json`, or `mutants.out/mutants.json`. A
mutants.json-only path counts unexecuted candidates in `matched_total`;
those rows have no runtime observation. Use cargo-mutants selection flags to
narrow a long run.

The boundary-gap case study has a checked-in sample for trying this without
running mutation tests:

```bash
ripr calibrate cargo-mutants \
  --mutants-json fixtures/boundary_gap/calibration/runtime-mutants.json \
  --repo-exposure-json fixtures/boundary_gap/calibration/after-targeted-test.repo-exposure.json
```

Runtime mutation vocabulary belongs in calibration reports only. A static
movement receipt is a good reason to run less blind mutation work, not a
replacement for runtime confirmation.

The calibration report's agreement section distinguishes static gaps with
runtime gap signals, static gaps without runtime gap signals, runtime signals
without static gaps, static-clean/runtime-clean matches, and inconclusive
runtime labels. Treat those counts as tuning evidence for when to run runtime
mutation next, not as a replacement for the targeted-test receipt.

## Review Checklist

Before calling the targeted-test loop handled, make sure the PR or local change
has:

- a before `repo-exposure-json` snapshot;
- a named seam and `seam_id`;
- the targeted test brief or seam packet used as the work order;
- one focused test or test case;
- an after `repo-exposure-json` snapshot from the same root, mode, and config;
- a `targeted-test-outcome` receipt;
- `agent verify` and `agent receipt` artifacts when handing the result to an
  agent, reviewer, cockpit, or generated CI artifact packet;
- optional SARIF, badge, and calibration artifacts only when they matter for
  the review surface.
