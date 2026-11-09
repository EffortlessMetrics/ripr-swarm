# Canonical product-gate plan

Issue #3330 establishes the typed authority for ordinary product gates before
any workflow migration. The implementation lives in
`tools/repo-policy/src/product_gate_plan.rs` and is intentionally pure: it describes gate
meaning and applicability without selecting a runner, invoking a provider, or
changing required-check routing.

## Staged admission and result reconciliation (#8197)

`tools/repo-policy/src/product_gate_plan/admission.rs` adds an inactive typed seam for the
13-row ordinary portfolio below. `AdmissionPlan::admit` takes an explicit
request binding and a separately obtained controller observation. It conserves
the canonical definitions, including each command, proposition, role and
non-claim. The immutable plan supplies typed per-gate inputs and reconciles
actual rows against its own denominator.

The binding includes repository/request, head/base/actual integration commit,
ordered parents, tree, complete old/new changed-input objects and modes,
inventory provenance, Cargo/configuration digests, toolchain, platform,
features/default-feature mode, controller/plan/result generations and the
published-evidence inventory provenance. Deleted, renamed and generated inputs
remain in the inventory. The admission sizing count that subtracts generated
files is not a suitable inventory producer.

This is a trusted-controller API, not authentication: its caller must obtain
the observations independently, verify request freshness and authorization,
bind the complete inventory to actual Git objects and authenticate result
provenance. Matching two caller-provided records cannot prove those facts.
The staged callable adapter below adds native observations and wire receipts;
the conservative API alone does not authenticate inputs.

This generation emits only `execute` or `blocked`. It does not promote the
observational plan's surface `NoOp` to `not_applicable`, or compiler caches to
`reuse_verified`. A complete empty diff still executes all 13 gates. Missing or
malformed observations, identity mismatches, unsupported plan generations and
changed/unknown selector-policy transitions block every row before allocation.
The closed disposition type reserves reuse and non-applicability for a later
implementation with compatibility and applicability evidence; there is no
current API that grants either stronger disposition.

Reconciliation rejects missing, duplicate, unexpected, contradictory and
identity-mismatched rows, including a different command/proposition under the
same gate ID. Failed, cancelled, pending, incomplete and skipped outcomes do
not satisfy an obligation. An empty or blocked plan cannot pass. Result
provenance must be present and structurally valid, and its typed evidence kind
must be an executed gate result (compiler/dependency caches and unknown kinds
are refused); authenticating its producer
and bytes remains the controller's responsibility.

This is not full #8197 acceptance. The staged #12973 caller also requires the
Windows subset, Python wheel, CI feature portfolio and printed-command
producers alongside Rust. Those are not silently covered by these 13 rows.
Their complete canonical representation/parity and integration with the wider
caller remain outstanding. The Rust-only projection below does not establish
that wider portfolio, hosted inactive parity or measured allocation reduction.
Existing gates-first ordering and unconditional nextest/doctest proof stay
unchanged. #8198 retains complete execution-graph and 20-minute qualification
acceptance; no timing savings or live activation follow from this seam.

## Inactive callable Rust execution graph (#8197/#8198)

`cargo xtask product-gate-dag` projects the existing Rust producer into
`.github/workflows/rust-gates-dag-branch.yml`. The new
`rust-gates-dag.yml` caller is `workflow_call` only, with no active caller,
development-event subscription or dispatch. It does not replace the installed
gates-first route. Its native-G verifier is the unchanged action from the staged
#12973 chain (commit `05c1266ba7c34d740542e2de50cc67e54720f876`, action blob
`44384c75205029f73bc633d5c3b664854e74a581`). A trusted base that lacks the
accepted controller/action cannot run this path; there is no head-code fallback.
The development base `d808febf4bbd54b628f673814fb5caf2cfbb73c8` has no
`product-gate-dag` command. Neither that base nor the separately preserved
admission-only commit `6df8d393a359657fe51359f1d24b0572d15bc961` is an eligible
controller revision. This implementation must first land through the existing
qualified route; a subsequent trusted base containing its accepted interface
must then be named and verified. The policy-change guard intentionally prevents
this workflow from qualifying its own introduction. Local synthetic CLI
rehearsals use a candidate-built executable and do not establish that bootstrap.

