# PR #8004 repair evidence

Source candidate `53e22999d19d07a28a355b740d62c01c25aea541`, tree
`07100736ea448043831337f64d45c7a26b6d1b79`, parent
`43f00b54eeb239c240bb71553c30a7bde3a4e080` (tree
`b520b108591cf046c3498e6558604c9f6101eac7`). This is an author repair
receipt, **REVIEW_INCOMPLETE**, not independent review or merge clearance.

The selected environment became usable; earlier executor provisioning failures
are historical. No source work was recovered from those failed attempts.
The independent review packet at `6f338913cf8a7b9f89aa8554892e0b07e74a26e9`
was fetched and all 68 manifest entries verified. Its manifest SHA256 is
`f352b982fe1975044fd963516ce30585a965b8b95457b0c96764e40204b38b5d`.

## Behavior and controls

Executable same-name local-receiver delegation now survives declaration-artifact
filtering. Inherent helper entry and last-hop calls must establish bounded receiver
identity through the existing direct-owner authority. Module-scoped assertion
macro shadowing now refuses operand credit consistently in diff and repository
consumers. Existing qualified-variant and live outer-receiver fixes remain.

The three new fixture inputs are the review's isolated macro, foreign-helper and
local-wrapper subjects. Six additional complete native Rust controls distinguish
standard/sibling-macro/local-helper positives from foreign-entry/foreign-last-hop/
module-type-shadow negatives. `final-native-expanded/mutation-identity.json`
binds the exact originals and mutants: each mutant changes exactly the private
Object arm from `Some(*value)` to `None`. All twelve compile and run; all originals
pass, positive mutants fail, negative mutants survive. Original review controls
and qualified/live-receiver native challenges are also retained under `native/`.

`baseline-cli/` uses an explicitly rebuilt unrepaired production binary at43
(dirty only for new regression tests). `repair-cli/` uses repaired production
before commit. `final-head-cli/` uses the clean committed default-feature binary53;
its version/hash/size are in `final-cli-identity.json`. Complete raw JSON remains
unmodified. `paired-input-identity.json` binds identical source subjects and hashes
for base and repair. The wrapper retains repository helper/high and four
strongly-gripped seams; the foreign helper loses high credit and those four false
strong grips; the discarded macro loses direct/high diff credit. Weak fallback
relations and conservative diff findings are intentionally still visible.

The initial accidental stale CLI instrument was detected from its version and
rebuilt. `stale-cli-source3865/INVALID.txt` marks its output unusable for baseline43.
The stale binary is not published. `red-repo-controls.log` is the intended failing
wrapper regression on unrepaired production. `compile-failure-E0515.log` retains
the first repair compile failure, corrected before green tests; no exact failed-tree
claim is made. `staged-whitespace-failure.log` retains the precommit whitespace
failure, fixed before the candidate commit. No failures were erased.
`baseline-check-fast.log` also retains a formatting failure in newly authored
regression tests before formatting; it is not evidence that pristine base failed.

## Executed qualification

Linux x86_64, Rust/Cargo1.95.0, one build job, incremental disabled, dev/test debug0.
Rust tests use default features and `--locked`. Xtask internal builds use the
unchanged repository lockfile without adding a forced `--locked` argument.

- Exact committed53: test-grip evidence383, related-tests175 and CLI rerun units43
  pass; final logs are prefixed `final-head-`.
- Same production bytes before commit: native receiver integration8 and native
  path5 pass; CLI rerun integration16 pass; unit-struct native/static control1
  passes. No ignored subjects in these selected runs. The underscore CLI filter
  selects16, not the historical17.
- Exact53: all four verdict corpora pass:36 Perl,69 Python,485 Rust,169 TypeScript.
  Rust26/485 false verdicts remain; contradictions0/621, spec131/131,
  oracle44/44 and overclaims0/44. Perl has no spec-coverage ledger. TypeScript
 112/120 meets its existing floor. Reports are retained under
 `reports/final-head-verdict-corpus/`.
