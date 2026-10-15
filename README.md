# Independent PR 8135 proof

Disposition: REPAIR_REQUIRED. Source head 720e27d14c46e8a48129dcc0d185272bbc4ca1cc, tree 7b804108c45e50d00e189d1027aba31ba432516a, base a142074061128a85e76c1ac494b9939f3a58428b.

The archive contains raw native logs, complete finding/record JSON, an additive test-only patch, exact subjects, commands and environment. Production source is unchanged.

Reproduce in an isolated checkout of the source head: apply independent-controls.patch, create target/ for the repository temp redirect, then use the environment in run-qualified.sh to run cargo test -p ripr --lib output::gap_decision_ledger -- --nocapture.

The real Python parser/diff/card/JSON/ledger pipeline selects pytest TestGood.test_same at line 28. JSON renders eight of nine related rows, retaining a unittest namesake at line 25; the ledger incorrectly emits 25. Missing selected identity in a partial inventory must withhold precision. The intended native assertion fails: Number(25) versus Null. Native result: 113 passed, 1 failed. The original 112 tests and an added 16-case complete-record control pass; all five existing selector tests pass separately. This does not execute external pytest or an editor session.

Pristine exact-head check-fast passed, including strict workspace/all-target Clippy and a stable four-path selector. Local precommit was still in flight at this immutable proof freeze; its terminal outcome will be retained separately. No aggregate or hosted qualification is claimed.

Verify outer SHA256SUMS, extract into a fresh directory, then run sha256sum -c PROOF-SHA256SUMS inside it. The nested original producer packet was independently verified: all 21 internal hashes, red-tree reconstruction, and policy-report digest.

No binaries, caches or source checkout are included. No submitted-source push, policy weakening, Ready, merge, release, thread resolution or bot contact occurred.