The canonical owner now explicitly accounts for 24 required Rust producer
obligations. Branch placement does not grant applicability exclusions. The
projection preserves actual run blocks and required artifact uploads from
`rust-gates.yml`, and refuses an unknown or duplicate producer step. Scratch
operations, optional/advisory diagnostics and the legacy PR-only best-effort
conflict recheck have explicit omission classifications; they are not silently
treated as product proof. The new native receipt replaces the old summary.

| Branch | Canonical obligations and authoritative producer commands | Required local edge |
| --- | --- | --- |
| `preflight` (admission job) | `cargo fmt --check`; `cargo policy preflight` | both exact successful rows → allocation of all eight expensive branches |
| `policy` | dependency, process and network checks | unchanged standalone checks |
| `base_matcher` | three individually receipted prerequisites (exact first-parent base preparation → pinned compiler-arity recovery → pinned MissedTxt recovery), base-owned `check-user-text --tree` | base checkout/recovery → independent matcher |
| `all_features` | `cargo clippy -p ripr --all-targets --all-features -- -D warnings` | selected source/toolchain/configuration |
| `clippy` | `cargo clippy --workspace --all-targets -- -D warnings` | selected source/toolchain/configuration |
| `doctests` | `cargo test --workspace --doc` | selected source/toolchain/configuration |
| `tests` | `cargo test --no-run --locked --workspace`; complete `cargo xtask ci-policy`; complete unpartitioned `cargo nextest run --workspace --profile ci`, fresh named JUnit and original native-exit/context guard | cheap preflight → workspace preparation → native policy inventory → test execution → retained JUnit/context |
| `corpus` | promotion honesty; goldens; wildcard calibration retention; fixtures; capability producers; capability docs; all verdict corpora | goldens → actual calibration bytes/upload |
| `pr_schema` | all ten produce/check commands for PR evidence, review comments, impacted evidence, summary and annotations; fresh doctor and schema sweep | PR/review outputs → schema sweep live artifact rows |

The three base prerequisites extend the prior 21-row denominator to 24; each
conditional recovery must finish successfully, including its legitimate no-op
case. A missing, skipped or failed recovery cannot reconcile. Their native
commands and pinned recovery checks remain owned by the authoritative workflow.

The three locked fixture fetches remain setup in each branch. Exact-once coverage
here describes top-level obligations, not a claim that existing nested policy
invocations have been deduplicated. Compiler caches
remain execution accelerators, never successful results. The schema and wildcard
consumers stay in their producers' jobs, so they need no new large archive or
cross-job artifact transfer. The generated branch retains the existing native
test denominator; it does not implement the separate prepared-xtask/shard work.

### Callable and receipt interfaces

The caller requires `head-sha`, `base-sha`, `subject-sha`, `subject-tree`,
`subject-json`, `subject-identity`, `workflow-generation`, `request-id`,
`constituent-pr-number` and `constituent-pr-node-id`. The existing native verifier
checks actual G, its tree and ordered parents against the native event and
frozen singleton H/B composition. Every branch rechecks the native subject.
All proof and receipt work uses one run/attempt; older-attempt artifacts are not
accepted by these names or by reconciliation.

`product-gate-dag plan <file>` writes a version-1 plan without allocating work: complete native subject; lossless hex of the full raw NUL-separated Git
changed-input inventory (generated/deleted/renamed inputs are not subtracted);
observed Rust/Cargo and configuration digest; authoritative producer blob; and
canonical rows `{id, branch, step, command, predecessors}`.
`allocate <plan> <preflight-receipt>` emits the closed eight-branch matrix only
after reobserving the plan and validating both exact successful preflight rows.
The admission job projects the unchanged formatting and lightweight policy
commands from the authoritative workflow. It records and retains their receipt;
the expensive matrix depends on that entire job succeeding. Missing, stale,
duplicate, skipped, cancelled or failed preflight cannot allocate work.
Admission also retains the original `target/ripr/reports/policy-preflight.json`
as a separate `rust-dag-policy-preflight-<run>-<attempt>` artifact before
allocation. Tests downloads that exact run/attempt artifact after cache/setup,
then invokes the trusted controller's existing `verify-preflight` before
workspace preparation. The receipt's source, compiler, complete policy-input
identity and exact two-check denominator remain mandatory. `ci-policy` verifies
it again after preparation via the unchanged `RIPR_POLICY_PREFLIGHT_RECEIPT`.
No policy check is repeated and no native receipt is synthesized from job
outcomes. Local independent-directory transfer tests establish native receipt
production and consumption; hosted artifact custody remains unqualified.

