# Issue #7826 CI policy-composition evidence

This immutable evidence bundle is for PR #7918. Candidate source identity: `3abb3b69f88e9865ce97c41c3e1db2e24010737f` / tree `171375b330c38394c78c5ba619a1601d0c82edf7`. The latest-main disposable composition and the GitHub PR merge-ref identity are recorded separately in `main-integration.json`; local composition success is not a hosted check.

`integration/` contains the exact focused command results and policy receipts for latest main + candidate. `candidate/` contains frozen-head executable validation and the remaining hosted qualification readback. `review/` binds the independent source-review disposition to the frozen commit/tree. `timing/` preserves one matched warmed-cache bounded-prefix pair and the raw launches; the candidate was slower in this pair, and this is not complete-lane timing. `negative-controls/` labels final-head receipt rejection controls separately from causal policy/Clippy controls on the earlier `9cdec14e182aa3c726274b737fa7b41a3f2386ef` subject.

No binaries, target directories, credentials, or build caches are included. The manifest records each payload file's byte length and SHA-256.
