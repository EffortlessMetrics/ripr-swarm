# Agent eval: tokens to the answer and loop closure

`cargo xtask agent-eval` asks two questions an agent using ripr has, on
hermetic answer-keyed scenarios (no network, no model calls, a few minutes):

1. **How many tokens until the agent holds the answer?** Per route, estimated
   tokens read until every keyed gap is visible (owner, changed line, and the
   facts a test must pin), and the route's total.
2. **Does the loop work?** On `ripr check --format agent`, the loop
   `ripr init --agents` teaches: exactly the keyed gaps reported, a pinned
   control left alone, a weak test leaving the gap open, and the reference
   test ending with `verify=complete`. Each key is checked too, with the
   scenario's own test runner: reference passes at head and fails at base,
   weak passes at both.

It sits under the real-model cohort work (#12902, #12912), cheap enough to run
on every output or analyzer change. Results are static and advisory: a closed
loop means ripr's static verdict moved as expected, not that a test catches a
runtime fault.

## Run it

```bash
cargo build -p ripr --bin ripr
cargo xtask agent-eval --ripr target/debug/ripr --compare benchmarks/agent_eval/baseline.json
```

For the Python and TypeScript scenarios, the key checks and trials also need
`python3` with pytest and Node on `PATH` (checked with pytest 9.1 and Node
22.23; type stripping needs Node 22.6 or newer). Without them those
checks are instrument failures, so use `--skip-cargo` or `--scenario`. The
report is written to `target/ripr/reports/agent-eval.{md,json}`. The
command exits nonzero when `--compare` finds a regression. Options:

- `--scenario <id>` (repeatable) runs a subset. It cannot be combined with
  `--write-baseline`.
- `--skip-cargo` skips the key checks (`reference_key_valid`,
  `weak_key_valid` become `-`).
- Without `--ripr`, the command builds and uses the debug binary.

When your change is meant to move a number, rerun with `--write-baseline` and
commit `baseline.json` in the same PR, so its diff shows what moved. It holds
no timings, paths or dates; only `ripr_version` changes between builds.

## Routes

| route | what the agent runs |
|---|---|
| `cli-agent-loop` | `check --quiet --format agent`, then, for each printed `next=` whose gap row's `missing=` is `-` or a sentence rather than code, that command and the ones it prints (explain, then context), as the `init --agents` block instructs |
| `cli-agent-find` | the same first step only |
| `cli-human` | `check` with the default human output |
| `cli-json-terse` | `check --format json-terse` |
| `agent-brief` | `agent brief --json` |
| `mcp` | `initialize`, `tools/list`, `ripr_workspace_status`, `ripr_refresh`, `ripr_list_gaps`, then `ripr_get_gap` for each listed item that is not `exposed` |
| `mcp-brief` | the `mcp` flow with `RIPR_MCP_TEXT_CONTENT=brief`, the opt-in text companion for hosts that read `structuredContent` (#14327) |

Every route passes `--base main` (or `{"base": "main"}`) and runs in a fresh
copy of the scenario repository, so first-run state cannot leak between routes.

## Definitions

- **bytes**: a CLI step's stdout plus stderr, or for MCP the response line, after the scratch root is replaced by `<root>`.
- **tokens**: an estimate, not a billing count. Each run of ASCII letters,
  digits and `_` costs ceil(len/4), other non-whitespace characters cost 1.
  For MCP only what a host shows the model counts: the tool list, each call's
  `content` text, and the `initialize` instructions. Compare routes and
  revisions with it, not absolute costs.
- **tokens to answer**: cumulative tokens after the first step whose output,
  together with earlier steps, names every keyed gap's owner, line
  (`<changed file name>:N`, such as `lib.rs:N` or `backoff.py:N`, or a JSON
  `"line": N`) and all of its facts. A control scenario
  is answered once the route's discovery step (the check, brief or
  `ripr_list_gaps`) has been read. The loop checks decide whether the route
  really stayed quiet.
- **missed / extra gaps**: keyed lines with no gap row, and gap rows beyond
  one per keyed line. Two rows for the same changed line count as one extra.
- **regression** (`--compare`): a lost answer; total or to-answer tokens up
  more than 10% plus 20; more failed steps (nonzero exit, JSON-RPC error, MCP
  `isError`); more missed or extra gaps; a loop check that was yes and is now
  NO or missing (key checks under `--skip-cargo` are unmeasured); or, on
  a full run, a baseline scenario or route that no longer runs. A check already
  NO in the baseline is unguarded; a fix shows as a note and is rebaselined.
- A key check that cannot build, import or report a test result is an
  instrument failure: shown as `not established`, never NO. It fails every
  run mode and blocks `--write-baseline`.
- `cli-agent-loop` opens with the same FIND command that `cli-agent-find`
  runs, on an identical fresh workspace. When the two captures differ in
  exit, bytes or tokens, the run measured host noise (a stray stderr note, an
  unnormalized path), not ripr: that is an instrument failure too, and
  `--compare` refuses a baseline that recorded one.
- A silent control counts only when `check --format agent` exited 0, its
  header says `gaps=0` and it printed no `stop=`. The weak and reference
  checks also need exit 0. `--write-baseline` refuses `--compare`.
- The eval clears the caller's `RIPR_*` environment, closes stdin, runs git
  without hooks, and replaces the scratch root with `<root>` before measuring.

## Scenarios

Scenarios live in `xtask/src/reports/agent_eval/scenarios.rs`: a base and a
head text of one changed source file, any unchanged support files (module
declarations, package markers, re-exports), the tests the repository already
had, a weak and a reference test, and the answer key. Keys are hand-written
facts about the scenario, not ripr output. A unit test checks that every keyed
line really changed and that its owner exists at head.

Each scenario names its language, which decides the manifest, the test layout
and the runner the key checks and trials use:

| language | manifest | tests | runner |
|---|---|---|---|
| Rust | `Cargo.toml` | `tests/<name>.rs` | `cargo test --offline` |
| Python | `pyproject.toml` (`pythonpath = ["src"]`) | `tests/test_<name>.py` | `python3 -m pytest -p no:cacheprovider` |
| TypeScript | `package.json` (ES module) | `test/<name>.test.ts` | `node --experimental-strip-types --test` |

The first ten scenarios are single-file Rust crates. The `py_*`, `ts_*` and
`rs_*` ones keep the layout and code shape of the open-source project their
summary names (urllib3, packaging, requests, vercel/ms, slugify, semver), with
code written for the scenario rather than copied. They have nested packages,
an `__init__.py`, a re-exporting `index.ts` or a module tree under `src/`.

To add a scenario, append it there and rerun with `--write-baseline`. Good
additions are changes agents get wrong today, noisy reports, and silent
controls. Keep each project dependency-free (pytest and Node's built-in
runner only) so the eval stays offline.

## Agent-in-the-loop trials

The routes above replay ripr's output without a model. To see whether a real
agent writes better tests, or spends fewer tokens, with ripr than without:

```bash
cargo xtask agent-eval trial-setup <fresh-dir>   # one repo per gap scenario
# give an agent each repo, with or without ripr; it adds tests under tests/
cargo xtask agent-eval trial-score <fresh-dir>   # judges what it left behind
```

`trial-score` does not ask ripr. For every keyed gap it puts that one changed
line back to its base text and runs the scenario's whole suite (`cargo test`,
`pytest` or `node --test`). The gap counts as caught only when the suite
passes on the changed code, fails with the line reverted, and the changed
source file still matches the scenario's head text. A run that does
not compile or report a test result (for example a revert that breaks the
build) is recorded as not established and never counts as a catch; the
command still exits 0. A run that cannot be measured at all (cargo missing, a
timeout, a signal), a missing repository or a scoring error leaves that
repository at zero caught, still in the denominator, and makes the command
exit nonzero. While it scores, the scorer keeps the agent's copy of that file in
`.trial-score-agent-lib.rs` (published whole through a staging file) and
puts it back on every path; if a score is interrupted, the next run restores
from that copy first. Controls over the fourteen gap scenarios (15 keyed
gaps): a fresh setup scores 0/15 and the weak tests score 0/15. The reference
tests catch every gap: 9/9 on the Rust single-file scenarios when they were
added, and 6/6 on the cross-language ones (re-run when they were added).

