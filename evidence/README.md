# PR #7817 evidence archive

Sanitized preservation of proof artifacts for the false-credit repair. Candidate: `a4e22c0c0ee52a5a858077b2639b3737934b55fa` / tree `0acdc48cdb96f6267fd7b2b799b918d42910323e`. Candidate branch remains separate and unchanged. See `provenance.json` for the base/composition identity and evidence scope.

The repair binds effect credit to the same assertion, typed receiver dispatch, and shared sink, and fails closed on unsupported expansion surfaces. It preserves real loop, operator, closure/for_each, and explicit-next observers plus existing decoys; general alias support is not claimed.

`tests/exact-a4/run-binding.json` and raw logs bind the requested final checks to a clean `a4e22c0c` checkout: `sibling_effect_oracle` passed 13/13, and goldens, honesty, and dogfood each exited 0. All four changed source files are fingerprinted in the run binding. The earlier focused run was stopped while goldens was in progress; the final exact-a4 run subsequently completed all four requested checks. Rustc original/write-removal controls remain fixture-level semantic controls recorded before candidate commit, not analyzer runs on the immutable candidate object; their sources, raw outputs, and receipts are preserved separately.

The independent source-review binding and candidate fingerprints are in `review/independent-final-review-binding.md` and `candidate-file-sha256.json`. The reviewer recorded hashes for two changed files; that hash binding does not cover `evidence.rs` or `transitive_reach.rs`. Full pinned external-corpus qualification remains blocked by the inherited setup failure tracked in [#7880](https://github.com/EffortlessMetrics/ripr-swarm/issues/7880); no full-corpus pass is claimed. No lifecycle, PR-body, gate, release, deployment, credential, or security action is included.

`SHA256SUMS` covers every other payload file in this directory; its own digest is recorded in the PR handoff comment.