- All five named existing observation rows remain `false_actionable`; truth,
  ideals and acceptable labels are unchanged. `five-corpus-observations.json`
  records them. Honesty corpus passes, including five added paired assertions
  and three complete report projections in source (only volatile envelope removed).
- Exact53: all453 golden fixtures pass, zero drift/errors. Only the three NEW
  fixtures were initialized with spec-linked reasons. No existing golden was
  re-blessed. Full reports identify the clean53 CLI.
- Exact53: check-fast passes all15 gates, none skipped, including strict
  workspace/all-target Clippy. Selector independently matches58 three-dot paths
  at merge-base67866bc9. Precommit passes with complete58-path change set,
  no limitations or skips. Reports and terminal logs are retained.
- Dogfood passes with nonempty boundary and weak-error findings; this is an
  analyzer/report control, not a claim of no findings.

### Correction to the previous golden account

The earlier failed full-golden execution and equal base/head/final JSON were real,
but their cause was missing documented fixture preparation, not established
inherited semantic drift. A separate ordinary registry
`cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml`
prepared locked anyhow1.0.100. Prepared baseline and repair complete parsed JSON
are equal, exposed, and findings/summary match committed expectation. See
`fixture-fetch.log` and both `*-prepared-golden.json`. A separately locked scratch
serde_json1.0.150 preparation supports the existing native integration fixture.
Root manifests, lockfile, dependency/process/network policy were unchanged.

## Integration subject

Inspected main `7dee26dd330fd648ed4f8ba46225cc78276fb147` and candidate53 compose
without textual conflicts as tree `c80d13b636ac00b1fc695c5f41ff42e058183556`.
Detached evidence commit `72e25151dd03a669575c57175321072e69b24d42` has those
parents (main first). Source branch was not restacked. Main's relevant deltas
include assertion-helper returned-Result credit and output/syntax changes;
composition execution is recorded in the accompanying composition logs.

Executed default-feature composition checks:383 test-grip tests,175 related-test
units,23 assertion-helper integration tests,8 receiver integration tests and5
native-path integration tests pass, zero failures/ignored in these selections.
The affected three fixture comparisons use the frozen source53 xtask driver
(hash retained); its runner/normalizer code is unchanged, and it explicitly
builds the actual composition CLI before use. It is not represented as a
composition-built xtask or a full composed golden/corpus suite.

Later main `37a31f949791827e6f29532d35ca980c19a3d70a` adds MCP response sizing
and names unresolved transitive paths beyond five hops. The latter adds no
direct/helper/high receiver authority; this repair's subjects are within the
existing bound. It composes textually as tree
`665505641bd21f018359268ed614a34b205bf45b`, without conflicts. No runtime claim
is made for that later tree; execution remains bound to frozen7dee/72e.

## Limits and retention

Rook owns assignment of fresh independent published-head review. Required hosted
default-feature qualification remains outstanding; historical Rust-only job
116287116012 ran an older combined tree and is not this qualification. Earlier
hosted15980 counts included two skips and one ignored doctest; do not reuse a
no-skips claim. No full local workspace-test or native Windows qualification.

Receiver authority remains bounded nominal syntax, not general type/Cargo-target
identity, arbitrary factory/parameter/field/cross-file flow or macro expansion.
Unsupported evidence can still produce false-actionable advice. Parsing cost
follow-up #8005 is not benchmarked or cleared here. #7415 remains open. External
threads remain unresolved for fresh review. Draft stays Draft.

Local worktrees, target/cache and frozen executables are retained for open-Draft
review/reproduction; they are not evidence Git blobs. No destructive cleanup.
The immutable packet excludes executable files, temporary implementation scripts
and volatile coordination checkpoints. Older original PR body/handoff are retained
to make the corrected golden and source claims inspectable. No Ready, merge,
release, deployment, credentials, policy weakening, thread resolution or bot contact.