The matrix grants execution only. It has no reuse or non-applicability path.

`verify <plan>` reobserves those inputs. `record <plan> <branch> <output>
<start-epoch>` receives native GitHub step outcomes through `RIPR_DAG_STEPS` and
produces `{plan, branch, rows: [{id, step, outcome}], elapsed_seconds}`. It uses
`outcome`, not a potentially overridden `conclusion`, and refuses unexpected
execution from another branch. `finalize <plan> <directory>` requires exactly
nine canonical run/attempt artifact directories (preflight plus eight expensive branches), each containing exactly its
one branch file, and every exact canonical row once with success. Artifact
directories are not flattened, so extra artifacts cannot hide by overwriting
a canonical filename. Missing,
stale, duplicate, extra, cancelled, skipped, failed and substituted results
refuse. The native matrix result must also succeed.

The caller and finalizer compile the controller from the trusted base checkout
into private build state. They do not compile candidate build scripts to decide
admission. Before candidate proof, the route blocks controller/workflow/policy,
Cargo configuration and toolchain transitions instead of trusting the changed
head to exempt itself. The same immutable plan is independently reobserved at
each branch and finalization. This remains a trusted workflow receipt boundary,
not authentication of arbitrary user-provided result JSON.

### Inventory integration and timing limits

The staged default workspace preparation and `ci-policy` consumer now run in the
same `tests` job, checkout, environment and writable Cargo target directory.
Preparation is a separate required row, not successful tests. A failed, missing,
cancelled or skipped preparation cannot reconcile, and normal success conditions
prevent the policy/test consumers from running after producer failure. Formatting
and the product-independent policy preflight finish in admission before any
expensive branch allocates. Admission includes checkout, pinned toolchain and
trusted controller compilation; policy preflight compiles the small policy crate,
not the product. This adds serial cheap-check latency without another job or
controller setup, while preventing wasted parallel work after cheap failure.
Hosted setup and full critical-path timing remain unqualified. The complete
policy composition still precedes test execution; no selected policy or test is
removed. This is inactive ordering only: the installed gates-first route remains
unchanged.

The producer uses the existing full-workspace preparation command already present
in `rust-test-shards.yml`. Native file-policy selectors still ask Cargo to verify
currentness under their exact package, features, target and configuration, then
validate fresh nonempty native listings. This introduces no external inventory
receipt, imported executable, cache-success claim or second selector registry.
Changed source/configuration can force rebuilding; such rebuilding is not a
failure of correctness and must not be reported as reuse.

SCIP library preparation compatibility additionally requires the inventory owner's
selector alignment from candidate `53f01e7693fa557a165ae375b8cb54555ee03533`.
Its exported delta is integrated with prerequisite parent
`7c2c435367c2594ee874052e250166030738b0fb`, including static-first refusal and
final static rescan. The export independently reproduced owner tree
`6b7b0f289748cb74d0c0cd9ec2688bd5b263a47f`; the combined DAG tree is separate.
Package-only xtask preparations remain distinct from the workspace feature graph.
This source establishes ordering and result coverage, not by itself an observed
production-profile compilation reduction. Combined-source proof must retain Cargo
freshness/artifact identities and measure the producer and native consumer;
predecessor debug=0 measurements do not qualify it.


`ci-policy` still owns its entire current obligation, including compilation-
bearing file-policy enumeration. This is not cheap Phase A. The separately owned
per-package inventory adapter is not replaced here, and plain nextest listings
cannot satisfy an inventory result. Its future producer/consumer edge must bind
package, target family, features, toolchain, source/configuration and complete
selected test identity; package-only xtask preparation is not assumed equivalent
to workspace preparation. No inventory reuse is currently granted.

Admission/controller compilation, each branch's setup/compilation and execution,
artifact transfer, finalization and queue delays remain separate costs. Branch
receipts report elapsed time including setup; the finalizer reports both summed
branch seconds (occupancy) and the longest branch, neither of which is complete
request latency. Trusted-controller compilation is presently repeated in each
job and can be expensive; it is not hidden as free planning. The workflow's
20-minute job deadlines do not establish a 20-minute complete route. No hosted
trial is authorized while the route is known to exceed the cap.

