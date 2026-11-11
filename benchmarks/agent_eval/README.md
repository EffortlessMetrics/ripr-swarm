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

## Agent-in-the-loop trials

The routes above replay ripr's output without a model. To see whether a real
agent writes better tests, or spends fewer tokens, with ripr than without:

```bash
cargo xtask agent-eval trial-setup <fresh-dir>   # one repo per gap scenario
# give an agent each repo, with or without ripr; it adds tests under tests/
cargo xtask agent-eval trial-score <fresh-dir>   # judges what it left behind
```

`trial-score` does not ask ripr. For every keyed gap it puts that one changed
line back to its base text and runs `cargo test`. The gap counts as caught
only when the suite passes on the changed code, fails with the line reverted,
and `src/lib.rs` was left as committed. A run that does not compile or
report a test result (for example a revert that breaks the build, or a
timeout) is recorded as not established and never counts as a catch. A
missing repository or scoring error stays in the denominator as zero caught
and makes the command exit nonzero. `src/lib.rs` is restored to what the
agent left on every path. Controls: a fresh setup scores 0/6, and the
reference tests score 6/6.

Limits: agents must put tests under `tests/` (a `#[cfg(test)]` module in
`src/lib.rs` counts as a production edit). The scorer does not check
`Cargo.toml`, `build.rs` or edits to `tests/existing.rs`, and a test that
asserts the source text itself (`include_str!`) would count as a catch, so
read the agent's tests before trusting a surprising score. Record each
arm's tokens and tool calls next to its score. Use a fresh directory per arm
and per run.

### First pilot (2026-11-11, one run per cell, Haiku 5.5, debug ripr at main `2ab9f908`)

Same task text in both arms; the ripr arm also got the `init --agents` block
and the binary path.

| arm | keyed gaps caught | tokens (5 runs) | tool calls | wall time |
|---|---:|---:|---:|---:|
| without ripr | 6/6 | 260,146 | 47 | 102 s |
| with ripr | 6/6 | 296,330 | 98 | 235 s |

Token totals are the subagent totals the host reported, including the fixed
system prompt each run pays (about 50k), so the difference (about +36k, +14%)
is the part ripr added. On these easy tasks both arms hit the ceiling: ripr
added no caught gaps but doubled tool calls and wall time. Two ripr runs
(`changed_constant_return`, `struct_field`) wrote the right test and then
stopped on `verify=refused` (#14325, #14326), which is the loop failure the
routes table measures. One run per cell supports no rate claim. It does show
where the cost goes and which scenarios need to be harder before a quality
difference can appear.

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
- **missed / extra gaps**: keyed lines with no gap row, and gap rows beyond
  one per keyed line. Two rows for the same changed line count as one extra.
- **regression** (`--compare`): an answer no longer reached, total or
  to-answer tokens up more than 10% plus 20 tokens, more failed steps (a
  nonzero exit, a JSON-RPC error or an MCP `isError` result), more missed or
  extra gaps, a loop check that was yes and is now NO or missing (except the
  cargo-only key checks under `--skip-cargo`, which are unmeasured), or (on a
  full run) a baseline scenario or route that no longer runs. A check that is
  already NO in the baseline is not guarded; fixing it shows as a note, and
  the fixing PR rebaselines so it stays fixed.
- A silent control counts only when `check --format agent` exited 0 and its
  header says `gaps=0`; a failed or empty discovery step answers nothing.
- The eval clears the caller's `RIPR_*` environment, closes stdin, and runs
  git without hooks, so local settings do not change what is measured.

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
- `error_variant` gets two identical gap rows for its one changed line
  (`extra_gaps: 1`), so the header asks for two tests where one is needed.
