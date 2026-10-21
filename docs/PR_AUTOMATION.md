# PR Automation Operating Model

`ripr` uses repo automation to shape PRs before human review. The goal is not
more process. The goal is to remove deterministic cleanup from the review path
and turn judgment-required issues into precise repair briefs.

Core rule:

```text
Anything deterministic should be automated.
Anything judgment-based should produce a repair brief.
Generated evidence is not authored copy.
```

Humans and coding agents should spend attention on behavior, evidence,
exceptions, and public contracts. They should not spend attention on formatting,
allowlist order, report directory setup, generated indexes, or gate ordering.

## Changelog inputs

The release cut assembles its `Unreleased` section from the swarm and source
changelogs as a semantic union. A contract-changing PR should normally add a
short changelog fragment under `changelog.d/` (see `changelog.d/README.md`),
folded into `Unreleased` at the release cut. The entry carries an issue or
PR link and states compatibility effects such as a newly refused path or a
changed finding class. The author and reviewer decide whether a change is
notable; a path-only check cannot establish that a user contract changed or
that a useful note was written.

An entry may instead be drafted retrospectively during the release cut. In
that case, the PR should say where the change will be found in the release
inventory and why the entry is deferred. The release checklist requires an
explicit inventory of merged contract changes, including deferred entries,
before the semantic union. `check-pr` does not enforce completeness of
`CHANGELOG.md` or `changelog.d/`; a green check must not be read as a
complete release-note inventory.

Codex Goals consume this harness. The `/goal` loop may advance a multi-PR
campaign, but each work item should still leave the same shaped PR, reports, and
review artifacts described here. Machine-readable receipts record which gates
and report commands ran so agents and reviewers can inspect evidence without
reading raw logs.

## Draft to Ready handoff

Publish a coherent candidate as Draft. The delivery owner completes this order
before spending the `merge-ready`-requested `Ripr Rust Small Result` qualification:

1. Bind the exact candidate head and integration basis. Complete independent
   substantive review through `review-pr`, disposition every known finding, and
   compose accepted repairs into that candidate while Draft.
2. Run applicable, available local proof on the composed head and refresh affected review.
   Follow changed expectations into their consumers: corpus rows can change a
   scoreboard's literal tuple; shared rendering can have a Rust-only mock beyond
   passing Python/card tests. Check the relevant test placement, private imports,
   targets and feature selection. Preserve independent literal oracles and
   meaningful negative controls; derive expected values from the contract, not
   from the implementation being tested. This selects focused proof, not every
   possible test or a duplicate of the hosted matrix.
3. Reconcile actual conflicts, changed prerequisites and affected combined-tree
   proof. Unrelated main movement alone needs no branch update or repeated proof.
   If an applicable required gate has a known deterministic failure on this
   basis, keep Draft, link the existing repair and its owner, and name the event
   that permits resumption. Do not repeat that failure on each dependent PR,
   duplicate its repair, or bless baseline failures. After the prerequisite lands,
   verify the resulting basis and refresh only affected proof and review.
   Before Ready, inspect GitHub's PR merge ref through an available permitted
   Git/API read. Record its commit, parents and tree against the reviewed head,
   basis and material prerequisites; local `merge-tree` proof does not establish
   that remote subject. Judge differences for applicability, without behind-only
   restacking. A stale subject missing a material prerequisite keeps Draft until
   its owner corrects the source ancestry or remote ref state and the changed
   subject is verified. A landing notification alone is not that verification.
   If the ref cannot be queried, retain the exact read limit and leave the hosted
   subject unestablished. An existing authorized, credibly expected-green route
   may obtain that evidence; do not spend known-red qualification to discover
   whether its prerequisite reached the hosted subject.
