# Agent Dispatch Workflow

Use this playbook to **use RIPR in another repository**: inspect one changed
behavior, add one useful discriminator, and hand back evidence another agent
can resume. The commands use an installed `ripr`; they do not require RIPR's
source checkout or its private `cargo xtask` commands.

For the short installable instruction block, use the
[agent quickstart](AGENT_QUICKSTART.md). For agents **developing RIPR itself**,
start at [root AGENTS.md](../AGENTS.md) and the
[repository skill routes](AGENT_WORKFLOWS.md). Those contributor procedures are
not prerequisites for adding a test in a consumer's project.

RIPR supplies static evidence and a bounded repair handoff, not runtime mutation
results or permission to change the project. The ordinary repair transaction
does not write the test or execute it. Explicit tools have narrower exceptions:
`agent stub --write` can insert a Rust skeleton, and separately authorized
verification routes can execute their retained command. Neither fills in the
correct expected values or establishes that a real bug would be detected.

## The loop

```text
select the real changed behavior
-> inspect one finding / compact card
-> prepare an attempt BEFORE editing its test
-> write a contract-derived discriminator inside its edit surface
-> run the focused test; inspect the actual executed test and result
-> continue that exact attempt; inspect static movement and limitations
-> retain the attempt evidence and the next useful action
```

Load more context only to answer a specific unresolved question. Start with
human output or `explain --format agent`, then a card or one context packet.
Do not paste full repo-exposure JSON, every seam packet, or the entire output
schema into each agent session. Bounded output is not necessarily bounded
analysis cost; reuse current evidence rather than repeat repository scans.

## First-hour repair path: the repair transaction

### 1. Select the input, not just a command

Run from the intended repository root:

<!-- agent-doc-command: check -->
```bash
ripr check --root . --quiet
```

Read the selected input and its limitations. Keep an explicit base or diff
selector when following the commands RIPR prints; do not silently switch from
saved edits to committed input or from the requested diff to a repository scan.
Save edited buffers first. New source files need intent-to-add (`git add -N PATH`)
when they are meant to participate in the diff; this changes the index, so apply
it only to the intended files.

Exit `0` is advisory success, not "all changed behavior has a discriminator."
An empty list, a truncated display, or "no gap selected" does not establish
complete analysis. Confirm the selected changed behavior was actually analyzed;
retain no-scope, partial, unsupported and failed outcomes as such. See
[exit codes](EXIT_CODES.md) and the [static limits](STATIC_LIMITS.md).

### 2. Inspect only the selected gap

Read the finding's file, missing observation, related test and next step. Run
its printed `explain` or `context` command with the same input selector. Prefer
`explain --format agent` for a small explanation; `context` is the JSON detail
route, not `explain --format json`.

For a compact handoff, use the emitted seam or supported finding ID:

<!-- agent-doc-command: card -->
```text
ripr agent card --root . --seam-id ID
```

Read the card's readiness, exact blocker, selected target and detail route.
A card is not an active repair attempt. A finding ID or seam ID selects behavior;
a repair-attempt ID identifies the prepared transaction. Never substitute one
for the other or reuse an ID against different analysis inputs by guesswork.

### 3. Prepare before the test edit

For the ordinary Rust path, commit the intended production change first and
ensure Git ignores the entire Cargo `target/` directory before preparation.
Preserve unrelated work; do not automatically stash, reset or commit someone
else's changes. Arrange one writer for the attempt's edit surface.

Start the repair using the command the current finding/card supplies. The
ordinary form is:

<!-- agent-doc-command: start -->
```text
ripr repair ITEM --root .
```

This spelling does not inherit a previous check's diff/base. For a Python or
TypeScript diff gap, keep the gap-bound start printed by `ripr check` when
available. Do not append `--base` to an item-selected start: that option scopes
only an automatic start without an item.

A successful preparation prints the allowed test file, attempt identity and
exact continuation command. Read any baseline ambiguity or readiness limitation
before editing. Keep the printed command, including its root, explicit store
and attempt selection. The advanced `ripr agent repair --phase before` spelling
uses the same transaction; follow it when that is what RIPR prints.

Do not add the test and only then prepare a "before" snapshot. That records the
new test as baseline rather than evidence of the edit. Preparation failure does
not authorize inventing an attempt, broadening the cage or rewriting artifacts.

### 4. Write the discriminator, not a duplicate test

Derive the expected result from the accepted behavior, specification or an
independent example. Read production code to locate the owner and exercise it,
not to copy its current result into the expected value. Ask: **which plausible
wrong implementation would this assertion reject?**

Stay inside the admitted test-only surface. Test an equality boundary with
below/equal/above cases when relevant; assert the exact changed field, error
variant, return value or observable side effect. Match the project's useful
test style without copying a smoke assertion that would accept the wrong value.
A legitimate product defect needs a separately scoped production repair, not
an undisclosed production edit inside this test-only attempt.

