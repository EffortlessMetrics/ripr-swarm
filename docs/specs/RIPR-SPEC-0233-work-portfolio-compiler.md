# RIPR-SPEC-0233: Read-only deterministic multi-campaign work-portfolio compiler

Status: proposed

Owner: test-infra

Created: 2026-10-05

Linked issues:

- #1704 (parent: the portfolio/candidate contract this spec implements)
- #1794 (the execution slice this PR lands)
- #1697 / #1701 (closed prerequisites: authority audit and the removal of
  `active.toml` selection authority — this compiler never reads or
  synthesizes a current/default campaign)
- #1692 (goals-portfolio migration epic)
- #1706 / #1707 (downstream consumers: identity binding and
  portfolio-to-merge proof; designed-for here, implemented there)
- RIPR-SPEC-0218 / RIPR-SPEC-0223 / RIPR-SPEC-0232 (the lifecycle attempt,
  intake and contract/plan contracts whose disposition vocabulary and
  corpus/provenance/digest conventions this compiler reuses; referenced,
  never duplicated)

Support-tier impact:

- None. The compiler is an offline typed projection over committed captured
  inputs. It launches no agent, selects no work, claims nothing, edits
  nothing, calls no provider, reads no live GitHub state at compile time,
  and writes only `target/ripr/reports/work-portfolio.{json,md}` plus
  stdout.

Policy impact:

- None. No new process, network or file-policy surface. The three commands
  register in the command mutability catalog as `report_only`; their
  mutation-negative tests pin the read-only boundary against the captured
  corpus, the fixture tree and source files.

## Problem

After #1701 retired `.ripr/goals/active.toml`, nothing in the repository can
answer, at a point in time and without mutating anything: what work exists
across the durable campaign records, which issues and PRs are in flight,
which candidates are eligible, blocked, stale, duplicated or conflicting,
where lane capacity is available, and what evidence and uncertainty explain
the ordering. Campaign records hold durable intent but cannot know current
GitHub or local state; GitHub alone cannot supply spec/requirement/slice
relationships, semantic conflict resources, local worktree collisions or
proof cost; a hand-maintained dashboard would become another stale
scheduler. #1704 requires a read-only, deterministic, explainable compiler
whose output informs explicit root selection without ever making the
selection.

## Behavior

`cargo xtask work portfolio [--captured <dir>] [--json]`,
`cargo xtask work candidates [--campaign <id>] [--surface <id>] [--limit <n>]
[--json]` and `cargo xtask work explain --candidate <id> [--json]` compile a
versioned `WorkPortfolioSnapshotV1` DTO from the captured-input directory
(default `fixtures/work_portfolio/corpus`) and render it. All three commands
derive from one compilation: `portfolio` renders the whole snapshot,
`candidates` renders the ranked candidate array with optional
campaign/surface filters and a bounded `--limit` (default 10), and `explain`
renders exactly one candidate by stable id. Human Markdown is derived from
the same DTO and can never strengthen readiness or ranking.

### Captured inputs and observation identity

The captured directory holds immutable normalized JSON inputs:
`manifest.json` (repository, default branch and head SHA, checkout root
spelling, per-source observation state), `campaigns.json`, `issues.json`,
`pull_requests.json`, `claims.json`, `local_state.json`, `surfaces.json`
and the optional `cargo_allow.json`. Every source carries an observation
record: source name, `observed | missing | stale | unavailable` state, a
volatile `observed_at` timestamp and a volatile request id. Missing,
stale or unavailable inputs stay visible in the snapshot, degrade the
affected candidates to `partial` or `not_proven` confidence with named
reasons, and never produce fabricated readiness. Live `gh` output never
enters the portable identity; only committed captured bytes do.

### Portfolio and candidate model

`WorkPortfolioSnapshotV1` (`work_portfolio_snapshot.v1`) carries: repository
and default-branch identity; source observations and freshness; campaigns
with stable relationships and no default/current flag; issues with
lifecycle dispositions in the closed RIPR-SPEC-0218 vocabulary; PR states
across open/in-flight/review/CI/merge/reconcile; claims, worktrees,
branches and lane capacity; blocked dependencies and external authorities;
semantic conflict edges; the complete candidate array; selected/omitted/
total counts; partial-data and claim boundaries; and a portable snapshot
identity digest.