4. Read back the live PR head, reviews, threads and workflow runs. Mark Ready
   only for the unchanged, reviewed and composed candidate when green is
   reasonably expected, with no unresolved material finding or known applicable
   gate blocker. If local execution is genuinely unavailable, follow the existing
   [local and hosted proof route](agent-context/validation.md#local-and-hosted-proof):
   record the unavailable dimension and reason, completed available checks, and
   existing authorized narrow hosted qualification that will establish it.
   `REVIEW_INCOMPLETE` may retain that evidence gap and merge-ready-requested proof;
   unavailable proof is not a pass. It cannot excuse an executable but unrun
   applicable local check, unfinished review or repair, a known-red test, or
   missing claim-essential native proof. After Ready, the `merge-ready` label (applied last, or removed and re-added for a new head) requests
   qualification for the exact head; Ready alone requests nothing;
   `REVIEW_READY` and required proof still govern merge under `finish-pr`.

After Ready, inspect the actual job checkout commit, parents and tree against
the reviewed candidate and basis. A run's head SHA or a pre-Ready ref observation
does not guarantee the checkout. If a material mismatch appears, retain the
actual run/subject, return to Draft and resume only after the changed prerequisite
or subject is verified; do not relabel that run as proof for the local tree or
re-apply `merge-ready` merely to refresh it.

A new head or material finding returns the candidate to Draft for one joined
repair/proof/review wave before the next `merge-ready` request (remove and re-add the label on the exact new
head). Preserve unaffected evidence. Do not re-apply the label on an unchanged
head with a queued or running required run: it cancels it. Retain its run link and yield the remote wait.

### PR-native continuation record

Keep one current block in the existing PR body or owned issue/PR comment. Edit
it at a transition; link the underlying reviews and reports instead of repeating
their prose. Live GitHub metadata remains authoritative over a stale block.

```text
Owner / governing claim:
Candidate: PR / exact head / reviewed basis and tree / observed GitHub merge subject
Review: exact-head record / remaining findings or evidence
Validation: command + result links / unrun applicable dimensions
Dependency: canonical issue or PR / owner / blocking reason, or none
Next: action / owner / exact resume condition
```

For pending local work include its original task/driver and log paths; a missing
terminal exit is not a pass. When native writes are unavailable, return this
record to the capable delivery owner. A fresh worker reads the native objects
and takes the earliest missing judgment; no separate tracker or idle polling
worker is needed. [Continuation scenarios](agent-context/delivery-continuation-scenarios.md)
exercise both proceed and hold decisions without claiming actual CI or merge.

## Current Commands

The current repo automation surface is:

```bash
cargo xtask actionable-gap-outcomes [--actionable-gaps <path>] [--agent-receipt <path>] [--targeted-test-outcome <path>]
cargo xtask agent-seam-packets [root]
cargo xtask badge-artifacts
cargo xtask badge-basis [--gap-ledger <path>] [--include-seam-classes]
cargo xtask badges
cargo xtask badges --check
cargo xtask branch-inventory [--input <path>] [--dry-run]
cargo xtask branch-inventory apply --plan <path> --digest <digest>
cargo xtask bun-ub-calibration [--corpus <path>] [--out <path>] [--out-md <path>]
cargo xtask bun-ub-preview-summary [--calibration-corpus <path>] [--graph-corpus <path>] [--dogfood-corpus <path>] [--out <path>] [--out-md <path>]
cargo xtask cache gc [--dry-run] [--max-size-gb <n>] [--ttl-days <n>]
cargo xtask cache report
cargo xtask check-allow-attributes
cargo xtask check-architecture
cargo xtask check-badge-diff-policy
cargo xtask check-badge-endpoints
cargo xtask check-behavior-manifest
cargo xtask check-capabilities
cargo xtask check-ci-lane-whitelist
cargo xtask check-command-catalog
cargo xtask check-dependencies
cargo xtask check-doc-artifacts
cargo xtask check-doc-index
cargo xtask check-doc-roles
cargo xtask check-droid-review-config
cargo xtask check-evidence-promotion-honesty [--pinned-external] [--clone] [--case <id>] [--checkout-root <path>] [--timeout-secs <n>]
cargo xtask check-executable-files
cargo xtask check-file-policy
cargo xtask check-covered-by
cargo xtask check-fixture-contracts
cargo xtask check-generated
cargo xtask check-generated-clean
cargo xtask check-lint-policy
cargo xtask check-local-context
cargo xtask check-network-policy
cargo xtask check-no-panic-family [--propose]
cargo xtask check-output-contracts
cargo xtask check-identity-registry
cargo xtask check-positioning-language
cargo xtask check-pr
cargo xtask check-process-policy
cargo xtask check-product-copy
cargo xtask check-proof-packs
cargo xtask check-pr-shape
cargo xtask check-public-api
cargo xtask check-readme-state
cargo xtask check-spec-format
cargo xtask check-spec-ids
cargo xtask check-spec-numbering
cargo xtask check-static-language
cargo xtask check-supply-chain
cargo xtask check-support-tiers
cargo xtask check-test-oracles
cargo xtask check-traceability
cargo xtask check-verification-contracts [--check]
cargo xtask check-workflows
cargo xtask check-workspace-shape
cargo xtask ci-budget [--workflow <name>] [--limit <n>] [--input <path>]
cargo xtask ci-fast
cargo xtask ci-full
cargo xtask cockpit
cargo xtask commands
cargo xtask configured-bridge-inventory [--graph-corpus <path>] [--out <path>] [--out-md <path>]
cargo xtask critic
cargo xtask doctor
cargo xtask dogfood
cargo xtask evidence-health
cargo xtask evidence-quality-audit
cargo xtask evidence-quality-scorecard
cargo xtask evidence-quality-trend [--current <path>] [--previous <path>]
cargo xtask first-pr [--root <path>] [--base <rev>] [--head <rev>] [--gap-ledger <path>] [--out-dir <path>] [--check]
cargo xtask fix-pr
cargo xtask fixtures [name]
cargo xtask gh-pr-status --pr <number>
cargo xtask golden-drift
cargo xtask goldens bless <name> --reason <reason>
cargo xtask goldens check
cargo xtask impacted-evidence [--pr-evidence <path>] [--label <label>] [--labels <csv>] [--check]
cargo xtask install-hooks
cargo xtask issue-intake --issue <number>
cargo xtask lane1-evidence-audit
cargo xtask lsp-cockpit-report
cargo xtask lsp-performance-report
cargo xtask markdown-links
cargo xtask merge-queue capture [--repo <owner/name>] [--out <dir>] [--input <path>] [--prior <path>]
cargo xtask metrics
cargo xtask module-health [--threshold <n>]
cargo xtask mutation-calibration [root] --mutants-json <path>
cargo xtask operator-cockpit
cargo xtask operator-cockpit-report
cargo xtask package
cargo xtask precommit
cargo xtask proof preflight [--base <rev>] [--head <rev>]
cargo xtask proof route [--base <rev>] [--head <rev>]
cargo xtask pr-ready
cargo xtask pr-summary
cargo xtask pr-triage-report
cargo xtask publish-dry-run
cargo xtask receipts [check]
cargo xtask recommendation-calibration [--root <path>] [--pr-guidance <path>] [--outcome-receipts <path>] [--out <path>]
cargo xtask release-readiness --version <version>
cargo xtask release-server-archive --version <version> --target <triple> --executable <name> --archive <zip\|tar.gz>
cargo xtask release-server-manifest --version <version> --repository <owner/repo>
cargo xtask release-upload-assets --version <version>
cargo xtask repo-badge-artifacts [--gap-ledger <path>]
cargo xtask repo-contract-report
cargo xtask repo-exposure-latency-report
cargo xtask repo-exposure-report
cargo xtask repo-exposure-summary-report
cargo xtask reports index
cargo xtask repo-seam-inventory
cargo xtask ripr-annotations [--comments <path>] [--out <path>] [--check]
cargo xtask ripr-plus [--gap-ledger <path>] [--repo-exposure-summary <path>]
cargo xtask ripr-pr [--base <rev>] [--head <rev>] [--root <path>] [--check]
cargo xtask ripr-pr-summary [--check]
cargo xtask ripr-review-comments [--base <rev>] [--head <rev>] [--root <path>] [--check]
cargo xtask ripr-swarm attempt-ledger [--swarm-plan <path>] [--actionable-gap-outcomes <path>] [--previous-ledger <path>] [--real-repair-attempts <path>]
cargo xtask ripr-swarm attempt --packet <id> --dry-run [--actionable-gaps <path>]
cargo xtask ripr-swarm plan [--top <n>] [--actionable-gaps <path>]
cargo xtask ripr-swarm readiness [--swarm-plan <path>] [--actionable-gap-outcomes <path>] [--attempt-ledger <path>]
cargo xtask route-quality [--attempt-ledger <path>]
cargo xtask rust-conversion-candidates
cargo xtask rust-repair-trust-report
cargo xtask sarif-policy --current <path> [--baseline <path>]
cargo xtask schema-producer-sweep [--rev REV] [--artifact SCHEMA[#POINTER]=FILE[#POINTER]]...
cargo xtask shape
cargo xtask specs next
cargo xtask suggested-fixes
cargo xtask targeted-rerun-benchmark --root <path> --changed-test <path> [--samples <n>] [--timeout-ms <n>]
cargo xtask targeted-test-outcome --before <path> --after <path>
cargo xtask test-efficiency-report
cargo xtask test-oracle-report
cargo xtask update-badge-endpoints
cargo xtask vscode-compile
cargo xtask vscode-package
cargo xtask vscode-test
cargo xtask vscode-test-e2e
cargo xtask worktree doctor
```

This list mirrors the authoritative catalog; run `cargo xtask commands` to
regenerate the live mutability catalog (`target/ripr/reports/commands.md`)
before relying on it.

`shape` is the safe local normalizer. It can mutate local files only when the
mutation is deterministic and reversible by normal version control review.

Current `shape` responsibilities:

- run `cargo fmt`
- sort `.ripr/*.txt` and `policy/*.txt` allowlist files
- ensure `target/ripr/reports`
- write `target/ripr/reports/shape.md`

`fix-pr` is the contributor and agent entrypoint for safe repair. It runs
`shape`, refreshes the PR summary, and writes
`target/ripr/reports/fix-pr.md`.

`commands` writes `target/ripr/reports/commands.md` and
`target/ripr/reports/commands.json`. The catalog classifies xtask commands as
`mutating`, `non_mutating_check`, `report_only`, `external_state_read`,
`external_state_mutating`, or `argument_dependent`, and flags commands that
require judgment before use.

`check-command-catalog` writes `target/ripr/reports/command-catalog.md` and
fails when the help catalog and mutability catalog drift apart, when a command
uses an unknown mutability class, when mutating commands omit their write
surface, when external-state mutations are not judgment-required, or when an
argument-dependent command does not explain when it writes.

`pr-summary` writes `target/ripr/reports/pr-summary.md` from git diff and git
status. It classifies changed paths into production, evidence, docs, policy,
workflow, extension, and public-contract surfaces. The first section is an
advisory actionable repair front panel: when existing
`target/ripr/reports/actionable-gaps.json`, `actionable-gap-outcomes.json`, or
`pr-review-front-panel.json` artifacts are present, it names repo and PR-local
actionable counts, receipt movement state, static-limited counts, and one top
next repair packet before raw path inventory. When the actionable artifact
contains an eligible Python preview packet, the same front panel also names the
top Python repair card with its canonical gap, changed owner, missing
discriminator, suggested test target, verify command, receipt command, stop
conditions, and preview/advisory boundary. Missing artifacts stay visible as
regeneration guidance; the summary does not infer analyzer truth, gate status,
runtime proof, mutation proof, source edits, or generated tests.

Top-level `plans/` files are documentation evidence and campaign-planning
inputs. They appear in the docs, evidence/support, and campaign-planning
sections of the reviewer packet without being treated as production behavior.

`cargo xtask ci-policy` is the hosted Rust policy entry. It uses the same
change-set discovery, metadata validation, policy table and three whitespace
checks as local precommit, with the existing nine-scan collect-all producer
before compilation-bearing file-policy enumeration. It writes
`target/ripr/reports/ci-policy.{json,md}` through the precommit report owner,
identifies its composition, and leaves legacy `precommit.md` and structured
local `precommit-v2` receipts untouched.
Its pass excludes selected Clippy; the required producer retains explicit
`cargo clippy --workspace --all-targets -- -D warnings`. The source/toolchain/input
policy-preflight receipt is still verified. Formatting and the later dependency,
process and network steps are retained; no duplicate-policy removal or measured
savings is claimed. Ordinary local `precommit` keeps selected Clippy.

`precommit` is the cheap non-mutating guardrail. It checks formatting, spec
format and numbering, and the policy surfaces that should fail quickly before
review.

Local `precommit` also runs `goldens check` when the diff against
`origin/main` touches output-surface code (`crates/ripr/src/output/`,
`crates/ripr/src/domain/`, `lsp.rs`, `docs/OUTPUT_SCHEMA.md`,
`policy/output_contracts.txt`) or a fixture `expected/` file, because renderer
wording changes make stale goldens the usual way main goes red (#8421). Other
diffs print a skip line and do not pay for the fixture run. Real drift fails
`precommit`. If fixtures only fail because their locked dependencies cannot be
prepared in the environment, `precommit` prints `goldens check: INCOMPLETE
(environment gap, not a pass)` and continues; run `cargo xtask goldens prepare`
to close the gap, and hosted CI still owns those fixtures. The step is not part
of the hosted collect-all policy run, which has its own goldens job.

`cargo xtask check-fast` also runs `check-support-tiers` for committed spec
Markdown, the support map, root README, Rust repair trust corpus, xtask source,
and referenced workflow/script path changes. It runs before compilation-bearing
Rust checks and Clippy, reuses the existing leaf diagnostics, and records actual
execution or non-applicability in its report. See
[RIPR-SPEC-0060](specs/RIPR-SPEC-0060-source-of-truth-stack.md#early-local-support-tier-validation).
The established selector remains `origin/main...HEAD`; while authoring
uncommitted or untracked specs, run `cargo xtask check-support-tiers` explicitly.
This is one applicable local scan; precommit and hosted routing are unchanged.

`check-fast` runs the existing exact help/parser parity library test when the
committed diff touches Rust source in the command-parser owners `cli`, `app`,
`mcp`, or `output/first_pr` (module files and directories). This catches parser
extractions whose help-owner mapping was left behind, including parser-only
edits. The validator's registration, resolution and bidirectional flag checks
remain unchanged. Native failure or zero/ignored/missing test execution fails
the gate; unrelated analyzer, renderer and documentation paths skip it. The
report and strict selector receipt retain the `cli-help-parity` obligation.
Cold library-test compilation is additional work; the single selected case
does not claim full CLI/runtime or hosted qualification. Uncommitted edits keep
the existing selector semantics; run the focused test directly while authoring.

Local command diagnostics reuse the existing reports: `precommit-v2.{json,md}`
records diff-check and selected Clippy children, `file-policy.md` records actual
covered-by warmup/list children, and `check-fast.md` records fmt, selected CLI
help parity and workspace Clippy children. Other commands are outside this
bounded capture scope. Reused
selectors and skipped warmups create no execution rows. Precommit diagnostics
remain bound by its report digest; they add no acceptance or waiver authority.
The nullable numeric `exit_code` comes directly from the observed native status;
missing status, capture/status-call errors and Unix signals keep it null. It is
independent of `outcome`: a timed-out call stays `timed_out` even if its observed
exit code is zero. Human diagnostics render a missing code as `unavailable`.
No environment values or new artifact locations are captured.
Structured argv preserves argument boundaries (`$REPO` substitutes the repository
root in precommit). Captured elapsed time runs from runner start through observed
exit, with polling. Enforced timeout duration includes termination and reaping,
and Unix process-group confirmation where applicable; its distinct scope records
this cleanup. Both captured scopes exclude pipe drain. Inherited-status elapsed
includes the launch/status call and preserves inherited streams. Missing values
are unavailable evidence. Cargo counts normalize existing ANSI color sequences
and recognize `Compiling`, `Checking` and `Fresh` stderr progress lines only; they do not identify compiler units, prove
artifact reuse or by themselves justify changing Clippy composition. Dirty-source HEAD,
base and paths supply invocation context rather than immutable source identity.
Successful report-write failures still return an I/O error; interrupted execution
retains its original error and separately reports publication failures.

`doctor` is the shortest agent hygiene entry point and is equivalent to
`worktree doctor`. It reports dirty `main` worktrees, branches behind
`origin/main`, generated badge/target residue, and broad source-of-truth diffs
that lack an obvious work item marker. Its report also includes a short
next-action queue so agents can move from diagnosis to the right cleanup or
follow-up validation command without reverse-engineering the findings.

### Bounded local storage and proof retention

The root's existing task/resource ownership carries storage admission and closeout;
Git and GitHub carry source/PR state. Do not add a registry, daemon or scheduler.
`worktree doctor` inspects repository hygiene; it does not measure host capacity,
prove consumer inactivity or authorize deletion. A successful build is not a
storage closeout.

Before allocating or generating, record in the existing task handoff:

- exact worktree, Git common directory, effective Cargo target/build paths,
  temporary output and frozen-proof destinations, with their current owners;
- mandatory current/witness artifacts: the qualified executable and each distinct
  negative or prior-candidate witness still needed to reproduce the claim,
  identified by source SHA, artifact hash and the receipt that consumes it;
- reusable mutable build state, and disposable or superseded outputs eligible for
  the already-authorized recoverable closeout after their consumers release them;
- observed free space, the host's accepted reserve and the conservative
  **peak additional bytes** for compilation, downloads, snapshots, archive creation
  and full restoration verification on every affected volume. Do not count prospective
  reclamation before it is verified, assume a compression ratio, or lower a floor
  merely to admit another run. Stop generation when that budget no longer fits.

Use an existing immutable artifact reference for repeated proof consumers when
its hash and source identity match. Do not freeze another expanded executable for
unchanged bytes merely because a new report or stage has a different name. Do not
hard-link proof into mutable Cargo output; later writes could alter the witness.
Different bytes or semantics can require separate witnesses. A hash, source/version stamp, Cargo freshness claim, successful rebuild or green
test alone does not establish the actual executable's dependency/source identity
or replace a required original witness. Bind qualification to the explicitly
selected compiler artifact and observed behavior under its admitted ownership.

The qualification-only #4510 installed-candidate harness retains the actual
Cargo-install executable message and observed toolchain. Installation uses the
same absolute PATH-selected rustc invocation entry for version observation and
compilation, with compiler selectors pinned and wrappers disabled in its owned
command/configuration. Relative PATH entries resolve from the extracted working
directory; ambiguous drive-relative entries refuse. This preserves a rustup
proxy's rustc basename and observes versions without authenticating SDK bytes.
The harness checks compiler and installed bytes agree and captures those retained
bytes in an exclusive slot
before qualified consumption. Reuse the same captured handle; another capture
must refuse an occupied slot even if its bytes match. Failed captures publish no
handle and remove only an observed owned partial; where native file ownership
cannot be established they name and retain it. The qualification producer-root
guard also names and retains its whole root after capture failure for verification;
successful capture restores normal cleanup. These unlocked checks do not
qualify a hostile filesystem race, crash durability, general test-artifact reuse,
or the final installed release candidate.

Before the next batch, and at merge, closure, cancellation or handoff, reconcile
resources with the actual writer and all queued, running and independent consumers.
Classify every resource as `released`, `retained with reason`, or
`awaiting owner verification` in the existing terminal return. Missing process/PID
observations, old mtimes, an unlocked directory, a clean source tree or a merged PR are not
release evidence. Preserve open-claim witnesses and unknown resources. Bound a
retention reason to the real consumer or proof need, rather than keeping every
superseded runner expanded by default.

For confirmed inactive resources within existing cleanup authority:

1. Re-read source status, ignored/untracked evidence, branch/HEAD/upstream,
   detached or unpushed work, Git registrations and native ownership immediately
   before acting. A primary checkout may contain a common Git store used by active
   linked worktrees. Do not move that store or infer removability from its branch.
2. Preserve required source, unique work, receipts and restore paths. Use ordinary
   Git worktree operations for linked trees, and the existing lock/owner protocol
   for generated state; never repurpose a live target to make room.
3. For compressed quarantine, retain a manifest with original paths, source and
   artifact identities, required metadata and restore instructions. Admit both
   the archive and expanded verification copy while originals still exist,
   restore into a separate staging path, and compare the restored bytes/metadata
   before replacing the expanded originals with the recoverable archive. Retain
   originals if any check fails. A same-filesystem rename alone reclaims no bytes.
4. Recheck excluded resources and actual free-space gain. Return the archive,
   verification result and exact restore path in the existing terminal report.
   Do not permanently purge the archive without the required separate authority.

These are root-owned checks using existing tooling, not new command-enforced
budgets or automatic deletion. Routine recoverable closeout already covered by
user/host authority should execute within that scope; ask only for a genuinely
missing permission or an unresolved ownership/data boundary.

`pr-triage-report` is the open-board hygiene report. It reads open PR metadata
through GitHub CLI and writes `target/ripr/reports/pr-triage.md` plus
`target/ripr/reports/pr-triage.json`. It flags same-title families, identical
changed-file sets, stale drafts, branches behind main, incomplete validation
signals, and policy/gate/generated workflow surfaces. It also emits an
advisory queue disposition for each PR — `merge_candidate`, `needs_integration_review`,
`needs_review`, `close_duplicate`, `superseded`, `needs_fresh_validation`,
`needs_owner_decision`, or `do_not_touch_wrong_lane`. It is advisory and never
updates, closes, merges, or comments on PRs. The behind finding and disposition
preserve the observation and unaffected proof; they request material integration
judgment rather than a branch update. Existing duplicate, policy, validation and
review routes precede that disposition.

`branch-inventory` is the remote-branch ownership/age report (#2024). It
regenerates the inventory from current GitHub/Git data and writes
`target/ripr/reports/branch-inventory.md`, `branch-inventory.json`, the raw
capture `branch-inventory-input.json`, and a separate digest-bound deletion
plan `branch-inventory-plan.json`. Classification goes through the all-state
PR lookup by head branch name with full pagination — never Git ancestry,
because squash merges leave merged branch SHAs unreachable from `main`.
Open PR heads, protected/authority branches (`main`, `freeze/*`, `release/*`),
and `#2022` claims are structurally excluded; unknown always classifies
`manual-review`, and only merged-PR leftovers whose head SHA still matches the
merged PR head are `delete-candidate`. The default mode is read-only and never
deletes anything. Deletion is a separate explicit operator action,
`cargo xtask branch-inventory apply --plan <path> --digest <digest>`: it
refuses a regenerated or changed plan, rechecks open PR heads, protection, and
branch SHAs immediately before each deletion, uses non-force ref deletion
bound to the rechecked SHA (`--force-with-lease=<ref>:<sha>`, never plain
`--force`), refuses to run under CI, and writes one cleanup receipt
(`branch-inventory-cleanup.{md,json}`) recording every deleted, skipped,
changed, and failed branch. No CI or scheduled job runs the apply path.

`gh-pr-status --pr <number>` is the per-PR merge-readiness packet.
Each full response uses the `gh 2.46` compatible `pr view` fields, bracketed by
`gh api graphql` reads of PR/head/base/target identity. The bracket identities
must agree and the full response must match their PR/head/target before its
base OID is attached. An independently moving branch-tip lookup cannot supply
that identity. GraphQL errors/partial or missing identity refuse collection.
Set `RIPR_TEST_GH` to a real supported CLI binary and run
`cargo test -p xtask --bin xtask gh_pr_status_real_cli_schema -- --ignored`
to check the production view fields against its actual offline field registry;
this opt-in capability test needs the tool but no credentials or network.
It reads the full PR snapshot again after required-context lookup. Both reads
must identify the requested PR and the same full head/base commits and target
branch. Movement or unavailable identity returns an error before publishing a
new readiness packet; existing report files remain historical and must not be
used as a successful result of that invocation. Head movement requires affected
head-bound proof/review and integration judgment; base-only movement preserves
unchanged-head proof/review and requires integration judgment, not blanket
reruns. When identities match, the final response supplies checks and review
facts. The warning list records the exact observed commits. This separate read
is not atomic with a later merge and supplies no merge authority. It reads
GitHub CLI PR status, branch-protection required contexts when available,
latest reviews, and Droid-related checks, then writes
`target/ripr/reports/gh-pr-status.md` and
`target/ripr/reports/gh-pr-status.json` with the merge state, outstanding
checks, failed checks, behind-main state, review status, Droid status, and a
safe next action: `wait`, `inspect integration`, `inspect failure`, or `merge`.
`BEHIND` requests integration judgment after Draft, review-required and pending
waits; it does not establish that a branch update is needed. The packet retains the current head commit and each latest review's original
commit, with `current_head`, `stale_for_head`, or `subject_unavailable` attribution.
An `APPROVED` aggregate without an exact-head latest approval recommends `wait`.
The CLI's `latestReviews` summaries are joined by author, submission time, and
state to the full `reviews` subjects; missing or ambiguous identity stays unknown.
This attribution does not establish review substance, independence, approval
quotas, integration qualification, or protection satisfaction. It is advisory and
never updates the branch, comments, approves, or merges.
The provider-response section retains known quota notices and unknown substance
from returned reviews/comments without treating either as review completion.
Historical response subjects remain unattributed; the packet head is collection
context only. This does not add a provider gate or change the advisory action.
See [provider observation fields](OUTPUT_SCHEMA.md#github-pr-status-provider-observations).
For unresolved inline conversations and their next actions, use the read-only
[finish-pr conversation inventory](../.agents/skills/finish-pr/references/review-conversations.md).
It paginates threads and keeps outdated unresolved findings visible; it does not
replace substantive review or change merge protections.
Use [Merge freshness and watcher policy](MERGE_WATCH_POLICY.md) for polling
cadence, branch-refresh decisions, REST status fallback, Droid/advisory-check
handling, and local worktree merge limitations.

`merge-queue capture` is the MQ0 read-only current-state receipt. It writes
desired settings, live observation, apply-route capability, and rollback
identity as separate facts under `--out` (default
`target/ripr/reports/merge-queue`). Tests use `--input`; live mode uses GET-only
`gh api` calls. It never applies settings or enables a queue.

`suggested-fixes` writes `target/ripr/reports/suggested-fixes.patch` and
`target/ripr/reports/suggested-fixes.md` with safe deterministic repair
suggestions. The patch covers allowlist ordering under `.ripr/*.txt` and
`policy/*.txt`, docs index table ordering for specs and ADRs, and traceability
behavior block ordering by spec ID, plus capability block ordering by spec ID
and capability ID, and command catalog ordering by xtask help order. It never
generates badge endpoint values,
golden blessings, baselines, suppressions, dependency exceptions, or
schema-version changes.
The generated-vs-authored boundary is documented in
[Generated evidence discipline](GENERATED_EVIDENCE.md).

`check-pr` is the review-ready local gate. It runs the current fast CI lane,
then clippy, docs, and PR summary generation. It intentionally leaves
release/package verification to `ci-full` or release-specific workflows.
Its fast policy lane includes `check-badge-diff-policy`, which rejects
generated badge endpoint diffs in ordinary PRs, and `check-generated-clean`,
which rejects generated target/sample build residue. Before writing the final
report index, it also refreshes the deterministic suggested-fixes patch under
`target/ripr/reports/`.

`repo-exposure-summary-report` is the ordinary repo-local summary route. It
writes `target/ripr/reports/repo-exposure-summary.json` from
`repo-exposure-summary-json` and avoids the per-seam evidence payloads carried
by full `repo-exposure-json`. The command is bounded by
`RIPR_REPO_EXPOSURE_SUMMARY_TIMEOUT_MS` (default: 240000). On timeout or
incomplete output, it overwrites stale summary JSON with a warning artifact whose
`runtime_status.downstream_consumable` is `false` and whose `metrics` object does
not claim a gap count. `cargo xtask ripr-plus --repo-exposure-summary
target/ripr/reports/repo-exposure-summary.json` may reuse this artifact only
when it is downstream-consumable; timeout and limited artifacts fail
deliberately. Use `repo-exposure-report` only when an operator explicitly needs
the full classified seam inventory for deep inspection.

`fixtures` validates fixture contract shape, runs `ripr check` for fixture
directories when they exist, writes actual outputs under
`target/ripr/fixtures/<name>/`, compares stable expected outputs, and writes
`target/ripr/reports/fixtures.md`. It passes with a clear report when no
fixture directories exist yet.

`goldens check` runs fixtures and fails on drift between actual and expected
outputs without mutating checked-in files. It also writes
`target/ripr/reports/golden-drift.md` and
`target/ripr/reports/golden-drift.json` so reviewers can inspect semantic drift
before any blessing. `goldens bless <fixture> --reason <reason>` requires the
reason to cite an existing `RIPR-SPEC-NNNN` under `docs/specs/`, then records
the explicit blessing reason, updates expected JSON and human outputs, and
appends the fixture expected-output changelog.

`golden-drift` writes the same advisory drift reports without failing merely
because output drift exists. It still reports fixture execution errors as
command failures.

`test-oracle-report` writes an advisory baseline of `ripr`'s own Rust test
oracle strength to `target/ripr/reports/test-oracles.md` and
`target/ripr/reports/test-oracles.json`. `check-test-oracles` is currently an
alias that produces the same non-blocking report.

`test-efficiency-report` writes an advisory per-test evidence ledger to
`target/ripr/reports/test-efficiency.md` and
`target/ripr/reports/test-efficiency.json`. It records apparent owner calls,
oracle kind and strength, observed literal values, static limitations, and
advisory reason counts for low-discriminator signals. The report is a review
aid and does not block CI.

`dogfood` runs `ripr check --mode fast` against stable in-repo fixture diffs,
writes actual outputs under `target/ripr/dogfood/`, and writes advisory
Markdown and JSON reports under `target/ripr/reports/`. It also dogfoods gate
adoption by running `ripr gate evaluate` over checked boundary-gap PR guidance
and calibration evidence for `visible-only`, `acknowledgeable`,
`baseline-check`, and `calibrated-gate` modes. Those gate adoption receipts
are compared against `fixtures/boundary_gap/expected/gate-adoption/` and
written under `target/ripr/dogfood/gate-adoption/`; the dogfood report records
that default generated CI still does not block unless `RIPR_GATE_MODE` is
explicitly configured. It also checks first useful action receipts from
`fixtures/boundary_gap/expected/first-useful-action/` for actionable,
baseline-only, stale, missing-required-artifact, unchanged-after-attempt, and
no-actionable-seam routes.

`lsp-cockpit-report` reads committed LSP fixture expectations plus the VS Code
e2e smoke test file and writes `target/ripr/reports/lsp-cockpit.md` and
`target/ripr/reports/lsp-cockpit.json`. It summarizes which fixtures produce
editor diagnostics, which code actions are exposed, which context/action fields
are present, and which VS Code commands are covered by e2e tests.

`lsp-performance-report` runs the saved-edit sequence harness from
`crates/ripr/src/lsp/saved_edit_sequence.rs`, overlays source/binary identity,
and writes `target/ripr/reports/lsp-performance.md` and
`target/ripr/reports/lsp-performance.json`. It records scheduler, delivery, and
cache-load work counts for cold start through explicit full refresh. Historical
2s/10s/30s envelopes remain proposals. Existing rust tests absorb the
deterministic sequence; the command is not a CI gate and does not add a
full-workspace job.

`repo-exposure-latency-report` builds the local debug `ripr` binary, runs
repo-exposure formats under a bounded timeout, captures opt-in analyzer phase
trace lines, and writes `target/ripr/reports/repo-exposure-latency.md` and
`target/ripr/reports/repo-exposure-latency.json`. It is a diagnostic report for
cache and warm-path work, including file-fact cache hit/miss counters; it does
not change repo-exposure JSON/Markdown.

`release-readiness --version <version>` writes
`target/ripr/reports/release-readiness.md` and
`target/ripr/reports/release-readiness.json`. It packages and extracts the
local `ripr` crate, installs from that extracted package, checks the installed
binary identity and an external `doctor` fixture, then checks that `pilot`, `outcome`, `calibrate cargo-mutants`, and
`agent verify`/`agent receipt` are exposed, runs the boundary-gap
pilot/outcome/agent-verify/agent-receipt fixtures, refreshes repo-exposure
latency and LSP cockpit reports, inspects the advisory GitHub workflow dry-run,
and checks VSIX and known-limit docs. Package list and publish dry-run checks
record `not_run` until the requested version matches `crates/ripr` and the tree
is clean, so release prep can rerun them on the version-bump branch.

`targeted-test-outcome` compares two `repo-exposure-json` artifacts and writes
`target/ripr/reports/targeted-test-outcome.md` and
`target/ripr/reports/targeted-test-outcome.json`. It matches seams by
`seam_id`, summarizes before/after grip-class counts, and reports moved,
unchanged, new, removed, and regressed seams as an advisory targeted-test
receipt. The receipt includes a reviewer-native review receipt that says what
changed, what RIPR flagged before, which focused proof signals moved outside
RIPR, what remains weak or unknown, and what reviewers should inspect or avoid
inferring. It does not run mutation testing, edit source, generate tests, claim
coverage adequacy or merge approval, or block CI.

The installed CLI exposes the same receipt loop as `ripr outcome --before
<path> --after <path>` so users do not need this repository checked out. The
xtask command remains the repo-local report writer for automation packets under
`target/ripr/reports/`.

`critic` writes an advisory adversarial review packet to
`target/ripr/reports/critic.md` and `target/ripr/reports/critic.json`. It reads
the current diff plus generated reports and receipts, then flags likely missing
evidence such as analyzer changes without fixture/golden evidence, output
changes without output-contract evidence, campaign movement without campaign
reports, fixture output drift without blessing reasons, policy changes without
process docs, and extension changes that still need npm compile/package proof.
It does not fail CI.

`reports index` writes `target/ripr/reports/index.md` and
`target/ripr/reports/index.json` as a reviewer front door. It summarizes the
active campaign, available reports, missing expected reports for the changed
surface, advisory reports, and suggested next commands. The index also carries
repo-ops packet status for command mutability, the repo cockpit, PR-ready,
worktree doctor, PR triage, per-PR merge readiness, generated-clean, badge diff
policy, critic, receipts, suggested fixes, and `check-pr` artifacts so agents
can consume the operating packet as JSON instead of scraping prose. The command
catalog check packet is included next to the catalog itself so catalog drift is
visible in the same front-door index.

The index also carries a Lane 1 Evidence Readiness section for the report chain
that supports actionable canonical-gap counts and badge-readiness decisions:
`evidence-health`, `lane1-evidence-audit`, `actionable-gaps`,
`evidence-quality-scorecard`, `evidence-quality-trend`, and `badge-basis`.
Missing, warning, or failing artifacts keep the index in a warning state and
add the relevant regeneration command. The index only checks existing artifact
paths; it does not run those expensive reports or infer evidence from source.

`cockpit` writes `target/ripr/reports/cockpit.md` and
`target/ripr/reports/cockpit.json`. It is the repo-level maintainer front door:
it composes worktree doctor, command mutability, command-catalog coverage, spec
numbering, campaign/source-of-truth checks, open PR triage, generated-clean, and
badge diff policy into one advisory action queue. It reads GitHub PR metadata
through `pr-triage-report`, writes local report packets, and does not close
PRs, update branches, edit badge endpoint JSON, mutate source, or change
policy authority.

`pr-ready` writes `target/ripr/reports/pr-ready.md` and
`target/ripr/reports/pr-ready.json`. It composes the local repo-ops checks that
an agent should run before opening or updating a PR: worktree doctor, command
mutability catalog, PR summary, critic, receipts check, suggested fixes,
generated-clean, and badge diff policy. The command is advisory front-door
metadata; it does not replace `check-pr`.

The CLI front doors use the same start-here wording. `safe next action` means
repair one named gap, regenerate missing evidence, or stop on no-action.
`missing artifact`, `stale evidence`, `wrong root`, and `malformed artifact`
are fail-closed states. `preview-limited evidence` remains syntax-first and
advisory. `verify command`, `receipt command`, and `receipt path` are the static
movement proof rail, not runtime adequacy, mutation proof, or gate approval.

`receipts` writes machine-readable gate receipts under `target/ripr/receipts/`
for shape, fix-pr, ci-fast, check-pr, fixtures, goldens, test-oracle, dogfood,
and metrics runs. `receipts check` validates the required receipt files and
writes `target/ripr/reports/receipts.md`. `check-pr` refreshes receipts before
the final report index.

`check-allow-attributes` rejects guarded Rust lint suppressions such as
panic-family, unsafe-code, dead-code, unused-code, and broad warning
suppression attributes unless they are narrowly allowlisted in
`.ripr/allow-attributes.txt`. The row count is exact: source above the row
and a row above the source both fail. It writes
`target/ripr/reports/allow-attributes.md`.

`check-local-context` rejects committed local machine paths, Codex memory or
sandbox references, uploaded-file/chat citation artifacts, and runtime/session
state files. It writes `target/ripr/reports/local-context.md` and
`target/ripr/reports/local-context.json`. Narrow generic examples must use
`policy/local_context_allowlist.txt`.

`check-supply-chain` runs `cargo deny check advisories licenses bans sources`
using `deny.toml` and writes `target/ripr/reports/supply-chain.md`. It is a
local and CI security preflight; duplicate dependency findings are warnings
until the dependency graph baseline is stable.

`ci-fast` is the current non-mutating local and CI check lane. It runs the Rust
checks plus the existing policy checks for static language, panic-family usage,
lint-suppression bypasses, local context leaks, file policy, executable bits,
workflow shell budgets and action runtime policy, Droid workflow invariants,
spec format, fixture contracts, generated files, dependencies, process
spawning, and network policy.
The workflow check rejects avoidable Node-20-backed action majors and requires
Node 24 for extension build and publish workflows. Those policy checks write
Markdown pass/fail reports under `target/ripr/reports`.

## Command Lanes

`ripr` automation is split into three lanes.

### Mutating Shape Commands

Mutating commands are allowed to change files, but only for deterministic local
normalization.

Current:

```bash
cargo xtask shape
cargo xtask fix-pr
cargo xtask goldens bless <fixture> --reason "RIPR-SPEC-0001: reviewed static-exposure output change"
```

The bless line is a template: replace `<fixture>`, the example spec ID and
the description with the fixture, governing spec and semantic change you
reviewed. The reason must cite an existing spec under `docs/specs/`; an
ellipsis or a description without a spec ID is rejected before fixture execution.

There are no `--write` variants of `metrics`, `docs-index`, or
`capability-matrix`; earlier revisions of this section listed them as future
commands, but they do not exist in the command catalog.

Safe default mutations:

- formatting
- allowlist sorting
- policy manifest sorting
- generated docs/spec/ADR indexes
- generated capability matrix from machine-readable source
- generated metrics reports
- generated PR summary
- report directory creation

Not safe by default:

- accepting golden output changes
- adding policy exceptions
- adding dependency exceptions
- changing output schemas
- changing public contract versions
- adding suppressions

Those require an explicit command, a reason, or a manual reviewed edit.

### Non-Mutating Check Commands

Check commands verify the committed shape and must not modify the worktree.

Current:

```bash
cargo xtask ci-fast
cargo xtask precommit
cargo xtask check-pr
cargo xtask check-allow-attributes
cargo xtask check-local-context
cargo xtask check-droid-review-config
cargo xtask fixtures
cargo xtask goldens check
cargo xtask golden-drift
cargo xtask test-oracle-report
cargo xtask check-test-oracles
cargo xtask dogfood
cargo xtask lsp-cockpit-report
cargo xtask targeted-test-outcome --before <path> --after <path>
cargo xtask critic
cargo xtask reports index
cargo xtask receipts
cargo xtask receipts check
cargo xtask check-allow-attributes
cargo xtask check-local-context
cargo xtask check-traceability
cargo xtask metrics
cargo xtask check-capabilities
cargo xtask check-workspace-shape
cargo xtask check-architecture
cargo xtask check-public-api
cargo xtask check-output-contracts
cargo xtask check-identity-registry
cargo xtask check-doc-index
cargo xtask check-readme-state
cargo xtask markdown-links
cargo xtask check-pr-shape
cargo xtask check-supply-chain
```

Local tools may fix. CI verifies.

### Reporting Commands

Reporting commands produce review artifacts under `target/ripr/reports` and
machine-readable receipts under `target/ripr/receipts`.

Current:

```bash
cargo xtask pr-summary
cargo xtask commands
cargo xtask precommit
cargo xtask check-pr
cargo xtask fixtures
cargo xtask goldens check
cargo xtask golden-drift
cargo xtask test-oracle-report
cargo xtask check-test-oracles
cargo xtask dogfood
cargo xtask check-droid-review-config
cargo xtask targeted-test-outcome --before <path> --after <path>
cargo xtask critic
cargo xtask reports index
cargo xtask receipts
cargo xtask receipts check
cargo xtask check-traceability
cargo xtask metrics
cargo xtask check-capabilities
cargo xtask check-workspace-shape
cargo xtask check-architecture
cargo xtask check-public-api
cargo xtask check-output-contracts
cargo xtask check-identity-registry
cargo xtask check-doc-index
cargo xtask check-readme-state
cargo xtask markdown-links
cargo xtask check-pr-shape
cargo xtask check-supply-chain
```

Reports should be useful to both humans and agents. A failed check should name
the path, explain why the rule exists, classify the fix kind, provide exact
commands to rerun, and include an exception template when a policy exception is
appropriate.

`check-pr-shape` is advisory. It writes `target/ripr/reports/pr-shape.md` and
warns when a diff shape suggests missing evidence, such as analyzer code
without specs/tests/fixtures, output code without output contract evidence, or
policy changes without process docs.

## Fix Kinds

Every check should classify failures into one of four fix modes.

| Fix kind | Meaning | Example response |
| --- | --- | --- |
| `auto_fixable` | The repo can normalize this safely. | Run `cargo xtask shape`. |
| `author_decision_required` | The author must explain or adjust the change. | Update dependency policy with reason and owner. |
| `reviewer_decision_required` | The change may be acceptable, but it changes a contract. | Update schema docs, goldens, changelog, and compatibility notes. |
| `policy_exception_required` | The default policy rejects the change unless an exception is justified. | Prefer moving logic into `xtask`, or add an allowlist entry with owner and reason. |

The failure text should answer:

- what failed
- why it matters
- what can be auto-fixed
- what requires judgment
- which file to edit
- which template to use
- which command to rerun

## Repair Brief Format

Policy checks should converge on this Markdown shape:

````md
# check-name

Status: fail

## Violation

Path:

```text
path/to/file
```

Problem:

```text
short description
```

Why this matters:

```text
repo-specific reason
```

Fix kind:

```text
policy_exception_required
```

Recommended fixes:

```text
1. Move the logic into xtask.
2. Or add an allowlist entry if this surface is truly necessary.
```

Then run:

```bash
cargo xtask shape
cargo xtask ci-fast
```
````

## PR Summary

The PR summary is the reviewer packet. It should become the first file a
reviewer opens for any non-trivial PR.

Current summary fields:

- production delta
- evidence and support delta
- detected surfaces
- public contracts touched
- policy exceptions
- suggested reviewer focus
- follow-up commands

Next summary fields:

- machine-readable receipt links
- warning-only drift checks

The summary should classify large evidence-heavy PRs correctly. A large fixture,
docs, and golden diff can be scoped when it supports one narrow production
change. A small code diff can still be unscoped when it mixes unrelated
contracts.

## Pre-Commit Shape

Local hooks are optional. CI is the source of truth.

The desired local hook behavior is:

```bash
cargo xtask shape --precommit
cargo xtask precommit
```

`precommit` should stay cheap. It should prefer formatting, policy checks,
file-surface checks, spec format, fixture contract validation, and Droid
workflow invariant checks. It should not run release packaging, marketplace
packaging, real mutation work, or slow full-matrix checks.

For local policy triage, `cargo xtask check-policy-all` runs nine independent
source/Git scans in precommit-relative order: static-language, no-panic-family,
allow-attributes, local-context, covered-by, executable-files, process-policy,
network-policy, and lint-policy. It runs every selected check even after a
failure, preserves each leaf report/error, and writes
`target/ripr/reports/check-policy-all.md` with every outcome and failure's rerun
command. Any leaf error or aggregate report-write error returns nonzero.
`check-file-policy` is excluded because it can compile tests. The selected
leaf scans invoke no Cargo build/test/Clippy children; `cargo xtask` still
compiles the driver when needed. Ordinary local precommit/check-fast behavior
is unchanged. Hosted `ci-policy` reuses this same producer once before the
remaining policy checks; it does not add another nine-scan pass.

[RIPR-SPEC-0253](specs/RIPR-SPEC-0253-local-collect-all-policy.md) maps this
command's five acceptance cases to its actual-driver tests and production owner.

The policy inventory (the maintained execution authority is `PRECOMMIT_GATE_COMMANDS`
and `precommit_policy` in `xtask/src/main.rs`) is:

```bash
cargo fmt --check
cargo xtask check-static-language
cargo xtask check-no-panic-family
cargo xtask check-allow-attributes
cargo xtask check-local-context
cargo xtask check-file-policy
cargo xtask check-covered-by
cargo xtask check-executable-files
cargo xtask check-text-encoding
cargo xtask check-workflows
cargo xtask check-droid-review-config
cargo xtask check-spec-format
cargo xtask check-spec-numbering
cargo xtask check-fixture-contracts
cargo xtask check-rust-judged-panel
cargo xtask check-release-challenge-selection
cargo xtask check-release-challenge-judgments
cargo xtask check-python-judged-panel
cargo xtask check-traceability
cargo xtask check-capabilities
cargo xtask check-workspace-shape
cargo xtask check-architecture
cargo xtask check-rust-source-role-authority
cargo xtask check-public-api
cargo xtask check-output-contracts
cargo xtask check-identity-registry
cargo xtask check-doc-artifacts
cargo xtask check-doc-index
cargo xtask check-changelog-fragments
cargo xtask check-readme-state
cargo xtask markdown-links
cargo xtask check-pr-shape
cargo xtask check-command-catalog
cargo xtask check-generated
cargo xtask check-badge-diff-policy
cargo xtask check-generated-clean
cargo xtask check-proof-packs
cargo xtask check-release-targets
cargo xtask check-dependencies
cargo xtask check-process-policy
cargo xtask check-network-policy
cargo xtask check-lint-policy
```

Install local git hooks with:

```bash
cargo xtask install-hooks
```

This writes a ripr-managed `.git/hooks/pre-commit` hook that runs
`cargo xtask precommit` and keeps hook scripts out of version control. If a
pre-existing hook is already ripr-managed, the command updates it idempotently.
If a pre-existing hook is not ripr-managed, the command refuses to overwrite it
so local user hooks are not clobbered.

## CI Reports

CI uploads review artifacts from the Rust workflow when reports are present:

```text
target/ripr/reports/
target/ripr/receipts/
```

CI also writes `target/ripr/reports/index.md` into the GitHub Actions job
summary when the index exists. The report index lists available receipts when
`target/ripr/receipts/` has been generated.

Expected reports as the automation matures:

```text
shape.md
fix-pr.md
pr-summary.md
static-language.md
no-panic-family.md
allow-attributes.md
local-context.md
local-context.json
file-policy.md
executable-files.md
workflows.md
generated.md
dependencies.md
process-policy.md
network-policy.md
spec-format.md
fixture-contracts.md
traceability.md
capabilities.md
workspace-shape.md
architecture.md
public-api.md
output-contracts.md
doc-index.md
readme-state.md
markdown-links.md
campaign.md
goals.md
goals-next.md
pr-shape.md
fixtures.md
goldens.md
goldens-bless.md
golden-drift.md
golden-drift.json
test-oracles.md
test-oracles.json
dogfood.md
dogfood.json
critic.md
critic.json
index.md
index.json
receipts.md
pr-shape.md
metrics.md
metrics.json
release-readiness.md
release-readiness.json
suggested-fixes.md
suggested-fixes.patch
```

For untrusted PRs, CI should not push fixes. It may upload a suggested patch for
safe deterministic changes so authors or agents can apply it locally. Suggested
patches are repair hints, not policy exceptions: they must not carry badge
counts, golden blessings, baselines, suppressions, dependency exceptions, or
schema changes.

## Current Automation Entry Point

GitHub is the live board for PR automation. Start with open PRs before creating
new local work:

```bash
cargo xtask pr-triage-report
```

If no PR is waiting, reconcile the live work state from GitHub and the
local worktree (the `.ripr/goals/` scheduler was deleted in #1701):

```bash
gh issue list --state open --limit 20
gh pr list --state open
git status --short
git log --oneline -10
```

When the issue graph reports ready work, follow the named work item and keep
the PR inside the scoped PR contract. When it reports only blocked work, do
not infer ready work from chat history. Resolve the named blocker, record an
accepted bounded blocker in the issue, or choose a separate high-leverage
cleanup that does not claim the blocked campaign is complete.

For repo-ops automation, use the current mechanical front doors instead of old
campaign queue names:

```bash
cargo xtask cockpit
cargo xtask pr-ready
cargo xtask first-pr
```

`cockpit` gives maintainers the repo-level action queue, `pr-ready` checks local
PR readiness before opening or updating a branch, and `first-pr` writes the
start-here packet for one safe repair action when validated evidence supports
one. These commands are advisory;
they do not close PRs, alter manifests, promote claims, or prove the
self-hosted routed-runner closeout.

## Source-Of-Truth PR Body Scaffold

The `cargo xtask pr-body --work-item` command was retired with the `.ripr/goals/`
scheduler (#1701). Draft PR bodies from the current GitHub issue/spec/plan
model instead:

```bash
# Reconcile the work item from its issue and linked specs
gh issue view <issue-number> --json body,title,labels
# Draft the body from the issue's acceptance criteria and the diff
cargo xtask pr-summary
```

The PR body should link the issue, proposal/spec/plan references when present,
acceptance text, non-goals, and proof commands. Support-tier and policy impact
checkboxes must be reviewed from the actual diff and proof, not inferred from
issue metadata.