An optional `ripr agent stub` is a scaffold, not a completed test. Preview it
before `--write`, check its placement against the attempt's allowed file, and
fill every expected-value placeholder. A placeholder/setup/compile failure is
not evidence that the intended behavioral assertion distinguishes a bug. Do not
write a stub merely because it is available when an existing test already
observes the behavior.

### 5. Verify two different things

First run the focused test through the owning package/target and inspect the
terminal result. For a simple Cargo project, `cargo test TEST_NAME` may suffice;
prefer the project or packet's exact command when features, integration targets
or workspace membership matter. A successful filter matching zero tests is not
a pass for the intended test. Record actual execution; a command printed in a
packet or supplied as receipt metadata does not mean it ran.

Then run the saved continuation command. For a default-store attempt its
ordinary shape is:

<!-- agent-doc-command: continue -->
```text
ripr continue --root . --attempt ATTEMPT_ID
```

Inspect that attempt's verdict, static movement, limitations and receipt. A
stronger static classification does not imply the test ran; a passing test does
not imply the static gap closed. Recheck the original changed-behavior scope as
needed without discarding its input selector. Do not turn disappearance from a
filtered display, a changed base, suppression or missing evidence into closure.

Complete only the selected claim supported by these observations. Other gaps,
project-wide test quality, merge readiness and runtime mutation results remain
separate. A useful test with an honest analyzer limitation can be retained;
report that result rather than claiming a completed static repair.

## Recovery, not refusal-as-completion

A refusal protects an evidence boundary; it is not the fix for the underlying
problem. Read the cause and choose a changed, authorized next action. Do not
repeat identical commands, weaken a valid assertion, alter correct production
behavior or hide a finding to manufacture a clean result.

| Observation | Next useful action | Do not claim |
| --- | --- | --- |
| No scope or no changed behavior analyzed | Verify repository, saved files and intended base/diff. Select the actual change and rerun. | Empty output establishes test quality. |
| Finding/card is stale or its ID no longer resolves | Refresh the intended analysis and select its current item; retain the old observation with its original identity. | A different run's ID is interchangeable. |
| Repair route is unavailable or ambiguous | Inspect the named blocker and related test. Correct a reversible setup/input problem, or make an independently authorized focused test without claiming an attempt receipt. | An unavailable guided route makes all useful work impossible. |
| Analyzer limit; an existing exact assertion appears to observe the behavior | Inspect the named test and hidden helper/macro/dispatch path. Preserve the valid test and report a minimal analyzer counterexample with source identity. | Adding duplicate tests or making the finding disappear repairs the analyzer. |
| After phase says no movement | Check whether the test was edited after preparation, saved, and included in the selected input. Read the attempt's refusal history before retrying. | Rerunning unchanged inputs creates new evidence. |
| HEAD, configuration, retained inputs or edit cage no longer match | Inspect attempt-specific status and the actual delta. Preserve work and evidence; follow the documented recovery or prepare a new attempt when necessary. | Rewriting retained snapshots, resetting shared work or suppressing the refusal restores comparability. |
| Test passes but static guidance still contradicts the exact assertion | Retain the runtime result and the contradiction; investigate the semantic owner or file a bounded analyzer finding. | A stricter refusal alone fixes a wrong analyzer recommendation. |
| Command fails or the test did not execute | Diagnose the concrete setup/test/instrument failure and rerun only after a relevant correction. | A static receipt substitutes for missing execution. |

Use status to recover an interrupted attempt, not chat memory or the newest
file timestamp:

<!-- agent-doc-command: status -->
```text
ripr status --root . --attempt ATTEMPT_ID
```

With multiple attempts, name the intended one explicitly. For a custom store,
use the emitted advanced `agent status` / `agent repair` commands with `--store`;
the task-first `status` and `continue` forms above do not accept that option.
Stop a repetitive test-writing loop when it yields no new evidence; keep the quickstart's bounded revision
rule distinct from the tool's own refusal budget. Status and
[repair-attempt recovery](REPAIR_ATTEMPT.md) own the exact state-specific next
command. Report a genuine unavailable capability or out-of-scope decision with
its evidence, while continuing any independent authorized work.

## Where receipts live

These are different families, not interchangeable proof:

| Evidence | Location / owner | How to use it |
| --- | --- | --- |
| Attempt identity and retained result | `target/ripr/repair-attempts/ATTEMPT_ID/attempt.json` and its `artifacts/`; an explicit store changes the root | Retain the whole attempt directory, including its before evidence and bound `agent-receipt.json` / `agent-verify.json` terminal artifacts. Attempt-specific status revalidates the evidence. |
| Agent-loop compatibility projection | `target/ripr/reports/agent-receipt.json` and `target/ripr/workflow/` | Shared workflow/review consumers use these paths; later attempts can replace them. They are not the sole durable result of a finished attempt. |
| Gap receipt | `ripr receipt write` defaults to `target/ripr/receipts/` | Consumed by `ripr receipt check`; not an agent-loop receipt or a replacement for attempt evidence. |
| Editor artifact copies | Commands may write under `target/ripr/agent/` | Follow the consumer's exact path; do not assume a copy satisfies a workflow or attempt's binding. |

