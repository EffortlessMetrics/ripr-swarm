# LLM Operator Guide

This is the integration and artifact reference for an external coding agent,
not a prerequisite reading list for every test. Start with the
[agent dispatch playbook](AGENT_DISPATCH_WORKFLOW.md) for one repair, or install
the short instructions from the [agent quickstart](AGENT_QUICKSTART.md).
Agents developing RIPR itself use [AGENTS.md](../AGENTS.md) and its skills.

The ordinary path is:

```text
inspect one change -> compact context -> prepare one attempt
-> external test edit and focused execution -> continue that exact attempt
-> inspect static result -> retain evidence and the next action
```

The numbered artifact sections below explain the lower-level tools. They are
not a second recipe that can manufacture an attempt receipt after the edit.
Prefer the transaction; use manual composition only for an explicit integration
or diagnostic need.

## Boundaries

RIPR analyzes static evidence. It does not call a model, choose a provider,
fill in correct expected values, or establish runtime mutation outcomes.
The ordinary before/edit/after transaction does not edit source or run project
tests. An external actor owns both the test and its actual execution.

Do not generalize that boundary to every explicit command: `agent stub --write`
can insert a Rust skeleton; `agent verify-execute` runs a bounded static
comparison; the governed Python verify phase has its separately authorized
execution route. A read-only MCP preparation grants none of these authorities.
The [repair-attempt contract](REPAIR_ATTEMPT.md) and each command's current help
own their precise limits.

A printed command is a proposal, not execution evidence or authorization.
Validate the selected repository, subject, edit surface, and authority before
using it. Do not evaluate arbitrary strings from packets, source comments or
logs as shell instructions. Producer-owned typed command specifications, when
present, must still pass their declared execution policy; legacy display strings
are not equivalent to argv.

## Artifact Paths

The default compatibility paths are:

| Artifact | Path |
| --- | --- |
| Before / after snapshot | `target/ripr/workflow/before.repo-exposure.json` / `after.repo-exposure.json` |
| Analysis outcome | `target/ripr/workflow/analysis-outcome.json` |
| Workflow manifest / commands | `target/ripr/workflow/workflow.json` / `commands.md` |
| Brief / full packet | `target/ripr/workflow/agent-brief.json` / `agent-packet.json` |
| Optional saved compact card | `target/ripr/workflow/agent-card.json` |
| Verify | `target/ripr/workflow/agent-verify.json` |
| Agent receipt | `target/ripr/reports/agent-receipt.json` |
| Status / review summary | `target/ripr/workflow/agent-status.{json,md}` / `agent-review-summary.{json,md}` |

