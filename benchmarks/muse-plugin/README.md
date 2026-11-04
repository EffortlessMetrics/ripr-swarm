# Muse plugin evaluation suite

This suite evaluates the native Muse RIPR plugin
(`plugins/muse/ripr`). It is not a test of the installed plugin by
construction: a `muse exec` process exiting 0 is not a pass. The
evaluator inspects the preserved artifacts against the independent
expected verdicts in `expected/`.

## Contract

Every run produces one `muse-plugin-eval-v1` record
(`schemas/ripr/muse-plugin-eval.schema.json`) plus retained raw
material: the initial fixture unchanged, the exact edits and Git diff,
the actual MCP exchange, before/after artifacts, executed test
results, the original transcript, and log digests. Missing cost,
token, or memory observations are `not_measured` or `unknown`, never
zero. Static RIPR verdicts and executed-test verdicts are separate
fields and separate judgments; neither substitutes for the other.

Five scores, never blended into one success number:

1. Installation and discovery — a genuinely new session obtains the tools.
2. Analysis quality — RIPR identifies a real gap without fabricating actionability.
3. Repair quality — the agent produces a valid, scoped test improvement.
4. Behavioral proof — the resulting test rejects the known wrong behavior.
5. Efficiency — wall time, time to first useful finding, calls, turns, tokens, reported cost.

Comparable conditions, when effectiveness is in scope: Muse
without RIPR; Muse with RIPR MCP tools but without the repair
skill; Muse with the complete plugin. Live-model results are
exploratory until a repeated baseline and variation exist; numerical
promotion thresholds come after that, not before. Deterministic
protocol and safety oracles gate immediately.

## Scenarios

`scenarios.toml` is the index. Fixtures under `fixtures/` are inputs,
never edited in place by a run; a run materializes its own copy.
`expected/<id>.json` holds the independently established verdict for
each scenario (a small scenario-verdict shape, distinct from the
run-record schema above). The TypeScript weak-assertion scenario is a
retained core-analyzer regression: it cannot be promoted to a passing
plugin result by narration.

## Receipts

`receipts/` retains completed run records and transcripts digests.
The first retained receipt (2026-11-04) covers cold installation,
approval, tool discovery, the boundary repair, and the
incorrect-repair control on Muse 1.4.14.

## Scoreboard connection

Results report beside the DX scoreboard (`xtask dx-scoreboard`) with
runner class, Muse version, plugin version, RIPR version, and
fixture identity preserved. Plugin runs never edit scoreboard
baselines, ceilings, or admitted comparisons; a single successful run
establishes existence, not a target. Ingestion into the scoreboard is
its own reviewed change.
