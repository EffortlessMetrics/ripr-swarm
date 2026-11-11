# Agent eval: tokens to the answer and loop closure

`cargo xtask agent-eval` asks two questions an agent using ripr has, on
hermetic answer-keyed scenarios (no network, no model calls, about 40 seconds):

1. **How many tokens until the agent holds the answer?** For each route an
   agent might take, the eval counts estimated tokens read until every keyed
   gap is visible (the owning function, the changed line, and the facts a test
   must pin), and the tokens the whole route costs.
2. **Does the loop work?** Using `ripr check --format agent`, the loop the
   `ripr init --agents` block teaches: are exactly the keyed gaps reported
   (no misses, no extras), is a control change the tests already pin left
   alone, does a weak test leave the gap open, and does the reference test end
   with `verify=complete`? With cargo available each key is checked too: the
   reference test passes at head and fails at base, the weak test passes at
   both.

It sits under the expensive real-model cohort work (#12902, #12912): cheap
enough to run on every output or analyzer change, so a sibling change that
saves tokens or breaks the loop shows up in numbers before an agent pays for
it. Results are static and advisory. A closed loop means ripr's static verdict
moved as expected, not that a test catches a runtime fault.

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
commit `benchmarks/agent_eval/baseline.json` in the same PR, so the diff of
the baseline shows reviewers exactly what moved. The baseline holds no
timings, scratch paths or dates. Two runs of the same binary give byte-identical
reports. Only `ripr_version` changes from build to build.

## Routes

| route | what the agent runs |
|---|---|
| `cli-agent-loop` | `check --quiet --format agent`, then every printed `next=` command in turn (explain, then context), as the `init --agents` block instructs |
| `cli-agent-find` | the same first step only |
| `cli-human` | `check` with the default human output |
| `cli-json-terse` | `check --format json-terse` |
| `agent-brief` | `agent brief --json` |
| `mcp` | `initialize`, `tools/list`, `ripr_workspace_status`, `ripr_refresh`, `ripr_list_gaps`, then `ripr_get_gap` for each listed item that is not `exposed` |

Every route passes `--base main` (or `{"base": "main"}`). Each route runs in a
fresh copy of the scenario repository, so first-run state, such as the one-time
human legend, cannot leak from one route into the next.

## Definitions

- **bytes**: stdout plus stderr of a CLI step, which is what a shell tool shows
  an agent; for MCP, the JSON-RPC response line on the wire.
- **tokens**: an estimate, not a tokenizer or billing count. Each run of
  ASCII letters, digits and `_` costs ceil(len/4), each other non-whitespace
  character costs 1, and whitespace is free. For MCP only the part a host
  shows the model is counted: the tool list, each tool call's `content` text,
  and the `initialize` instructions. Use the numbers to compare routes and
  revisions, not as absolute costs.
- **tokens to answer**: cumulative tokens after the first step whose output,
  together with earlier steps, names every keyed gap's owner, line
  (`lib.rs:N` or a JSON `"line": N`) and all of its facts. A control scenario
  is answered once the route's discovery step (the check, brief or
  `ripr_list_gaps`) has been read. The loop checks decide whether the route
  really stayed quiet.
- **regression** (`--compare`): an answer no longer reached, total or
  to-answer tokens up more than 10% plus 20 tokens, more missed or extra gaps,
  or a loop check that went from yes to NO.

The scratch root path is replaced by `<root>` before anything is measured.

## Scenarios

Scenarios live in `xtask/src/reports/agent_eval/scenarios.rs`: a base and a
head `src/lib.rs`, the tests the repository already had, a weak and a
reference test, and the answer key. Keys are hand-written facts about the
scenario, not ripr output. A unit test checks that every keyed line really
changed and that its owner exists at head.

To add a scenario, append it there and rerun with `--write-baseline`. Good
additions are realistic changes where an agent goes wrong today: ones vanilla
agents miss, ones ripr reports noisily, and controls where the right answer is
silence. Keep each crate free of dependencies so the eval stays offline.

## Baseline (main `2ab9f908`, debug build)

| route | answered | tokens to answer | total tokens |
|---|---:|---:|---:|
| cli-agent-loop | 6/6 | 1,492 | 8,731 |
| cli-agent-find | 6/6 | 1,492 | 1,492 |
| cli-human | 5/6 | 5,405 | 6,515 |
| cli-json-terse | 6/6 | 25,679 | 25,679 |
| agent-brief | 6/6 | 8,704 | 8,704 |
| mcp | 6/6 | 52,117 | 56,773 |

What the baseline shows:

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
