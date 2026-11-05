# Muse plugin evaluation suite

This suite evaluates the [native Muse RIPR plugin](../../plugins/muse/ripr/README.md).
A `muse exec` process exiting 0 is not a pass. An evaluator must compare the
preserved artifacts against the independent expected verdict for the scenario.
The retained observations below do not establish that the entire suite passed.

## Run contract

For acceptance, a new run must produce one `muse-plugin-eval-v1` record matching
the canonical `schemas/ripr/muse-plugin-eval.schema.json`. The shared evaluation
consumer owns that schema and its version bindings
([#13120](https://github.com/EffortlessMetrics/ripr-swarm/issues/13120), implemented
in [#13165](https://github.com/EffortlessMetrics/ripr-swarm/pull/13165)); this
package does not publish an independent schema. Complete run-record validation
depends on that canonical consumer.

Retain raw evidence: the unchanged initial fixture, exact edits and Git diff,
actual MCP exchange, before/after artifacts, executed test results, original
transcript, and log digests. The existing receipts are historical Markdown
summaries; no completed machine-readable run record is committed here yet.

Identify the exact plugin/package bytes, manifest hash, Muse version, RIPR
binary/build, platform, fixture revision, repository root identity, source,
and base. Record actual before/after snapshot IDs, canonical gap, seam, durable
attempt ID, and receipt route when produced. Use the schema's missing-value
representation: omit unavailable optional counters or use `null`; use
`not_measured` or `unknown` only where the field permits a string. Never
substitute zero for an unmeasured value. Static RIPR verdicts and executed-test
verdicts are separate fields and judgments.

Five scores remain separate:

1. Installation and discovery: a new session obtains the declared tools.
2. Analysis quality: RIPR identifies a real gap without fabricating actionability.
3. Repair quality: the agent makes a valid test edit within the allowed surface.
4. Behavioral proof: that exact test rejects the known wrong behavior.
5. Efficiency: wall time, time to first useful finding, calls, turns, tokens, and reported cost.

When effectiveness is in scope, compare Muse without RIPR, Muse with RIPR MCP
tools but without the repair skill, and Muse with the complete plugin under
comparable conditions. Live-model results remain exploratory until repeated
baselines and variation are available. Deterministic protocol and safety
oracles apply immediately; numerical promotion thresholds require evidence.

## Scenarios and isolation

[scenarios.toml](scenarios.toml) indexes eight scenarios. Fixtures under
[fixtures/](fixtures/) are inputs, never edited in place by a run. Materialize
an owned disposable copy. Files under [expected/](expected/) are independent
scenario verdicts, distinct from the full run-record schema.

| Scenario | Check |
| --- | --- |
| `rust-boundary-gap` | Add the missing boundary discriminator; retain static, executed, and durable evidence independently. |
| `no-actionable-gap` | Report the available static scope without inventing an actionable repair. |
| `missing-or-unapproved-server` | Diagnose binary, approval, or discovery failure visibly. |
| `stale-snapshot` | Obtain current worktree evidence after an edit; do not reuse a retained earlier snapshot as current. |
| `changed-head-or-baseline` | Preserve identities and report the actual base/attempt refusal when comparability is lost. |
| `test-only-edit-constraint` | Keep the edit within the packet's permitted test surface. |
| `typescript-weak-assertion` | Retain the analyzer weakness as a regression; narration cannot promote it to passing. |
| `incorrect-repair-control` | Recognize an interior-value test that passes on both behaviors as non-discriminating. |

For a durable repair, capture a successful `--phase before` and its actual
attempt ID **before editing**, then use its printed after command. An
in-memory MCP transaction cannot substitute for that baseline. Keep logs
outside the repair root and preserve the producer's refusal/recovery if a
phase fails. See the [durable attempt contract](../../docs/REPAIR_ATTEMPT.md).

Test the exact added discriminator on correct, wrong/pre-change, and restored
behavior in an isolated copy containing the relevant current source and saved
test bytes. Require pass, intended behavioral failure, then pass. An unrelated
suite failure, compile error, or zero selected tests is insufficient. Check
that the control leaves the original fixture and worktree unchanged.

Run `incorrect-repair-control` independently with only the candidate added
interior test. A second, correct boundary test can make the whole suite fail
on the wrong behavior while the candidate interior test still passes. That
mixed result does not establish an incorrect-only repair refusal.

## Retained observations and limits

The two files in [receipts/](receipts/) are reported historical observations
on Windows/Muse Code 1.4.14 with RIPR development build
`7fd1d0e00591d0b9cc18ce8a8f5a5130a037b7a3`. Preserve their identities and bytes;
a later documentation change does not rerun them.

| Receipt | What it reports | What it does not establish |
| --- | --- | --- |
| [Cold installation and repair](receipts/2026-11-04-cold-install-and-repair.md) | Earlier package `dbb837d7e9` under `dist/muse`: approval, tool discovery, boundary-test repair, and correct/wrong/restored execution. | No durable CLI attempt ID. The interior-value example ran alongside the boundary test, so it is not an isolated incorrect-only journey. |
| [Specified layout and durable attempt](receipts/2026-11-04-spec-layout-journey.md) | Recorded pre-reshape package hashes under `plugins/muse/ripr`, server `core`, a fresh Rust boundary journey, attempt `repair-attempt-0e453aadc3aeeb4a132fcffd`, static receipt, and separately reported test results. | The durable receipt records no test run. The journey is not a fresh execution of all eight scenarios or later package revisions. |

The second receipt's stale-snapshot coverage is the successful journey's
worktree refresh, not a separately retained stale-read refusal trial. Its
missing-server and incorrect-control coverage refers back to the earlier
package. It explicitly leaves `no-actionable-gap`, `changed-head-or-baseline`,
`test-only-edit-constraint`, and `typescript-weak-assertion` without dedicated
end-to-end runs. A test-only diff in one successful journey is not the same
as exercising a rejected out-of-scope edit.

Repeated reproduction, with/without-plugin effectiveness, elapsed-time
targets, peak memory, and cross-agent compatibility remain unestablished by
these receipts. Retain unmeasured and unrun dimensions explicitly in future
records; do not infer them from a successful install or static improvement.

## Scoreboard connection

Results report beside the DX scoreboard (`xtask dx-scoreboard`) with runner
class, Muse version, plugin version, RIPR version, and fixture identity
preserved. Plugin runs do not edit scoreboard baselines, ceilings, or admitted
comparisons. A single run establishes an observation, not a target. Ingestion
into the scoreboard is a separate reviewed change.
