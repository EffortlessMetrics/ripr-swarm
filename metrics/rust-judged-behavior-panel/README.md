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

The seed records the test/oracle evidence required for later reconstruction.
A subsequent execution PR must materialize exact repositories, source/test
artifacts, RIPR binary/config identities, and current output before any label is
populated.

`cargo xtask rust-judged-panel check` is the retained semantic guard for this
seed. It validates the typed manifest contract, selected directions, explicit
null-as-unjudged state, and exact Rust-token anchors on added diff lines. The
same checker is reached by required precommit policy. Passing it establishes
only that the selected seed is internally coherent; it does not establish an
analyzer result, replay identity, judgment, rate, or support claim.

## Offline replay packets

Build the workspace binary and replay the selected denominator without using
PATH or the network:

```bash
cargo build -p ripr
cargo xtask rust-judged-panel replay \
  --ripr-bin target/debug/ripr \
  --out target/ripr/rust-judged-panel
```

The replay command consumes this manifest through the same strict validator,
materializes deterministic Git repositories for all three directions, invokes
only the explicitly supplied binary under a bounded process-tree deadline, and
retains raw stdout/stderr plus one internal packet per case. Packets bind the
manifest row, selected diff, Git base/head/tree, governed source/test/config
bytes, binary digest/version, stable argv, analyzer input identity, raw-output
digests, and exactly one anchor projection. Scratch paths and wall-clock time
are excluded from packet identity.

These packets are replay evidence, not judgments. Judgment labels stay null,
runtime calibration stays `not_run`, and the command emits no rate, badge,
support-tier, release, mutation-adequacy, or correctness claim. Generated
packets live under `target/` and are not committed source truth.

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
