# Repair-attempt finish lifecycle pilot

This is a finite native TLA+/TLC model of an existing production boundary.
It is not a new repair orchestrator, a replacement for edit-cage evaluation,
or proof that the Rust implementation refines the model.

## Actual owner and bounded claim

The source owner is
[repair_attempt/mod.rs](../../crates/ripr/src/app/repair_attempt/mod.rs).
The model abstracts these existing operations:

| Model operation | Production owner | Boundary |
| --- | --- | --- |
| Capture | `after_phase_window_is_current` and after-phase edit-cage evaluation | Existing scoped observations, not a generic verification bit |
| FinishState | `repair_attempt_state_for_finish` | Six currentness/verdict combinations map to existing durable states |
| Commit / Refuse | `finish_repair_attempt_with_workflow_backups_from` through `commit_manifest_locked` | AwaitingEdit is rechecked under the existing manifest commit lock |

One attempt begins in `AwaitingEdit`; two finishers can capture observations
and race to commit. The native model checks that:

- `ReadyToFinish` requires a current captured after phase.
- `ReadyToFinish` requires a compliant cage verdict.
- At most one finisher commits before a reset or new attempt.

The model deliberately includes all six currentness/verdict pairs. The real
producer coerces a non-current verdict to incomparable, so these inputs
overapproximate that producer while covering the classifier's full domain.

[assumptions.json](assumptions.json) is the exact assumption input.
No fairness or liveness property is claimed. Two finishers are a finite scope,
not a theorem for arbitrary concurrency. The lock/recheck/write is abstractly
atomic; the model does not establish filesystem atomicity, crash recovery,
later receipt currentness, full causal invalidation, or arbitrary repairs.

## Native execution and evidence

The existing unpublished Rust xtask owns the thin native adapter:

```console
cargo xtask repair-lifecycle-model check --tlc-jar /path/to/tla2tools.jar --out target/ripr/reports/repair-lifecycle-fresh-run
```

The output directory must not already exist. The command snapshots original
model/config/assumption files and the verified jar before execution. It uses
the existing bounded process-capture owner, one TLC worker, a 256 MiB Java heap,
and a 60-second deadline per config. Native output is capped at 4 MiB stdout
and 1 MiB stderr; overflow refuses rather than truncating to a pass. It preserves raw native stdout/stderr,
version output, exact argument/scope records, source commitments and a JSON
result. It checks inputs again after execution and refuses mixed snapshots.

The jar is the official immutable
[TLC 1.7.4 release asset](https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar):

- Size: 2,274,532 bytes.
- SHA-256: `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`.
- Upstream published SHA-1: `bee4a54f3ee3d4afc347c3240ec2d9e93b075104`.
- SHA-256 cross-check: NixOS/nixpkgs
  `pkgs/by-name/tl/tlaplus/package.nix`, blob
  `ac8c01557736fd2d89d137d07ada03b86dbd40a6`; its SRI value is
  `sha256-k2omIGHJFGlN/WaaVDviRXPEXVqg/yCouWsj0B4FDog=`.
  Cursor pstack's `skills/statechart/scripts/run.ts`, blob
  `4efd368e5bcd0b4ef7cf8d8cfd1e511483c720aa`, independently pins the same digest.

A positive result requires native exit zero, complete TLC tool frames,
the native success event, at least two generated and distinct states, and an
empty exploration frontier. Exit zero alone, unframed success text,
truncation, timeout and a parser failure never establish the claim.

All three negative controls are mandatory:

| Original config switch changed | Required native result |
| --- | --- |
| IgnoreCurrentness = TRUE | Safety exit 12, FinishRequiresCurrentSnapshot violation, native counterexample trace |
| IgnoreCageVerdict = TRUE | Safety exit 12, FinishRequiresCompliantCage violation, native counterexample trace |
| BypassAwaitingGuard = TRUE | Safety exit 12, AtMostOneFinish violation, native counterexample trace |