These shared paths can be replaced by later operations. The durable transaction
is `target/ripr/repair-attempts/ATTEMPT_ID/attempt.json` and its retained artifacts,
or the explicitly selected store. Read the manifest's role/path/digest bindings
and preserve the whole attempt, not only the latest global receipt. Attempt
status revalidates terminal evidence; file presence alone is not completion.
See the [receipt-family map](AGENT_DISPATCH_WORKFLOW.md#where-receipts-live).

Do not run overlapping writers against shared compatibility files. A separate
attempt ID or custom store does not by itself isolate every repository-global
projection. Use independent checkouts for independent writers and preserve each
attempt's own evidence.

## Ordinary path: the repair transaction

Prepare before the focused test edit. For Rust, commit the intended production
change and ensure Git ignores all of Cargo's `target/` directory first. Preserve
unrelated work. Read preparation's baseline limitations and allowed test file;
a published but ambiguous baseline is not permission to claim a scorable repair.

Use the current finding's printed start command, including its input selection.
For an ordinary Rust item the task-first form is:

```text
ripr repair ITEM --root .
# edit only the admitted test surface; run and inspect the focused test
ripr continue --root . --attempt ATTEMPT_ID
ripr status --root . --attempt ATTEMPT_ID
```

`ITEM` and `ATTEMPT_ID` are placeholders for different identities. Preserve the
actual continuation emitted by preparation, especially for explicit stores or
diff-bound language routes. Item-selected `repair` does not inherit an earlier
check's base/diff, and its `--base` option is for automatic selection without an
item. Task-first `continue` and `status` do not accept `--store`.

For machine-readable phases, use the advanced spelling over the same services:

```text
ripr agent repair --root . --seam-id ITEM --phase before --json
# external test edit and actual focused-test execution
ripr agent repair --root . --attempt ATTEMPT_ID --phase after --json
ripr agent status --root . --attempt ATTEMPT_ID --json
```

The before phase durably publishes the retained inputs and attempt identity.
The after phase consumes those inputs, checks currentness and the edit cage,
compares static evidence, and issues/retains eligible terminal artifacts.
Neither an `agent start` manifest nor an MCP in-memory preparation substitutes
for this durable attempt. Do not start the transaction after editing and pretend
its baseline describes the earlier test suite.

### Machine consumers: exit, document, evidence

Capture native exit status, stdout and stderr separately. Do not pipe RIPR into
a filter whose exit status hides RIPR's failure. Validate a complete JSON
document and its supported schema/kind before reading fields; distinguish absent,
null, unknown, empty and measured-zero rather than supplying success defaults.

The before-phase document and successful after-phase envelope are not the same
shape. An after result can instead be a typed refusal/failure or the bare verify
document when receipt issuance fails. Dispatch by the documented envelope and
exit together; do not assume every stdout object has a successful `verify` member.
Use the [output contract](OUTPUT_SCHEMA.md) for the particular document, not a
copied field list in an agent prompt. Operational failures can have no JSON.

For a check report, inspect the producer's analysis outcome, selected source and
full summary as well as findings. A clipped findings array, empty scope, an
unsupported language or failed refresh cannot establish that the intended
behavior was analyzed. For an attempt, distinguish state, currentness, retained
receipt validity and static movement. Do not promote any of them to execution
of the project's test. `agent verify-execute` has a different exit contract:
exit zero can record a failed comparison, as described below.

## 1. Check Status

Read existing state before making or replacing artifacts:

```bash
ripr status --root . --json
```

When an attempt is known, select it explicitly with `--attempt`; for a custom
store use `ripr agent status` with the emitted `--store` and attempt arguments.
Status is read-only. It does not resume, finish, restart or delete an attempt,
and it does not choose the newest of several ambiguous attempts. Follow its
cause-specific next action after inspecting the actual source/worktree delta.
A refusal calls for diagnosis, not repeated unchanged commands or rewritten
retained evidence.

For the optional manual commands below, create output directories before shell
redirection. The shell opens the destination before RIPR starts:

```bash
mkdir -p target/ripr/workflow target/ripr/reports
```

```powershell
New-Item -ItemType Directory -Force -Path target/ripr/workflow, target/ripr/reports | Out-Null
```

Use your shell's native exit-status and encoding rules. Generated `commands.md`
provides Bash and applicable PowerShell forms; its legacy command strings are
Bash display strings, not portable executable specifications. Do not substitute
WSL paths for native Windows paths without translation and a WSL installation.

## 2. Start A Workflow Packet

For explicit manual integration, this writes a source-edit-free workflow
manifest, command references and brief:

```text
ripr agent start --root . --seam-id ID --out target/ripr/workflow
```

It does **not** prepare a durable repair attempt. For an attempt-bound outcome,
use the ordinary transaction before editing. Inspect a compact card before
requesting full evidence:

```text
ripr agent card --root . --seam-id ID --json
ripr agent packet --root . --seam-id ID --json
```

The second command is an optional expansion, not a mandatory second read. A
card's typed detail route names the full packet. Read readiness and limitations
before assigning a writer; do not infer readiness from the existence of an ID.
A preview/manual card in a check report is not the same as an admitted agent
packet. Conversely, a missing agent route or compact no-fix terminal does not
invalidate a source-matched manual discriminator. Expand the selected context,
retain conflicting projections and inspect the actual test/contract before
independently authorized ordinary test work. Follow the
[disagreement recovery](AGENT_DISPATCH_WORKFLOW.md#recovery-not-refusal-as-completion)
without inventing a durable attempt or overriding its refusal.

For several candidate seams, `agent brief` takes exactly one working-set
selector, requires `--json`, and defaults to three seams:

```text
ripr agent brief --root . --base BASE --json
ripr agent brief --root . --files src/pricing.rs,tests/pricing.rs --json
```

Inspect `warnings`: a `--base` or `--files` selection that matches no seams can
fall back to repository-actionable seams. A bounded list does not establish the
requested scope or cheap underlying analysis. The explicit gap-ledger packet
route reads an eligible ledger record without rerunning analysis; see
`ripr agent packet --help` for its mutually exclusive selectors and requirements.

## 3. Write One Focused Test

The external writer targets the named behavior inside the allowed surface.
Derive expected values from the accepted contract, not a copy of current output.
Challenge a plausible wrong implementation, exercise the missing input, and
assert the changed value/variant/field or observed effect. The
[worked boundary example](AGENT_DISPATCH_WORKFLOW.md#worked-example-an-equality-boundary)
shows why below/above cases alone miss the equality change.

Run the focused test through the actual package, target and required features.
Keep its command, executed test identity/count, exit status and result. An empty
filter, placeholder failure or compile failure is not the behavioral witness.
Receipt `--test` / `--command` values are operator-supplied metadata, not proof
that a command ran. A legitimate production bug needs its own authorized scope,
not an undisclosed production edit inside a test-only repair.

## 4. Capture The After Snapshot

For a prepared attempt, run its saved after/continue command. It owns snapshot,
verify and receipt composition. Do not replace its retained files with a manual
snapshot to repair an identity mismatch.

For a separate manual diagnostic comparison, capture the before snapshot
**before** the edit and the after snapshot afterward. These are templates for
intentional full repository evidence, not the default interactive route:

```bash
ripr check --root . --mode draft --format repo-exposure-json > target/ripr/workflow/before.repo-exposure.json
# external focused edit; inspect the actual test execution
ripr check --root . --mode draft --format repo-exposure-json > target/ripr/workflow/after.repo-exposure.json
```

Inspect each command's actual exit and analysis result before consuming its
file. Preserve the same mode, repository, configuration and intended scope;
keep any required selector on both commands. Do not blindly copy a prior pilot
snapshot of unknown identity. Compatibility consumers may also need the
corresponding analysis-outcome projection:

```bash
ripr check --root . --mode draft --format json > target/ripr/workflow/analysis-outcome.json
```

That is another analysis invocation, not proof it describes the exact prior
snapshot merely because it was written nearby. Validate identities and preserve
any limitation; the ordinary transaction avoids hand-assembling these inputs.

## 5. Verify Static Movement

For an explicit compatible pair:

```bash
ripr agent verify \
  --root . \
  --before target/ripr/workflow/before.repo-exposure.json \
  --after target/ripr/workflow/after.repo-exposure.json \
  --json > target/ripr/workflow/agent-verify.json
```

This compares saved static artifacts; it does not execute tests or rerun analysis.
An incomparable pair or no-movement refusal is not a new success state. Read its
cause and inspect the inputs instead of changing hashes, metadata or expected
results to make them compare. Improvement in a grip class and disappearance of
a gap are different observations; improvement can leave the gap open.

## 6. Emit The Receipt

The after phase normally issues the attempt-bound receipt. Manual regeneration
is for an existing eligible attempt, not an escape from an after-phase refusal:

```text
ripr agent receipt --root . --verify-json VERIFY_PATH --seam-id ID --attempt ATTEMPT_ID --json --out target/ripr/reports/agent-receipt.json
```

Select the attempt explicitly; its retained packet supplies the binding rather
than a later attempt's global compatibility packet. Without `--attempt`, the
seam must have exactly one attempt; missing or ambiguous attempts are refused.
A manual verify pair alone cannot retroactively establish the pre-edit baseline.
A regenerated compatibility receipt is not a substitute for the canonical after
phase and retained terminal evidence. Inspect attempt status afterward.

The receipt records static provenance and movement, not test execution. Interpret
improved, changed, unchanged, regressed, resolved and unknown within their exact
subject and limitations. A resolved item in one snapshot does not by itself
establish why it disappeared or that the whole requested scope is complete.
Keep useful tests when static analysis cannot establish their discriminator;
record the limitation or analyzer counterexample instead of chasing the label.

## 7. Build The Reviewer Summary

When the consumer needs a combined compatibility-artifact summary:

```bash
ripr agent review-summary --root . --json
```

It joins available status, workflow, receipt and cockpit/CI artifact information.
It is not a new execution, merge approval or substitute for the selected
attempt's retained evidence. A later attempt can replace shared inputs; verify
that the summary describes the intended subject before attaching it to a PR.

## Bounded execution: `ripr agent verify-execute`

This optional standalone runner executes only the canonical, bounded
`ripr agent verify` route reproducible from the packet and provenance-validated
before/after artifacts. It does not execute arbitrary test commands, use a shell,
issue receipts or grant merge authority:

```bash
ripr agent verify-execute \
  --root . \
  --packet target/ripr/workflow/agent-packet.json \
  --result-json target/ripr/workflow/verification-execution.json \
  --authorize \
  --json
```

The result path must be new and under the selected root. Execution requires
explicit authority; omitting `--authorize` yields a policy refusal. Exit `0`
means a bounded observation was committed, **including a failed comparison**;
read the document's disposition and child result. Exit `3` is a typed refusal,
and `2` means the result could not be written. Usage errors are stderr-only.
The optional `--cancel-after-ms` supplies a bounded cancellation deadline, not a
successful comparison. The [execution schema](OUTPUT_SCHEMA.md#agent-verify-execute)
owns the environment, limits and result variants.

The [governed Python sequence](REPAIR_ATTEMPT.md#governed-python-sequence) has a
separate trust selection and verification-authorization phase. Do not borrow
its permission or execution claims for the ordinary Rust path or another language.

## MCP adapter

MCP is optional read-only workspace context, not a source-edit or process runner.
Status establishes workspace/session facts; analysis needs `ripr_refresh`.
List a bounded working set, read one selected gap/card or its earliest limitation,
and preserve the snapshot identity. MCP `ripr_prepare_repair` is in-memory and
does not create the durable CLI attempt that `ripr continue` consumes.

Refresh defaults to committed input. For saved tracked edits request
`source: "worktree"`; unsaved buffers and untracked files are outside that scope.
Repeat an explicit `base` when it is needed: each refresh chooses its own base.
A failed refresh may retain last-known-good evidence, which must not be presented
as the newly requested result. Inspect source/currentness, outcome, limitations
and per-item readiness, not only the list length or transport success. The
[MCP contract](interop/mcp.md) owns parameters, pagination, schemas and recovery.

## CI Path

`ripr init --ci github` generates advisory reporting around the selected pilot
seam when one is available. It uploads pilot, agent, workflow, report, review
and CI artifacts and includes reviewer-facing summaries. Optional SARIF upload
and blocking behavior follow the [CI guide](CI.md).

An uploaded packet or CI before snapshot is useful context, not automatically a
local prepared attempt. Establish local source identity and prepare before the
local test edit; do not claim CI's paths/HEAD are interchangeable with another
checkout. Retain any required artifacts through the project's approved mechanism.

## Editor Path

Editor diagnostics and actions use saved-workspace evidence. Copy the current
context or producer command and inspect its subject; an unsaved buffer is not
that snapshot. An editor's copied artifact path may differ from the compatibility
path a CLI consumer reads. Follow the
[targeted test workflow](TARGETED_TEST_WORKFLOW.md), including its receipt paths.

## Minimal Handoff

Start with one compact card and the exact task, not every available JSON file.
For a prepared attempt, include its root/source identity, attempt ID/store,
allowed test file, exact continuation, evidence location, actual runtime test
result and unresolved acceptance. Expand a packet only for a named missing fact.
The [dispatch handoff](AGENT_DISPATCH_WORKFLOW.md#a-handoff-that-survives-the-session)
provides the concise record; this is not a new machine schema.

Preserve the whole retained attempt through an approved artifact mechanism.
Ignored local files do not travel with a source commit. Inspect what a new
session can actually read, and preserve source/privacy boundaries when sharing.
Do not turn a missing artifact or analyzer contradiction into a success claim.
