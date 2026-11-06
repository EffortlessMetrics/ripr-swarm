# Canonical product-gate plan

Issue #3330 establishes the typed authority for ordinary product gates before
any workflow migration. The implementation lives in
`tools/repo-policy/src/product_gate_plan.rs` and is intentionally pure: it describes gate
meaning and applicability without selecting a runner, invoking a provider, or
changing required-check routing.

## Current producer inventory

The current producer is `.github/workflows/rust-gates.yml`, called by the
CX43, CPX42, CX53, and GitHub-hosted routes in `routed-rust.yml`. The required
single-command gates have separate named steps; the conditional composites below
retain their existing step ownership, including the id-less user-text steps.
They are not copied into each runner route. Runner identity and matrix shape are route details, not
product meaning.

| Canonical gate | Current command / composite anchor | Role | Surface |
| --- | --- | --- | --- |
| `product.rust.formatting` | `cargo fmt --check` | required | Rust |
| `product.rust.all_features_check` | `cargo check -p ripr --all-targets --all-features` | required | Rust |
| `product.rust.clippy` | `cargo clippy --workspace --all-targets -- -D warnings` | required | Rust |
| `product.rust.workspace_tests` | `cargo nextest run --workspace --profile ci` | required | Rust |
| `product.rust.workspace_doc_tests` | `cargo test --workspace --doc` | required | Rust |
| `product.repository.precommit` | `cargo xtask ci-policy` | required | repository policy |
| `product.evidence.promotion_honesty` | `cargo xtask check-evidence-promotion-honesty` | required | evidence |
| `product.repository.agent_skills` | `cargo policy preflight` | required | repository policy |
| `product.repository.dependencies` | `cargo xtask check-dependencies` | required | repository policy |
| `product.repository.process_policy` | `cargo xtask check-process-policy` | required | repository policy |
| `product.repository.network_policy` | `cargo xtask check-network-policy` | required | repository policy |
| `product.evidence.goldens` | `cargo xtask goldens check` | required | evidence |
| `product.evidence.fixtures` | `cargo xtask fixtures` | required | evidence |
| `product.evidence.capability_producers` | `cargo xtask check-capability-producers` | required | evidence |
| `product.evidence.capability_docs` | `cargo xtask capability-docs --check` | required | evidence |
| `product.evidence.verdict_corpora` | `cargo xtask verdict-corpus check-all` | required | evidence |
| `product.repository.base_user_text` | `cargo xtask check-user-text --tree "$GITHUB_WORKSPACE"` | required on PR/merge-group events | conservative: all declared surfaces |
| `product.evidence.pr_evidence` | `cargo xtask ripr-pr --base "$BASE_SHA" --head "$HEAD_SHA"` | required on PR/merge-group events | conservative: all declared surfaces |
| `product.evidence.schema_producer_sweep` | `cargo xtask schema-producer-sweep "${artifacts[@]}"` | required | conservative: all declared surfaces |

`product.rust.clippy` also carries the plain workspace type-check. `cargo clippy
--workspace --all-targets` and `cargo check --workspace --all-targets` build the
identical 523-unit set (same targets, features and profile; verified with
`cargo ... -Zunstable-options --unit-graph`), so a separate `product.rust.workspace_check`
step only repeated the compile of the workspace crates (about 2.2 minutes of a
hosted run) and was removed in #10341. A type error fails Clippy.

The Rust test contract is `canonical_nextest_plus_cargo_doc` (#3825,
`TEST_RUNNER_CONTRACT` in `tools/repo-policy/src/product_gate_plan.rs`). It is
intentionally dual:

| Subject | Required owner | Features | Evidence retained |
| --- | --- | --- | --- |
| lib, bin, integration, and example test binaries | `cargo nextest run --workspace --profile ci` | default | fresh `junit.xml` naming at least one test, plus `run-context.txt` with checkout SHA, tool versions, and blob identities of `Cargo.lock`, `.config/nextest.toml`, and `rust-gates.yml` |
| Rust doctests | `cargo test --workspace --doc` | default | job log only |
| `lang-perl` and other non-default-feature tests | compiled (not run) by `cargo check -p ripr --all-targets --all-features`; not executed in the required lane | all / perl / no-default | advisory Test Analytics, the `Perl and release proof` job, and the Windows advisory feature matrix |

