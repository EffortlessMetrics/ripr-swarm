# Agent eval: tokens to the answer and loop closure

`cargo xtask agent-eval` asks two questions an agent using ripr has, on
hermetic answer-keyed scenarios (no network, no model calls, about 40 seconds):

1. **How many tokens until the agent holds the answer?** Per route, estimated
   tokens read until every keyed gap is visible (owner, changed line, and the
   facts a test must pin), and the route's total.
2. **Does the loop work?** On `ripr check --format agent`, the loop
   `ripr init --agents` teaches: exactly the keyed gaps reported, a pinned
   control left alone, a weak test leaving the gap open, and the reference
   test ending with `verify=complete`. With cargo, each key is checked too:
   reference passes at head and fails at base, weak passes at both.

It sits under the real-model cohort work (#12902, #12912), cheap enough to run
on every output or analyzer change. Results are static and advisory: a closed
loop means ripr's static verdict moved as expected, not that a test catches a
runtime fault.

## Run it

```bash
cargo build -p ripr --bin ripr
cargo xtask agent-eval --ripr target/debug/ripr --compare benchmarks/agent_eval/baseline.json
```

The report is written to `target/ripr/reports/agent-eval.{md,json}`. The
command exits nonzero when `--compare` finds a regression. Options:

- `--scenario <id>` (repeatable) runs a subset. It cannot be combined with
  `--write-baseline`.
- `--skip-cargo` skips the cargo key checks (`reference_key_valid`,
  `weak_key_valid` become `-`).
- Without `--ripr`, the command builds and uses the debug binary.

When your change is meant to move a number, rerun with `--write-baseline` and
commit `baseline.json` in the same PR, so its diff shows what moved. It holds
no timings, paths or dates; only `ripr_version` changes between builds.

## Routes

| route | what the agent runs |
|---|---|
| `cli-agent-loop` | `check --quiet --format agent`, then every printed `next=` command in turn (explain, then context), as the `init --agents` block instructs |
| `cli-agent-find` | the same first step only |
| `cli-human` | `check` with the default human output |
| `cli-json-terse` | `check --format json-terse` |
| `agent-brief` | `agent brief --json` |
| `mcp` | `initialize`, `tools/list`, `ripr_workspace_status`, `ripr_refresh`, `ripr_list_gaps`, then `ripr_get_gap` for each listed item that is not `exposed` |

Every route passes `--base main` (or `{"base": "main"}`) and runs in a fresh
copy of the scenario repository, so first-run state cannot leak between routes.

## Definitions

- **bytes**: a CLI step's stdout plus stderr; for MCP, the response line.
- **tokens**: an estimate, not a billing count. Each run of ASCII letters,
  digits and `_` costs ceil(len/4), other non-whitespace characters cost 1.
  For MCP only what a host shows the model counts: the tool list, each call's
  `content` text, and the `initialize` instructions. Compare routes and
  revisions with it, not absolute costs.
- **tokens to answer**: cumulative tokens after the first step whose output,
  together with earlier steps, names every keyed gap's owner, line
  (`lib.rs:N` or a JSON `"line": N`) and all of its facts. A control scenario
  is answered once the route's discovery step (the check, brief or
  `ripr_list_gaps`) has been read. The loop checks decide whether the route
  really stayed quiet.
- **missed / extra gaps**: keyed lines with no gap row, and gap rows beyond
  one per keyed line. Two rows for the same changed line count as one extra.
- **regression** (`--compare`): a lost answer; total or to-answer tokens up
  more than 10% plus 20; more failed steps (nonzero exit, JSON-RPC error, MCP
  `isError`); more missed or extra gaps; a loop check that was yes and is now
  NO or missing (cargo-only checks under `--skip-cargo` are unmeasured); or, on
  a full run, a baseline scenario or route that no longer runs. A check already
  NO in the baseline is unguarded; a fix shows as a note and is rebaselined.
- A cargo key check that cannot build or report a test result is an
  instrument failure: shown as `not established`, never NO. It fails every
  run mode and blocks `--write-baseline`.
- A silent control counts only when `check --format agent` exited 0, its
  header says `gaps=0` and it printed no `stop=`. The weak and reference
  checks also need exit 0. `--write-baseline` refuses `--compare`.
- Every scenario subprocess clears inherited `RIPR_*` and `GIT_*` variables,
  including repository, worktree, index, object-store and configuration routing.
  Global and system Git configuration are disabled for these children. This
  boundary also covers analyzer, MCP and Cargo descendants; changing cwd alone
  does not isolate Git under `git bisect run`. Explicit scenario commit identity
  is retained. The eval closes stdin, runs scenario Git without hooks, and
  replaces the scratch root with `<root>` before measuring.

## Scenarios

Scenarios live in `xtask/src/reports/agent_eval/scenarios.rs`: a base and a
head `src/lib.rs`, the tests the repository already had, a weak and a
reference test, and the answer key. Keys are hand-written facts about the
scenario, not ripr output. A unit test checks that every keyed line really
changed and that its owner exists at head.

To add a scenario, append it there and rerun with `--write-baseline`. Good
additions are changes agents get wrong today, noisy reports, and silent
controls. Keep each crate dependency-free so the eval stays offline.

## Baseline (debug build)

Route totals are the `routes` rows at the top of `baseline.json`. What they show:

- The first agent-format step already carries every answer. The DETAIL steps
  the loop then prescribes (explain, then context) cost 4.9 times as much and
  add no keyed fact.
- `cli-human` shows one finding and hides the rest, so on the two-gap scenario
  the second gap stays hidden.
- MCP costs about 35 times the agent CLI to reach the same answer. The tool
  list alone is about 4,300 estimated tokens per session.
- Loop: after the reference test, two of the five gap scenarios
  (`changed_constant_return`, `struct_field`) still end with
  `verify=refused`, so an agent following the block cannot finish.
- `error_variant` gets two identical gap rows for its one changed line
  (`extra_gaps: 1`), so the header asks for two tests where one is needed.
