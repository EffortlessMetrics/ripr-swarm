# Formal-verification maintenance runbook

This runbook keeps the system current after the initial implementation.

## Reproducible execution

Start from the exact reviewed source. Record its commit/tree and any captured
dirty or untracked inputs before running a provider. Follow
[`rust-toolchain.toml`](../../rust-toolchain.toml) for ordinary Rust commands
and the separate pin in the [Kani workflow](../../.github/workflows/kani.yml)
for Kani. Kani's bundled compiler and solver are separate identities from the
product's Rust/MSRV pin; matching one version does not establish the others.

The existing lane supports a focused observation, for example:

```bash
cargo xtask kani --harness self_computed_cap --harness-timeout-secs 60 --timeout-secs 900
```

Inspect the selected and completed subjects, native exit and retained report.
The selector may match more than one harness. A source count or one successful
row does not establish that every expected subject ran. The provider budget
does not include installing tools or compiling the xtask front door; an outer
job budget must include those phases and owned-child cleanup.

Record the exact command, working directory, effective supported Cargo/Kani
configuration, provider/compiler/solver identities, source fence, native
status and raw artifacts. A changed flag such as disabled default checks,
an unregistered stub, a different body under `cfg(kani)`, or an unknown
configuration invalidates the original production claim even when the solver
returns success. Hashes preserve identity; admission must separately enforce
which configurations and trust boundaries are allowed.

The historical lane retains Markdown only. #14472 owns native JSON, raw
custody and complete inventory; #14478's offline auditor checks supplied
documents and cannot substitute for that execution. Use the
[work index](WORK_ITEMS.md) to distinguish landed commands from candidates.

## Claim-change acceptance record

Keep the acceptance record with the owning spec/claim and PR. It needs:

| Item | Evidence to retain |
| --- | --- |
| Proposition | Exact owner, input domain, observable and independent expected behavior |
| Positive proof | Current native result and every required reachable partition |
| Behavioral control | Compiling wrong behavior, the named failed assertion/invariant and raw outcome |
| Domain guard | Constructor addition/omission controls and the specific compile-time guard that refuses them |
| Production correspondence | Real caller or state-transition test; pure helper proof alone has a narrower scope |
| Execution identity | Source/configuration/tool/process/inventory/artifact bindings |
| Maintenance | Generation change, affected selection, rollback and explicit remaining nonclaims |

A compile-time domain guard is a structural control. It must not be counted
as a behavioral mutation result. Report each mutant as detected, survived,
equivalent after review, unsupported, timed out or not run. An equivalent or
unrun control is not an observed detection; a timeout is not a counterexample.

## Routine maintainer checks

### On every formal claim change

- validate the claim registry;
- compare base/head authority;
- run affected critical claims with all-critical fallback if selection is
  incomplete;
- run the named negative control when its source closure changed;
- verify required covers;
- regenerate the assurance scoreboard;
- retain exact machine and raw artifacts;
- review nonclaims and user-facing wording.

### On every runner or result-parser change

Run canaries for:

- missing tool;
- preflight timeout;
- whole-run timeout;
- per-subject timeout;
- nonzero/absent process exit plus favorable-looking output;
- missing result;
- duplicate source and native results;
- neighboring same-name result;
- malformed/truncated machine artifact;
- stale source/configuration/claim generation;
- incomplete shard inventory;
- unreachable required cover.

A runner change is not qualified by its unit tests alone. It requires at least
one exact native positive and the relevant native negative controls.

### On every provider/toolchain upgrade

Record:

- old and new versions;
- schema/output changes;
- changed semantics, defaults, bounds, solver, and target;
- claim rows affected;
- replay of retained native fixtures;
- current positive and negative native runs;
- performance/resource comparison;
- rollback path.

A version bump invalidates current native evidence unless the provider
contract explicitly establishes compatibility.

## Adding a claim

1. Reuse the existing issue/spec owner.
2. Write the proposition and nonclaims first.
3. Identify exact production owner and source closure.
4. Select the right proof family.
5. Add registry row and generation.
6. Add harness/model, assumptions, bounds, and covers.
7. Add negative control.
8. Execute the native provider.
9. Publish machine result and raw evidence.
10. Add the row to the assurance scoreboard.
11. Review CI phase and transition behavior.

## Triage by status

### `FAILED`

- confirm exact source, claim, provider, and bounds;
- identify whether the failure is a real counterexample, invariant violation,
  or unreachable required cover;
- preserve the counterexample;
- do not repair the proof by weakening domain, assumptions, covers, or
  proposition unless that change is independently intended and reviewed.

### `NOT_ESTABLISHED`

- distinguish tool absence, timeout, abnormal process, missing row, zero
  subject, ambiguity, malformed artifact, and resource failure;
- keep completed exact rows;
- repair the instrument or redesign the proof;
- do not call it a product counterexample or pass.

### `STALE`

- name every mismatch axis;
- retain historical evidence;
- rerun on exact current inputs;
- never rebind by matching harness name, count, or Git HEAD alone.

### `UNSUPPORTED`

- keep the applicable proposition visible;
- name the missing provider/route;
- select a manual, later, or alternative evidence owner;
- do not convert unsupported to not applicable.

## Proof performance problems

Preferred remedies:

- isolate a smaller production pure core;
- tighten symbolic types to the real domain;
- add exact bounds plus covers;
- split independent claims;
- execute expensive claims individually;
- shard by measured cost;
- use another verification method where it is more honest.

Forbidden remedies:

- silently remove the claim;
- lower bounds without transition review;
- delete covers;
- copy production logic into the harness;
- trust another source/config result;
- treat timeout as failure or success;
- raise global timeouts without measuring the complete route.

## TLA+/state-machine models

A model result and implementation conformance are separate.

For every admitted model maintain:

- model and configuration identity;
- state/action-to-production mapping;
- invariants and liveness properties;
- finite bounds and symmetry assumptions;
- deliberately wrong model configurations;
- native positive and expected counterexample outputs;
- implementation conformance tests;
- unsupported implementation behavior.

Changing either the model or production transition requires currentness and
conformance review.

## Retention

Retain:

- canonical machine result;
- raw provider output or content-bound reference;
- claim registry generation;
- source/config/model/tool commitments;
- counterexamples and negative-control identity;
- timeout and failed attempts;
- resource/performance observations;
- transition decisions.

Do not retain only the fastest or successful attempt.

## Documentation drift

These documents must change with the owning implementation:

| Change | Required documentation |
| --- | --- |
| Claim/result schema | CLAIMS.md, RESULTS.md, examples, generated reference |
| Kani lane status | RIPR-SPEC-0371, CI.md, workflow docs |
| Required-check policy | CI.md and #12904 decision |
| New provider family | README.md, claim family registry, provider docs |
| Transition semantics | CLAIMS.md, MAINTENANCE.md, #5224/#6452/#6456 |
| User-facing support | public support/capability docs and actual route evidence |
| Model admission | model README, mapping, nonclaims, native evidence |

A green documentation gate does not establish that a native tool ran. The
assurance scoreboard is the maintained current-state projection.

## Periodic review

At least once per toolchain or release cycle:

- regenerate source census;
- regenerate assurance scoreboard;
- list claims without current results;
- list claims without negative controls or covers;
- inspect chronic timeout/resource rows;
- inspect unsupported applicable obligations;
- inspect required/advisory phase drift;
- exercise runner canaries;
- verify public support claims against actual routes;
- reconcile open owner issues and stale Draft PRs.