`WorkCandidateV1` carries: stable candidate/issue/work-item identity; the
closed candidate kind (`research_issue`, `challenge_contract`,
`compile_plan`, `resume_pr`, `repair_review`, `verify_current_head`,
`merge_ready`, `reconcile_closeout`, `start_build`, `blocked`,
`complete`); campaign and accepted-contract references; lifecycle stage and
exact next durable transition; the open-PR/branch/claim/worktree
relationship; dependencies and blockers; semantic conflict resources and
overlapping work; proof/review/CI cost classes (`not_proven` when
unsourced); lane/capacity state; explicitly sourced readiness contribution
and regression risks; the ordered ranking factors with per-factor
contributions and source references; confidence with named partial-data
reasons; and the recommended packet compiler entrypoint
(`cargo xtask work explain --candidate <id>`). The portfolio preserves the
complete candidate set; ranking is advisory and inspectable.

### Classification, conflict and duplicate behavior

- An issue with an existing open PR or durable exclusive claim for the same
  semantic slice classifies as `resume_pr` / `repair_review` /
  `merge_ready` / `verify_current_head` (in-flight states), never as a
  new-build duplicate. A PR with unresolved review findings classifies as
  `repair_review`; a PR waiting only on checks stays `resume_pr` with the
  exact next transition; a merge-ready PR classifies as `merge_ready`; a
  merged PR whose issue remains open classifies as
  `verify_current_head` / `reconcile_closeout` material.
- Distinct issues resolving the same accepted requirement/delta are grouped
  as a `duplicate_family` conflict edge carrying the shared requirement
  identity and evidence; both candidates stay visible with their distinct
  and overlapping semantic slices named.
- Path-disjoint work sharing an output contract, golden family,
  spec/ledger, workflow, release asset or mutable report/build root is
  `shared_contract` conflict-visible via the surfaces metadata.
- A durable claim collision and an unregistered branch/PR collision each
  produce their own conflict edge, reduce lane parallelism and confidence,
  and never resolve silently.
- An issue blocked on an external dependency classifies as `blocked` with
  the external authority named.
- Candidate classification never closes or supersedes work automatically.

### Ranking policy

Candidates are ordered by the eight explicit factors from #1704, in exactly
that order, each factor inspectable on the candidate with its contribution
and source references: (1) user/root direction and already-owned resumable
work; (2) exact near-merge work with unresolved bounded residue; (3)
structural blockers and dependency fan-out; (4) user-facing
correctness/honesty risk; (5) accepted-contract readiness and evidence
completeness; (6) lane/resource capacity and collision cost; (7)
review/CI/merge cost and current saturation; (8) staleness and observation
confidence. There is no hidden score: the sort is a lexicographic
application of the declared factor sequence, every factor names why it
contributed, and no campaign, label, title keyword, age or release language
receives universal precedence. Factor 1 credits an open PR or a durable
active claim; a merged PR alone is finished work (`verify_current_head` /
`complete` / `reconcile_closeout` material), not resumable work. Readiness/release contribution appears only
when sourced from explicit accepted artifacts, marked advisory, and never
changes the factor order.

### Determinism, budgets and portability

Fixed captured inputs produce byte-stable normalized JSON independent of
input ordering and absolute checkout spelling: every output array is sorted
by stable identity, local paths are relativized against the manifest root,
and volatile timestamps and request ids stay inside source observations
but outside the portable identity digest. The default candidate output is
bounded (`--limit 10`); overflow reports selected/omitted/total counts and
the exact stable retrieval commands (`--campaign`, `--surface`, `--limit`).
Filters narrow the rendered view only — they never change candidate
identity, classification, ranking or authority, and a blocked, complete or
low-ranked campaign never hides unrelated eligible work.

## Required Evidence

- The committed captured corpus `fixtures/work_portfolio/corpus` and the
  variant corpora under `fixtures/work_portfolio/variants/` covering all
  twelve #1704 required fixture scenarios, with `SPEC.md` and a
  digest-bound `provenance.json` in the RIPR-SPEC-0223 convention.
- `cargo test -p xtask work_portfolio -- --nocapture` pinning every
  acceptance box: multi-campaign snapshot, no synthesized default campaign,
  per-candidate explainability (stage, blockers, conflicts, capacity,
  evidence, ranking factors), resume/in-flight instead of duplicate
  candidates, partial/unavailable inputs lowering confidence, filter
  authority-neutrality, mutation-negative compilation, deterministic and
  root-portable output, bounded-wave selection without reading every
  campaign or issue body, and blocked/complete/low-ranked campaigns never
  hiding eligible work.