Prefer the repair transaction to assembling these files by hand. For explicit
receipt regeneration, current `ripr agent receipt --help` documents `--attempt`
selection and retained-packet binding; a bare file's presence does not establish
a completed loop. The [repair-attempt contract](REPAIR_ATTEMPT.md) owns identity,
digests, retention and currentness. The
[operator reference](LLM_OPERATOR_GUIDE.md) owns manual artifact plumbing.

## A handoff that survives the session

Put a short evidence record in the existing issue or PR; do not introduce a
second orchestration database. This is a handoff checklist, not a new JSON schema:

```text
Claim: changed behavior and the wrong result the test must reject
Subject: repository/root, RIPR version, HEAD, base/diff and saved/committed source
Selection: finding/seam ID; attempt ID and store, or no attempt established
Edit: allowed test file; actual test change; unrelated work preserved
Runtime: exact focused command, executed test/count, exit/result, retained log
Static: selected input, before/after observation, limitations, receipt/status
Evidence: attempt manifest and its retained artifacts; PR/commit or artifact link
Next: remaining acceptance, concrete action and owner, or scoped completion
Non-claim: static evidence is not runtime mutation confirmation
```

Ignored `target/` files do not travel with an ordinary source commit. Retain the
needed artifacts through the project's approved evidence mechanism and make the
handoff resolve to them; do not claim remote availability from a local path.
Preserve source/privacy boundaries when sharing a packet or report.

## Examples by seam kind

These illustrate test intent, not claimed results from a live run:

| Changed behavior | Useful discriminator | Weak substitute |
| --- | --- | --- |
| Predicate threshold | Exact results below, at and above the boundary | Only a distant happy-path input |
| Error variant | Assert the contract's exact variant and relevant payload | Only `is_err()` |
| Return value | Assert the required returned value from independent inputs | Call succeeds without an observation |
| Field construction | Assert each changed field against the contract | Only object existence or an unrelated field |
| Side effect | Observe the intended event, state or persistence effect | Only absence of panic |
| Opaque evidence | Inspect the named path and existing assertion first | Add a test solely to chase a static label |

Declared intent and reasoned suppression are governance decisions, not evidence
improvement. Follow the project's policy; do not add them merely to clear a gap.

### Worked example: an equality boundary

The accepted contract is: **shipping costs 10 below a total of 100, and 0 at
or above 100**. Suppose the production change corrects `>` to `>=`. Tests at
99 and 101 pass with either implementation; the missing discriminator is 100.

This self-contained example shows the intended implementation and test together
for explanation. In a repair attempt, production is already changed: add only
the test inside the admitted surface, not a replacement implementation.

<!-- agent-doc-example: threshold-oracle -->
```rust
fn shipping_fee(total: u32) -> u32 {
    if total >= 100 { 0 } else { 10 }
}

#[test]
fn threshold_contract() {
    assert_eq!(shipping_fee(99), 10);
    assert_eq!(shipping_fee(100), 0);
    assert_eq!(shipping_fee(101), 0);
}
```

The equality assertion rejects the plausible wrong `>` implementation. Removing
only that assertion makes the two remaining cases accept the wrong behavior.
In a disposable test copy, this pair distinguishes the intended behavioral
failure from a setup or compile failure; do not mutate production inside the
repair cage to run the experiment. A test filter matching no tests supplies
neither result, even when its process exits successfully.

The repository's `agent_documentation_contract` test target extracts this Rust
block, compiles the valid and wrong copies, checks their actual test results,
and challenges the missing-assertion and zero-test controls. Those checks concern
this example's oracle, not a live RIPR classification, a completed repair
transaction, or an agent's obedience to this guide. Consult the current test
receipt before claiming they passed.

## Advanced routes, only when needed

For several candidate seams, `ripr agent brief --help` describes bounded
working-set selection. Inspect `warnings`: an empty `--files` or `--base` match
can fall back to repository-actionable seams. Do not pass that wider scope to a
writer as though it were the original diff. A full packet is the explicit detail
route behind the compact card, not mandatory initial context.

For manual before/after artifact assembly, see the
[LLM operator guide](LLM_OPERATOR_GUIDE.md). For editor actions and runtime
calibration, see the [targeted test workflow](TARGETED_TEST_WORKFLOW.md). Those
are optional paths; they are not extra prerequisites for one CLI-led repair.

The [governed Python sequence](REPAIR_ATTEMPT.md#governed-python-sequence) has a
separate trust selection and explicitly authorized verification phase. Do not
infer another language's repair or execution support from Rust examples; follow
the current readiness and language limitations. MCP is optional read-only
workspace context, not test-writing or process-execution authority; see the
[MCP contract](interop/mcp.md).
