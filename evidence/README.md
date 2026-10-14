# PR #7817 evidence archive

Sanitized preservation of proof artifacts for the false-credit repair. Candidate: `a4e22c0c0ee52a5a858077b2639b3737934b55fa` / tree `0acdc48cdb96f6267fd7b2b799b918d42910323e`. Candidate branch remains separate and unchanged. See `provenance.json` for the base/composition identity and evidence scope.

The repair binds effect credit to the same assertion, typed receiver dispatch, and shared sink, and fails closed on unsupported expansion surfaces. It preserves real loop, operator, closure/for_each, and explicit-next observers plus existing decoys; general alias support is not claimed.

`tests/focused-receipts.json` records seven focused command exits after the candidate commit; raw post-commit logs are included for reveal and transitive only. The 13/13 public suite result was reported on the final repaired source, but raw stdout was not retained; an older raw 13/13 log is preserved separately and explicitly marked pre-final. The Rustc original/write-removal controls are fixture-level semantic controls created before candidate commit, not analyzer runs on the immutable candidate object. Their sources, raw outputs, and receipts are preserved with that scope.

Independent source-review binding and the exact recorded file fingerprints are in `review/independent-final-review-binding.md` and `candidate-file-sha256.json`. Full pinned external-corpus qualification remains blocked by the inherited setup failure tracked in [#7880](https://github.com/EffortlessMetrics/ripr-swarm/issues/7880); no full-corpus pass is claimed. No lifecycle, PR-body, gate, release, deployment, credential, or security action is included.

`SHA256SUMS` covers every other payload file in this directory; its own digest is recorded in the PR handoff comment.
