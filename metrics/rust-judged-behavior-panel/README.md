# Rust Judged Behavior Panel Seed

Authority: [#3164](https://github.com/EffortlessMetrics/ripr-swarm/issues/3164).

This directory fixes the first selected denominator for auditing both visible
bad repairs and silent over-credit in Rust analysis. It is deliberately a
**metrics seed**, not a runnable RIPR fixture, accuracy result, or judging
engine.

## Why emitted findings are insufficient

Reviewing only RIPR findings can identify a visible error:

```text
false_actionable
  RIPR routed a repair for behavior that was already discriminated
```

It cannot identify the quiet error:

```text
false_exposed
  RIPR credited exposed/strongly_gripped and emitted no repair,
  but no aligned oracle actually discriminates the behavior
```

The selected panel therefore contains all three directions:

| Direction | Expected static behavior | Error exposed by the case |
| --- | --- | --- |
| `should_gap` | report a gap, weakness, or exact missing discriminator | silent false-`exposed` if RIPR stays quiet |
| `should_stay_quiet` | retain exposed/strong evidence and route no repair | false-actionable if RIPR emits repair work |
| `should_limit` | retain a typed limitation and no repair | false-actionable or false-`exposed` if RIPR crosses the unresolved edge |

A gap-only panel cannot measure either side correctly.

## Files

- `manifest.json` — selected, unjudged cases. Every judgment label is `null`.
- `diffs/*.diff` — the exact changed production behavior for each synthetic
  seed case.
- `subjects.json` and `subjects/*` — independent checked source, test, config,
  diff, semantic-selection, and deterministic Git identity authority for later
  replay materialization.

The seed records the test/oracle evidence required for later reconstruction.
The subject authority proves that the three selected repositories materialize
at exact base/head/tree identities from independently hashed inputs. A later
execution PR must still build and invoke RIPR and retain host run receipts before
any replay result exists or any label is populated.

`cargo xtask rust-judged-panel check` is the retained semantic guard for this
seed and its subject authority. It validates the typed manifest contract,
selected directions, explicit null-as-unjudged state, exact Rust-token anchors,
subject digests, manifest joins, and deterministic Git materialization. The
same checker is reached by required precommit policy. Passing it establishes
only that the selected seed and independent subject inputs are internally
coherent; it does not establish analyzer execution, a replay result, judgment,
rate, or support claim.

## Host-bound replay receipts

`cargo xtask rust-judged-panel replay --out target/ripr/rust-judged-panel`
owns a fresh `cargo build -p ripr --locked --offline` in a run-local target,
then invokes those exact hashed bytes against all three materialized subjects.
It retains byte-exact stdout/stderr plus typed source, build, binary, host,
argv, config, diff, process, timeout, and analyzer-input identities below the
ignored `target/` tree.

Each attempt is staged under an exclusive lock. Only a validated three-case
generation receives `run-index.json`, is moved into the immutable `runs/`
namespace, and advances `current.json` last. A failed, partial, or concurrent
attempt cannot become current. The build has no network fallback: an offline
cache miss is a failed attempt.

These files are host-bound run receipts. They do not by themselves select or
bless findings, interpret quiet output, populate judgments, or support a
mutation, rate, gate, badge, or support-tier claim.

## Portable bounded projection

`cargo xtask rust-judged-panel packet-check --host-current
target/ripr/rust-judged-panel/current.json` first validates the complete
host-current/index/receipt/raw chain, then projects, serializes, strictly reads
back, and validates all three cases without writing output. The bounded packets
bind the governed subject, producer and run plan, exact input identity, probe
path/line/family/expression and enclosing Rust owner, observed class, and the
independently governed direction-specific missing/recommendation/limitation
witness. Duplicate or unknown JSON keys fail closed.

`should_stay_quiet` is exactly one `exposed` finding with no action evidence;
the macro case is a complete `no_static_path` finding with its named static
limitation, not a timeout or incomplete run. Judgment remains explicitly null
and runtime calibration remains `not_run`. This command creates no portable
generation, index, current pointer, staging directory, or publication lock;
retained publication is a separate ordered follow-up.

## Item contract

Each `items[]` row carries:

- stable case identity;
- exact diff path and behavior anchor;
- one of the three expected directions;
- behavior family, required discriminator, and expected static class/limit;
- relation, oracle, activation, observer, propagation, and target dimensions;
- all-null adjudication labels;
- explicit runtime calibration status;
- non-claims and a load-bearing reason.

`false_actionable` and `false_exposed` remain separate. At most one may be true
for a completed case. `static_under_credit` is separately retained because a
conservative static gap can coexist with a caught runtime mutant without being
a false-actionable result.

## Selection and denominator rules

1. Every seed item is selected before judgment.
2. A difficult, unfavorable, unsupported, timeout, or inconclusive item remains
   in the denominator.
3. Null labels are not passes and do not support rates.
4. Structural judgment must cite exact owner, activation, path, sink, and
   observer evidence rather than copying RIPR output.
5. Runtime mutation calibration is a separate fact. It may challenge the
   static judgment but never silently rewrites it.
6. A rate always names its numerator, denominator, excluded/inconclusive count,
   case coverage, and exact as-of identities.
7. The panel never becomes a default gate, badge, RIPR Zero input, or support
   claim without a later explicit promotion decision.

## Seed coverage

The initial selected cases are:

1. **Boundary should gap** — direct owner tests have exact return assertions but
   omit the equality input that distinguishes `>=` from `>`.
2. **Boundary should stay quiet** — the exact equality input and aligned return
   assertion are present; repair routing would be false-actionable.
3. **Macro reach should limit** — a macro witness suggests a test path, but the
   expanded owner-to-observer path is unresolved; crediting exposed would be a
   silent over-promotion.

These cases establish the schema shape only. They are not representative error
rates.

## Next slices

```text
materialize exact replay repositories and analysis identities
→ retain RIPR outputs and bounded evidence packets
→ record independent structural judgments
→ resolve disagreements visibly
→ add exact targeted mutation receipts where safe
→ emit stratified confusion/agreement reports
→ turn confirmed failure families into permanent analyzer fixtures
```

## Boundaries

No analyzer change, automated judgment, mutation execution, provider call,
generated test, repair assignment, gate, or support-tier promotion is made by
this seed.