This stages parallelism and exact result coverage, not achieved speedup. Native
Windows/wheel/feature/printed-command portfolios, qualified inventory reuse,
prepared test artifacts, full inactive hosted parity and complete <=1,200-second
measurement remain with their existing owners and acceptance. The installed
required checks and production routing are unchanged.

## Current producer inventory

The current producer is `.github/workflows/rust-gates.yml`, called by the
CX43, CPX42, CX53, and GitHub-hosted routes in `routed-rust.yml`. The required
commands have separate named steps with stable IDs; they are not copied into
each runner route. Runner identity and matrix shape are route details, not
product meaning.

| Canonical gate | Current command | Role | Surface |
| --- | --- | --- | --- |
| `product.rust.formatting` | `cargo fmt --check` | required | Rust |
| `product.rust.all_features_check` | `cargo clippy -p ripr --all-targets --all-features -- -D warnings` | required | Rust |
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
| `lang-perl` and other non-default-feature tests | compiled and linted (not run) by `cargo clippy -p ripr --all-targets --all-features -- -D warnings`; not executed in the required lane | all / perl / no-default | advisory Test Analytics, the `Perl and release proof` job, and the Windows advisory feature matrix |

Nextest cannot execute doctests, so a green nextest row never stands in for the
doctest row, and neither row claims non-default-feature subjects. The `ci`
profile pins `retries = 0` and may not declare a `default-filter` or
per-test execution overrides, so every test selected by default is required. Its
one output-only exception retains successful JUnit output for the exact
`xtask::rust_activation` test
`installed_rustup_activation_keeps_the_exact_fixture_compiler` (#13123).
The parsed row must contain only the exact package/binary/test filter and
`junit.store-success-output = true`; changed scope, duplicate rows, extra fields,
and false or non-boolean retention refuse. Test selection, retries, deadlines
and concurrency are unchanged. The
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
- a `default-filter`, any override outside the closed native-activation output
  row, non-zero or table `retries`, or a different JUnit path in the parsed
  nextest config, quoted or inline;
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
product-gate rows: advisory reports, uploaded artifacts, PR summaries,
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
workflow commit, subject head, run/attempt, route, and the actual outcomes for
all twelve product gates plus PR evidence. A push skips PR evidence by design;
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

This is observational groundwork. Existing workflows remain authoritative, and
future migration must compare their selected rows with this plan before any
required check is rerouted.

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

## Product-independent controller bootstrap (#8197)

The canonical owner and its unchanged portfolio tests live in `repo-policy`.
`cargo policy product-gate-dag` and the compatibility `cargo xtask
product-gate-dag` command invoke the same implementation. The inactive workflow
compiles `repo-policy` from the exact trusted base before admission, each branch
and finalization; this excludes product and parser compilation from the controller
bootstrap. Required product compilation and all gate commands remain selected.
The base must first contain this controller through independently qualified
protected landing; the staged route cannot self-qualify that policy change.

The initial route supports repository defaults plus the pinned toolchain action's
ordinary `CARGO_HOME` (`HOME/.cargo`, or `USERPROFILE/.cargo` on Windows) and
`CARGO_INCREMENTAL=0`. The selected home and incremental mode are bound in the
configuration digest. Alternate homes and other incremental values refuse.
Compiler/rustdoc flags, wrappers, profile and target overrides refuse even when
empty; names are checked case-insensitively. Effective Cargo configuration is not
ignored: the root config must match Git, extensionless root configuration and
any ancestor or home config refuse. This deliberately unsupported inherited
configuration requires a separate reviewed contract before use. Job count and private
target-directory placement remain operational inputs. This is no published-result
reuse authority and does not prove complete hosted latency below twenty minutes.

### Lightweight proposition parity command

`cargo policy check-product-route-parity [--corpus <path>] [--json]` runs the
existing ordinary/reusable proposition comparator in `repo-policy`. The xtask
command and fixture-provenance validator delegate to that same implementation.
Standalone default corpus lookup uses the nearest invoking checkout boundary
(Git marker, repo-policy directory, or lockfile plus pinned toolchain); missing or
invalid corpus inputs at that boundary fail without falling back to the build
checkout. The xtask delegate retains its existing source-workspace convention.
Explicit relative corpus paths and `target/ripr/reports/product-route-parity.json` stay relative to
the invoking working directory, as before. No corpus schema or gate selection
semantics change. This compares the ordinary 13-row plan on the committed
corpus; it does not execute either route or qualify the 24-obligation DAG.