Nextest cannot execute doctests, so a green nextest row never stands in for the
doctest row, and neither row claims non-default-feature subjects. The `ci`
profile pins `retries = 0` and may not declare a `default-filter` or
per-test overrides, so every test selected by default is required; the
`framed_lsp_`/`editor_agent_loop_` skip in `.config/nextest.toml` is a local
iteration hint and is never applied in CI. Nextest exit 0 without a fresh JUnit
report naming at least one test fails the step, so an empty selection cannot
render green. A change to the nextest config, the workflow, or `Cargo.lock`
changes the blob identities recorded in `run-context.txt`, so an older receipt
cannot be read as evidence for the new inputs.

Hosted sharded form (#10507, hash layout #12777). On the GitHub-hosted route the same row runs as
four `cargo nextest run --workspace --profile ci --partition hash:N/4` jobs in
`rust-test-shards.yml`, started by `routed-rust.yml` only after the hosted
`rust-gates.yml` job (every other gate, doctests and PR evidence) passed; that
job is called with `test-shards: 4`, so its own `rust-tests` step is skipped, the
single `if: inputs.test-shards == 1` condition `product_gate_plan.rs` allows on
that row. The self-hosted routes keep the unpartitioned step in their gates job.
A partitioned run selects exactly the tests of the unpartitioned command because
`rust-test-shards-verify` runs `xtask verify-test-shards` (the binary shard 1 built), which fails the
hosted result unless each shard retained status 0, a run context naming this
repository, checkout, run, attempt, profile, partition and command with
`nextest_exit=0`, and a complete JUnit report with no failure whose testcases
are exactly that shard's partition list, and the partition lists are pairwise
disjoint and equal to the full workspace list; and `routed-rust.yml`'s
`result` job fails unless that job passed (also after a self-hosted tempfail
fallback). A missing, cancelled or red shard is a failure, never a pass. After
a partial "re-run failed jobs", each shard's evidence is its latest artifact
for this run at or below the verifying attempt (#10605), and the combined
context discloses `verified_attempts` whenever any accepted shard's evidence
comes from an attempt other than the verifying attempt, so reused results are
never presented as fresh. Each
shard retains its own `junit.xml` and `run-context.txt` (the context names the
partition and the blob identities of this workflow too), and the verifier writes
the combined `junit.xml` and `run-context.txt` plus `shards-summary.txt` under
the unsharded `<artifact>-nextest-<attempt>` name, so consumers of that
evidence, including the Contabo product contract, read the same files. The
Contabo importer accepts the one `if: inputs.test-shards == 1` condition on the
nextest row (it rehearses the unpartitioned form) and refuses any other. The
parallel form that runs the gates and shards at once would save more time but
removes the gates-first ordering, so it is not the required shape.

Focused tests in `product_gate_plan.rs` read the real workflow, nextest
config, and this table. Each of these fails `cargo nextest run`:

- removing, filtering, or respelling a required runner row, or adding any
  other single logical line (backslash continuations joined) that invokes
  `cargo test`/`cargo t`/`cargo nextest run`/`r`, including behind wrappers,
  leading flags, or a `+toolchain`;
- an `if:`, `continue-on-error:`, or `shell:` key (bare or quoted) on a runner
  step; an `if:` or `continue-on-error:` on any job; a `defaults:` block; or a
  doctest step that is anything other than exactly
  `run: cargo test --workspace --doc`;
- any `NEXTEST_*`, `RUSTDOCFLAGS`, or `CARGO_TARGET_*_RUNNER` variable in the
  workflow;
- a `default-filter`, `overrides`, non-zero or table `retries`, or a
  different JUnit path anywhere in the parsed nextest config, quoted or inline;
- restating a different command in this table.

The `Required Rust tests` step body itself is executed under
`bash -eo pipefail` with the runners stubbed: a zero-test, leading-zero, junk,
missing, or stale-only report fails a green run, and a failing run keeps its
exit code in the step result and in `run-context.txt`.

These are text- and step-level controls over one workflow file, not a YAML or
shell interpreter. They do not resolve repository-defined Cargo aliases, read
runner-host or caller environment, catch control flow added to the nextest
step beyond the executed scenarios, assert the blob identities that
`run-context.txt` records, or guard a doctest run that selects zero doctests.
The all-feature
Test Analytics replay remains advisory telemetry and does not substitute for
either required gate.

The following current workflow producers are deliberately not ordinary
product-gate rows: advisory reports, uploaded artifacts, standalone PR summaries,
coverage telemetry, release readiness, package listing, publish dry-runs, and
scheduled or release-only qualification. They remain operational or
release-specific evidence until a separate contract promotes them.

## Execution order and diagnostics

Formatting remains before CI-tool installation and hosted-cache restoration.
After setup, repository precommit, promotion honesty, agent-skill, dependency,
process, and network checks run before the later workspace check, Clippy, and
nextest stages. The first xtask invocation still builds xtask and its normal
dependencies; this is not a zero-build preflight. Every command in the inventory
runs once at the workflow level, without suppressing failures. A failing policy
step prevents the later stages from starting rather than spending those stages
before discovering the same policy error.

The required nextest step retains #3852's `rust-tests` ID, explicit `ci` profile,
fresh JUnit/context generation, original exit status, and two-file upload
unchanged. This companion does not replace that evidence implementation or
schedule another test suite.

Each named step exposes its own timing and outcome in the Actions job view and
workflow-jobs API. `Write gate summary` runs with `always()` and records the
workflow commit, subject head, run/attempt, route, and actual outcomes for
the named steps plus PR evidence. A push skips PR evidence by design;
`skipped`, `cancelled`, and missing (`not_reported`) outcomes are never promoted
to success. A terminated or unavailable runner can still prevent any summary
from being written; the summary is not independent execution proof.

`Ripr Rust Small Result` remains the required check. Runner routing,
scratch-tempfail fallback, advisory reporting, artifact selection, and job
budgets are unchanged. The summary does not decide release readiness, establish
cargo-test/nextest equivalence (#3825), or claim a measured speedup.

`xtask/tests/rust_gate_workflow_contract.rs` guards the command inventory,
required-step shape, execution order, and outcome bindings. It includes
negative controls for omitted or duplicated commands, optional or suppressed
tests, late policy checks, failure-only summaries, and fabricated success.
Run it with `cargo test -p xtask --test rust_gate_workflow_contract`.

## Selection boundary

`ProductGatePlan::for_subject` selects applicable rows when selector authority
is present. A missing selector authority or an external-tree trust class
selects the complete route with an explicit reason. It never returns an empty
green plan for an unknown subject. A selected gate claims only the proposition
listed in its definition; its non-claim is explicit and remains separate from
the command that happens to produce it.

This is observational groundwork. The independent required-step inventory is
compared against these rows. The three Rust composites below are now modeled,
but the complete callable-family portfolio is still not represented. Existing workflows remain authoritative. No
`NoOp` row is an allocation or reuse decision, and future migration must
reconcile the complete portfolio and exact subject before rerouting any check.

## Observational Rust composites (#8197)

The three composite descriptors live under the existing `ProductGateDefinition`
authority, in `product_gate_plan/composites.rs`. They are not a second obligation
list or path selector. Ordered commands and artifact edges are parts of one
owning result; a row per shell line cannot stand in for that result. The
`command` field is a command anchor for each composite; its descriptor,
not that anchor alone, carries the exact ordered commands.

- **Base-owned user text:** all three existing steps retain the
  `pull_request || merge_group` guard. The worktree is the checkout's exact first
  parent. The only modeled compiler repair is `test_stub.rs` blob
  `41ce81981802365b31e5182b7a7a51540ff8d7be` to
  `e934320980555de2d431b2aa8482a6059e9cf7f7`, with that path as the sole change.
  The original base matcher runs against the checkout if it supports `--tree`.
  The final existing named step owns the composite result; no Actions step id
  is invented. A legacy matcher-pin-only exit 0 is explicitly weaker and always
  leaves cross-tree execution not established.
- **PR evidence:** all ten generate/check commands remain ordered under
  `pr_evidence`. Merge-group inputs prefer admitted B and G, never constituent
  H as a substitute for G. Retained pull-request callers keep their B-to-H
  comparison. Both review-comment commands consume the same run's freshly
  generated `target/ripr/pr/check.json`. The model records these explicit shell
  edges, not every internal xtask file read or every auxiliary output.
- **Schema sweep:** `schema_producer_sweep` always creates doctor JSON before
  consuming it. PR and merge-group events also consume the same run's PR
  evidence and review-comment JSON, in the existing order and with their exact
  schema bindings. Other events retain doctor-only runtime input.

`queue_workflow::reconciliation` checks supplied composite owner, step/command
order, subject, permitted recovery, and run/attempt/subject artifact bindings.
Missing observations, generic success without composite details, a head-owned
matcher, relabeled legacy fallback, reordered commands, and missing/stale edges
remain findings. Event-excluded execution cannot become success. All reports,
including structurally matching ones, remain `NotEstablished`.

`cargo test -p repo-policy --test rust_composite_contract` binds these descriptors
to the existing workflow and replays its literal Bash on Linux with bounded
Git/Cargo stand-ins. It injects failure at every PR command, exercises both base
matcher branches and recovery refusals, and challenges fresh doctor and
missing/stale PR inputs. These controls do not execute the product commands,
authenticate GitHub data, validate the actual pinned Git blobs independently,
or interpret arbitrary YAML. Structural artifact stamps are not proof of real
producer custody. Existing workflow/runner policy retains its own scope.

No workflow invokes this model or inspector. No job can be pruned, reused or
allocated by them, and no qualification success can be emitted. The old
three-surface applicability model is deliberately conservative for these rows;
it does not establish event-specific non-applicability or finish #8197's
pre-allocation adapter, portfolio parity, or #8198's measured timing acceptance.

## Product-independent policy bootstrap (#7158)

`cargo policy preflight` compiles only the unpublished `repo-policy` package and
its small JSON/hash dependency graph. It runs the existing workflow policy
(including composite actions, assistant-loop corpus, routed workflow and scratch
lease contracts) and agent-skill policy. Both the reusable Rust lane and docs lane
run it before the shared policy composition (`cargo xtask ci-policy` in Rust CI,
`cargo xtask precommit` in the docs lane). Both consume
`RIPR_POLICY_PREFLIGHT_RECEIPT` only after checking its exact input, implementation
and compiler identities. Missing, unreadable or stale receipts fail closed.
Without that environment variable, local precommit retains its workflow check.

The existing `cargo xtask check-workflows` and `check-agent-skills` commands
delegate to the same implementation. `cargo policy check-workflows` and
`cargo policy check-agent-skills` offer individual lightweight commands. Always
use Cargo to rebuild against current source; direct retained executables also
reject a source or compiler identity mismatch. Writable target directories remain
private to their execution owner. This extraction does not make the remaining
precommit table or the full docs lane product-independent. Workspace nextest and
Cargo doctests include the new package in the existing required test lane.

The policy executable anchors identity and policy reads to the nearest repository
workspace when started in a repository or member subdirectory. Cargo selects a
manifest before starting the executable: a separate or malformed nested fixture
package can therefore reject both `cargo policy` and the existing `cargo xtask`
alias before either binary runs. Invoke from the repository root, or use
`cargo run --locked --manifest-path /path/to/repository/Cargo.toml -p repo-policy -- preflight`
with the actual repository manifest when working inside such a fixture. The alias
does not promise to override nested Cargo package selection.


## Hosted policy composition (#7826)

The stable `product.repository.precommit` identity now runs `cargo xtask ci-policy`
through the existing dispatcher and precommit report owner. All 41 predicates in
`PRECOMMIT_GATE_COMMANDS` are retained. The existing nine-check collect-all
producer runs once, before compilation-bearing file-policy enumeration; its
individual reports and errors remain authoritative. Remaining predicates run in
the shared aggregate. Ordinary local precommit retains its original fail-fast
policy order and selected Clippy.

Both compositions validate repository/HEAD/base identity, committed/staged/worktree
NUL change records and rename pairs, untracked inputs, completeness and applicable
Cargo metadata. Both run committed, staged and worktree `git diff --check`.
Rust CI consumes its source/toolchain/input-bound workflow/agent-skill receipt;
wrong, missing, stale or partial proof returns failure before file-policy work.
The CI report explicitly identifies `ci_policy`, records that selected Clippy is
outside its scope, and writes `ci-policy.{json,md}` without replacing the local
`precommit-v2` receipt or legacy `precommit.md` report. Neither report grants authority to skip the later CI lint.

| Predicate/result | Producer | Consumer |
| --- | --- | --- |
| Workflow and agent skills | `cargo policy preflight` | shared policy receipt verifier; `agent_skills` outcome |
| 41 policy predicates and three whitespace checks | shared aggregate plus existing nine-scan producer | `precommit` step outcome; existing leaf reports and CI composition report |
| Evidence promotion honesty | standalone existing command (absent from the table) | `promotion_honesty` outcome |
| Formatting, dependencies, process and network | existing aggregate and retained standalone steps | existing step outcomes and summary |
| Full default-feature workspace/all-target strict Clippy | explicit unchanged `cargo clippy --workspace --all-targets -- -D warnings` | `clippy` outcome |

The four later standalone policy steps were audited: dependency/process/network
are already in the table; evidence-promotion honesty is separate. No standalone
policy execution is removed here, and no duplicate-policy savings is claimed.
The only removed lint phase is the convenience route's impact-selected Clippy.
Tests, doctests, all-features checks, platform routes, event admission, stable
required-check identity, runner configuration, diagnostics and owned process
draining are unchanged. A bounded trace can establish argv/count composition;
only matched native complete-lane measurements can establish runtime savings.
