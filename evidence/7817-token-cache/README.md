# PR #7817 token-confirmation and expansion-cost repair evidence

Candidate: `a97ec757ba2bc84292729ee8675d0f5459d3808a`, tree `1742c3f7ceb9c5556571dbc28a82f90af9664283`, owned branch `codex/7794-sibling-effect-credit`.

P1 preserves independently confirmed tokens under `Some(false)` using the existing field/collection identity exclusions; kind-only false credit remains refused. P2 replaces per-probe source-byte copies and per-assertion workspace parsing with one lazy run-scoped expansion memo. The 64-source/768-query test observes 64 scans. Local receiver/test/owner parsing remains; this is not a wall-clock performance or general alias-resolution claim. Workspace macro/attribute/parse/Drop unknowns remain conservative. Renamed Drop import paths are now conservatively refused through their AST path.

## Exact subjects and results

- Pre-repair production: `a4e22c0c0ee52a5a858077b2639b3737934b55fa`, tree `0acdc48cdb96f6267fd7b2b799b918d42910323e`, plus the retained regression-only patch. `p1-public-red-valid` and `p1-reveal-red` compile the analyzer and fail at the intended weak-versus-yes assertion, exit 101. The first public attempt requested a SideEffect family that this Cell mutation does not generate; it is an invalid selection attempt, not the red witness.
- Final candidate exact-head native analyzer tests: `v5-public` 14/14; `v5-reveal` 114/114; `v5-expansion` 2/2; `v5-transitive` 66/66; `v5-native-roots` 5/5; `v5-cli-rerun` 17/17, all exit 0.
- Exact candidate `v5-goldens`, `v5-honesty`, `v5-dogfood`, `v5-check-fast`, `v5-precommit`: all exit 0. Stable check-fast selector: 55 paths, 15 gates, independently matched to the recorded main basis. This is local proof, not hosted merge qualification.
- Frozen current-main integration: merge `704c4f611c3586073e1efb322ae6fcd60c013f90` has parents repair `008fc842cf934d8e0270e0fa79a770958398ba4f` and main `a84449ad6cbbe026f838cab656ad82993ccdc5e3`; final a97 removes a duplicate neutral callback in the unit test. Thus final candidate/tree is the actual main-composed execution subject. The new sibling-exclusion field is kept separately from implicit-effect admission. No authored gate/settings/alias-support changes occur in the final diff against this main basis.
- Earlier disposable composition `6a089607056ea1643e11df587018aa7503347491`, tree `8454f317424e433cd12ad9a7c0eb7643313f2445`, parents main `1cdc7ef41a7f17b3b6d3536582b01a5e46952545` and predecessor `008fc842...`: six focused suites with the same counts, goldens and honesty all pass. These receipts retain their original subject and are not relabeled as final-head runs.
- Fresh Luna review: `independent-review.md` binds a97, inspected the changed/merged seams and independently reconciled the author receipts and selector. No source finding remains. Reviewer did not run Cargo; author executions are identified separately. Disposition `REVIEW_INCOMPLETE` for hosted PR qualification.

## Controls and failed attempts

`token-native-controls/` contains exact public-regression loop/operator input programs. Rustc originals compile and pass 2/2 each. `set(1)` to `set(0)` net-effect-removal variants compile and fail only the real sink observer (exit 101); smoke callers pass. These are fixture-level compile/runtime controls, not analyzer mutations. Source/binary hashes and raw logs are retained. The fixture input file is byte-identical between its original 008 owner and final a97 (`control-input-currentness.txt`). The gzip binary witnesses decompress to the binary hashes in their receipts; uncompressed local binaries are omitted from this compact archive.

All failed/intermediate receipts retain their own source identities: two initial cache ownership compilation errors; a new cached-surface test exposing a renamed-Drop import gap, then its conservative repair; four saved-candidate cmp_owned baseline diagnostics; three introduced Clippy signature/let-return diagnostics, then their repair; and 704's duplicated unit-test callback compile failure, then the one-line fix preserving `Some(false)`.

A goldens attempt on unchanged 8b lacked fixture-pinned anyhow 1.0.100 in the restored source cache. Locked Cargo fetch restored it; goldens then passed on unchanged source. No golden was blessed. This environment setup condition is separate from #7880.

The platform reported a disconnection at 09:29 UTC. Read-only recovery checks confirmed unchanged a97 and continuing original precommit PIDs. Its receipt then recorded explicit exit 0 at 09:33:54 UTC. No interrupted check is claimed as passing and no duplicate precommit was started.

Prior exact-a4 and compiler-only evidence remains immutable at `5585bfb60af9456a0aed3bbfe4132d4595dff89b`. The inherited external-corpus setup failure in #7880 remains separate; no full external-corpus pass, released analyzer parity, general alias/value-flow support, Ready, protected merge, release or gate clearance is claimed. PR body and threads are unchanged. Ordinary source/evidence branch push and technical handoff are the authorized publication boundary.