- `cargo xtask check-fixture-contracts` validating the new corpus shape,
  the twelve scenarios, and the provenance digest bindings fail-closed.
- `cargo xtask check-command-catalog`, `check-goals`,
  `check-local-context`, `check-output-contracts`, `check-pr` and
  `git diff --check` staying green.

## Inputs

- The captured-input directory (`--captured <dir>`, default
  `fixtures/work_portfolio/corpus`): manifest, campaigns, issues, PRs,
  claims, local state, surfaces and the optional cargo-allow graph.
- No live GitHub, branch, worktree, claim, spec, campaign or source state
  is read or written by any portfolio command.

## Outputs

- `target/ripr/reports/work-portfolio.{json,md}` for `work portfolio`.
- `target/ripr/reports/work-candidates.{json,md}` for `work candidates`.
- `target/ripr/reports/work-explain.{json,md}` for `work explain`.
- The same DTOs on stdout; with `--json` only the normalized JSON is
  printed.

## Non-Goals

- No autonomous backlog priority, claim, branch, worktree, PR, merge or
  closeout; no GitHub or local mutation of any kind (pinned by
  mutation-negative tests).
- No hosted portfolio service, watcher, polling daemon or second project
  database.
- No model/provider invocation.
- No opaque business-value score, no hidden ranking weight, no global
  release train.
- No replacement for cargo-allow structure, GitHub live state or local
  worktree truth; captured inputs remain the authority the compiler reads.

## Acceptance Examples

- Two campaigns with eligible issues in different surfaces appear in one
  snapshot with no default campaign required or synthesized; a third
  campaign with no eligible current work stays visible with zero candidates
  and does not hide the others (#1693-style plus a Rust repair campaign
  visible simultaneously).
- An issue whose PR has unresolved review findings classifies as
  `repair_review`; one waiting only on checks stays `resume_pr`; one
  merge-ready classifies as `merge_ready`; each carries its exact next
  durable transition and factor-by-factor ranking explanation.
- A durable claim collision and an unregistered branch collision are
  conflict-visible and lower confidence; a duplicate family names its
  shared requirement and evidence; path-disjoint work sharing an output
  contract is `shared_contract` conflict-visible.
- Missing GitHub data, stale local data and an unavailable cargo-allow
  graph each degrade the affected candidates to `partial`/`not_proven`
  with named reasons while the rest of the snapshot stays complete.
- Compiling the corpus with Windows-style and Unix-style root spellings, or
  with every input array reordered, yields byte-identical normalized JSON
  and the same portable identity.

## Test Mapping

- `xtask/src/work_portfolio.rs::tests::work_portfolio_*` — the full
  acceptance, classification, conflict, ranking, determinism,
  root-portability, reordering, budget and mutation-negative suite
  (test names are recorded in `.ripr/traceability.toml`).

## Implementation Mapping

- `xtask/src/work_portfolio.rs` — captured-input DTOs, fail-closed loaders,
  the pure compiler, classifier, conflict/duplicate edges, ordered ranking
  factors, deterministic renderers and the test suite.
- `xtask/src/command.rs`, `xtask/src/dispatch.rs`, `xtask/src/main.rs` —
  command parsing, catalog registration and dispatch only.
- `xtask/src/fixture_contracts/general_validators.rs` — the committed-corpus
  validator registered in `xtask/src/fixture_contracts/mod.rs`.
- `fixtures/work_portfolio/` — the canonical corpus, the variant corpora,
  `SPEC.md` and `provenance.json`.

## CI Proof

- `cargo test -p xtask work_portfolio -- --nocapture`
- `cargo xtask check-fixture-contracts`
- `cargo xtask check-command-catalog`
- `cargo xtask check-goals`, `check-local-context`, `check-output-contracts`,
  `check-pr`
- `git diff --check`

## Metrics

- `unit_test_pass_rate` over the `work_portfolio` suite.
- Corpus shape invariants enforced by `check-fixture-contracts` (twelve
  scenarios present, digest bindings intact).

## Failure Modes

- A captured file that is absent, malformed, or digest-drifted fails closed:
  the affected source reports `missing`/`unavailable`, affected candidates
  degrade to `partial`/`not_proven`, and the snapshot never fabricates
  readiness.
- An unknown candidate id on `work explain` fails closed with the stable
  candidate id list.
- Reordered or root-respelled inputs that do not produce byte-identical
  normalized JSON are caught by the determinism and root-portability tests.
- Any write outside `target/ripr/reports/` during compilation is caught by
  the mutation-negative tests.
