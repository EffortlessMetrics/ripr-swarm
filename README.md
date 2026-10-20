PR #8961 qualification continuation evidence

Exact source: 1a525eaaba07bb416a83a4f4d76c833a20cb5256; original base: 1e9468f4b0d133f0a50b64134318763d8cf48beb. candidate.patch contains the full candidate delta; repair.patch contains the continuation delta from b1998c390.

Current focused tests: 4 passed, 20 CLI invocations. check-fast: exit 0, 15 gates, 10 selected changed paths. precommit: exit 0, fresh goldens pass with no fixture/lockfile/expected-output edits or blessing. Raw logs and 54 fresh native reports are retained. execution.json binds commands/runtime/results and names historical or missing dimensions.

The original evidence remains immutable at d0e95e2c733541d8e088b04c8ce6a2c0d6fd5cd7. Old evidence is not relabeled as a current-head run. The corrected original base fixture has no claimed pre-repair red. The new admission-identity red reached its intended early mismatch on old production; later/final fixture setup is established by the green run.

Golden outputs: restore golden-actuals.tar.gz into a separate empty directory; it contains fixtures/ and belongs under target/ripr/. golden-restoration-verification.json records comparison of all 1521 files by bytes, size and mode; the verification staging was released. Actual CLI/test executables remain local task-owned witnesses and are not delivered in this evidence tree.

At this receipt, ordinary required hosted qualification has not yet been requested and advisory ub-review is in progress. A later receipt must establish actual job checkout and terminal results. Integration, auto-merge, release and deployment remain held.