Limits: agents must put tests under the scenario's test directory (a
`#[cfg(test)]` module in the changed file counts as a production edit). The
scorer guards only the changed source file. It does not check the manifest
(`Cargo.toml`, `pyproject.toml`, `package.json`), `build.rs`,
`.cargo/config.toml`, a `conftest.py`, the unchanged support files (an
`__init__.py`, `errors.py`, `index.ts`, `mod.rs`) or edits to the existing
tests, and a test that
asserts the source text itself (`include_str!`) would count as a catch, so
read the agent's tests before trusting a surprising score. Record each
arm's tokens and tool calls next to its score. Use a fresh directory per arm
and per run; if `trial-setup` stops partway, delete the directory and run it
again.

### First pilot (2026-11-11, one run per cell, Haiku 5.5, debug ripr with sources at main `2ab9f908`)

Same task text in both arms; the ripr arm also got the `init --agents` block
and the binary path. This pilot ran on the first five gap scenarios (six keyed
gaps), before the harder ones below were added.

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

## Baseline (debug build)

Route totals are the `routes` rows at the top of `baseline.json`. What they show:

- The first agent-format step already carries every answer. The `init --agents`
  DETAIL step is skipped when `missing=` is code (`total == 100`), so the loop
  follows explain and context only for rows whose `missing=` is `-` or a
  sentence ("No strong discriminator ..."). Before that rule the loop read
  8,731 estimated tokens over the original six scenarios; with it, 2,594 for
  the same answers (#14536). `baseline.json` records the current figure. A
  row with `(+N more)` or a clipped `…` value is followed too.
- `struct_field` closes since #14726: #14701 credits the bool-field
  assertion for the `field_construction` seam, and the predicate seam now reads
  the added `flag == "--verbose"` arm as its boundary, pairing only with an
  assertion that reads the `verbose` field. Before that, #14565 had stopped
  crediting the sibling field's assertion and the loop read `false`.
- `cli-human` shows one finding and hides the rest, so on the two-gap scenario
  the second gap stays hidden.
- MCP costs about 25 times the agent loop to reach the same answers (82,254
  against 3,287 estimated tokens over the ten scenarios). The tool list alone
  is about 4,300 estimated tokens per session.
- `mcp-brief` (#14327) reaches every answer for 56,168 tokens, against 82,254
  on `mcp`. When the route landed, its eight `ripr_get_gap` reads dropped from
  12,294 to 3,083, so the fixed `tools/list` cost now dominates.
- Loop: after the reference test, every one of the eight single-file Rust gap
  scenarios ends with `verify=complete`. `changed_constant_return` closed once
  #14380 (#14325) landed, `struct_field` once #14726 did, and `private_helper`
  in the baseline refreshed by #15152.
- Harder scenarios (`match_arm_value`, `private_helper`, `inclusive_range`,
  `refactor_control`):
  - `match_arm_value` and `inclusive_range` read correctly on the loop: the
    row names the new value, and the reference test closes it. When the
    scenarios were written, the row showed only the arm pattern (#14420) and
    the range read exposed from an `n = 0` test (#14419); both are fixed.
  - `private_helper`: FIND now prints the gap row (`missing=-`) and the
    reference test closes it, but the row does not name the private helper,
    so `cli-agent-find` does not count as answered; the loop reads it from
    explain/context (#11481).
  - `refactor_control`, a behavior-preserving extraction, prints no gap but
    still ends with a `stop=` line, so it is not silent
    (`silent_on_control: false`, #12770).
  - `agent-brief` reads the repo seam view, so it answers `match_arm_value`
    since #12078 (an arm no test input selects is weakly gripped there too).
    It still misses `inclusive_range`, whose gap depends on the before-text
    (`0..n` and `0..=n` agree at `n = 0`) that the repo view never reads, and
    `private_helper` (#13069). `cli-agent-find` misses `private_helper`.
- `error_variant` once got two identical gap rows for its one changed line;
  #14453 folded them, so it now reads `extra_gaps: 0`. `struct_field` no
  longer has an extra gap row.
- Cross-language and real-shape scenarios (`py_*`, `ts_*`, `rs_*`, added for
  #14298). The route rows in `baseline.json` sum all eighteen scenarios; the
  original ten alone, at the same binary, read `cli-agent-loop` 2,413 tokens to
  answer and 4,091 total (the eight new ones: 2,109 and 8,483, 7 of 8
  answered). Compare the original ten against older figures, not the sums.
  Every key holds with its own runner (pytest, `node --test`, cargo), and the
  two new controls (`py_refactor_control`, `ts_tested_control`) are silent. Of the six gaps:
  - `py_retry_backoff` works end to end: the row names `missing=attempt == 1`
    and offers a repair start, and the reference test closes it.
  - `py_invalid_version`: a raise changed to an imported `ValueError`
    subclass reads `static_unknown` (`limit=missing_import_graph`) with no
    FIND row, and `pytest.raises(InvalidVersion)` does not close it (#15356).
    The loop still learns the gap from explain, which names the owner and
    the before-text since #15502.
  - `py_default_headers` and `ts_slugify_default`: a changed dict value and
    a changed option default get `missing=-`, so FIND prints only the header
    count; the loop finds the fact in explain/context. The reference closes
    the Python one but not the TypeScript one (#15357).
  - `ts_duration_hour`: the boundary is a derived module const
    (`h = m * 60`); an exact test at `3_600_000` still reads
    `infect:unknown` / `observation_unconfirmed` (#15358). The row never names
    the owner, so no route but the human, JSON and full MCP output answers it.
  - `rs_semver_prerelease` (nested module): check reads the arm correctly, but
    `agent brief` withholds it as `strongly_gripped` though no test selects it
    (#15359), and the reference test does not close it (#15320).
  - `agent-brief` defers Python and TypeScript to `check` by design (its
    warning names the command), so it answers only the controls there.
