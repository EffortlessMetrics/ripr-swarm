# PR 7817 handoff

- PR #7817 remains open and Draft. Candidate branch `codex/7794-sibling-effect-credit` remains at `a4e22c0c0ee52a5a858077b2639b3737934b55fa` (tree `0acdc48cdb96f6267fd7b2b799b918d42910323e`).
- Intake head/tree: `9d4d4adf74726854bc106a1e910a1edc2467aafd` / `5b932730ea11e44a07e61f921e51a3d9dfe67612`. That merge has parents `b61141690a0dcd9e679ea2ca6f3e9261357c5e62` and PR base `main` at `ef2bce9017dde50949f4dbf26c7ff25ec6446247` (tree `76e9686acb199752075568fbe36fa4efb430ad01`). Candidate repair commits are descendants of this composition.
- Exact-candidate checks completed on a clean checkout of `a4e22c0c`: `sibling_effect_oracle` passed 13/13; goldens, honesty, and dogfood each exited 0. Raw stdout, exit codes, UTC completion times, candidate tree, and SHA-256 fingerprints for all four changed source files are in `tests/exact-a4/`.
- Native Rustc controls compile fixture programs with/without the write. They were run before commit a4, so they are not represented as immutable-a4 analyzer runs. Their sources, compile/run logs, and outcomes are retained for negative-control validity evidence.
- The independent final source review’s two recorded file hashes match candidate a4, but cover only 2 of the 4 changed source/test files; the changed `evidence.rs` and `transitive_reach.rs` are not fingerprint-bound by that review. See the review binding.
- The inherited #7880 pinned external-corpus setup failure remains separate and blocks full external-corpus qualification.

This is evidence preservation only: no candidate code, PR body, status, gates, or lifecycle state is changed.

The original evidence archive is immutable at commit [`4959fec55d394b432b15c17fe78cc174b20c3d12`](https://github.com/EffortlessMetrics/ripr-swarm/tree/4959fec55d394b432b15c17fe78cc174b20c3d12/evidence). This follow-up adds exact-candidate qualification receipts; the latest immutable commit is linked from the PR handoff comment.