Every config retains the original invariant set. An unrelated solver failure
does not count as a successful negative control. No failed invariant is
removed to obtain a passing exploration.

The JSON uses `proof.state-machine-evidence-binding.v1` for **model_check**
records. Digests bind the actual model bytes, property record, assumptions,
native scope and toolchain observations. `model-closure.json` commits the
actual source plus the complete pinned jar supplying Naturals, FiniteSets and
their transitive standard-module dependencies. The fixed pilot refuses extra
imports, nested modules or INSTANCE declarations until its closure is reviewed.
The toolchain preimage also commits the native adapter/parser, the existing
bounded process owner, xtask entry/dispatch/catalog, Cargo manifests/lockfile
and Rust toolchain pin. An adapter-only change invalidates this model-evidence
identity; it is not hidden only in a separate source record.
Model-only bindings deliberately omit
implementation/adapter digests. Separate source-correspondence records do not
become implementation-conformance receipts merely because sources were hashed.

## Separate production obligations

[properties.json](properties.json) maps stable property IDs to existing owner
symbols and the real tests. The pilot reuses rather than duplicates:

- `finish_state_follows_verdict_status_and_currentness`: the six actual
  classifier outcomes, including non-current/compliant rejection.
- `concurrent_finish_attempts_leave_exactly_one_winner`: the real finish
  operation under two concurrent callers, rejecting double success.
- `finished_attempt_currentness_follows_worktree_drift_like_the_receipt`:
  later drift is a separate currentness obligation.

The normal required Rust nextest lane (or its existing shards) executes these
tests and retains its own JUnit evidence. The model command neither runs nor
claims to pass them. Focused local qualification can use:

```console
cargo test -p xtask repair_lifecycle_model::tests
cargo test -p ripr finish_state_follows_verdict_status_and_currentness
cargo test -p ripr concurrent_finish_attempts_leave_exactly_one_winner
cargo test -p ripr finished_attempt_currentness_follows_worktree_drift_like_the_receipt
```

This is source-bound correspondence plus separate production tests, not an
automatic complete refinement proof or a general TLA-trace-to-Rust replayer.

## Existing CI route, unchanged admission

The existing reusable `rust-gates.yml` job selects this pilot after its warm
policy preflight. It checks the exact admitted base against the candidate
without rename elision. Lifecycle, model, adapter, workspace/toolchain and
workflow changes select the model; an unavailable/unknown base selects it
conservatively. Unrelated documentation does not install Java or download TLC.

The selected step sets up Java, obtains and verifies the immutable jar,
executes all four original configs, and retains a small native evidence
artifact separately from Rust test evidence. Missing Java, a bad jar or
unestablished native evidence fails that invocation.

This hook does not add an event, open a PR, activate a queue, change Draft/Ready
admission, change required-check names, or authorize a hosted run. It becomes
part of whichever existing Rust job is independently authorized and admitted.

## Repeatable practice

For another lifecycle, carry the same bounded sequence:

1. Discover a candidate from actual branches, flags, status transitions,
   persistence boundaries and failures. Retain source anchors and the evidence
   that makes it a state machine; discovery is not acceptance.
2. Separate the as-is machine from intended behavior, unknown transitions and
   the proposed correction. Choose a bounded production seam and its owner.
3. Record the native model, properties, finite scope, assumptions, production
   symbols and existing executable obligations. Review abstraction boundaries.
4. Add known-wrong model controls and production-facing falsifiers. Require
   semantic failure for the intended reason, not just a nonzero tool exit.
5. Select affected checks when any bound input changes. Do not reuse evidence
   across changed model, property, scope, assumptions, toolchain or adapter.
6. Retain raw execution and limitations. Distinguish model_check,
   implementation_conformance and implementation_proof throughout consumers.

This first pilot supplies the pattern. Automated semantic discovery,
cross-repository atlas integration, native trace replay into the production
implementation, authenticated execution and universal receipt invalidation
remain separate work. Do not report them as implemented by this directory.
