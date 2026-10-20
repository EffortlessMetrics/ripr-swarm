# Agent brief parser-refusal evidence

Candidate `b1998c3907f0470138fd67df0feaec5af5d61d68`, base `1e9468f4b0d133f0a50b64134318763d8cf48beb`, EffortlessMetrics/ripr-swarm.

The real CLI preserves valid empty/fallback/ordinary brief success and refuses shared-parser limitations with exit 2 and `diff_input_unsupported`. Three final focused tests (14 CLI invocations), check-fast and precommit exited 0. Check-fast independently selected all 10 changed paths and ran 15 gates, skipping fixture contracts. Its existing help-parity test and strict workspace all-target Clippy passed.

Precommit initially reported one golden comparison INCOMPLETE because locked anyhow 1.0.100 was unavailable offline. The installed `goldens prepare` route prepared exactly that fixture; a separate unchanged-source `goldens check` then passed. No golden was blessed and no fixture/lock/expected-output source changed. Preserve this distinction when reading the original precommit receipt.

`execution.json` binds exact commands, sources, environment, limits and resource disposition. `candidate.patch` is the full recoverable repair. `source-review.md` records independent source/oracle review and its carried-forward dimensions. `candidate-reports/` includes the final native reports. `golden-actuals.tar.gz` preserves all 1521 generated golden output files; its verification JSON records restored byte hashes, sizes and modes. Executable hashes are provenance only: the binaries remain in the retained local target and are not delivered in this artifact.

The original red receipt has one valid combined-hunk defect witness. Its original base fixture was uncommitted and therefore supplied an empty committed diff: that base-red interpretation is withdrawn. The corrected committed-base fixture has final green proof; no corrected-base red is claimed. The separate module-path compile failure did not reach behavior.

Latest inspected main `ce0966416c4bc3c7d6c470737a3e3cd716055fd3` adds unrelated xtask lock-wait and oracle wording repairs after the recorded base. No candidate-file or instruction overlap was found; no rebase or combined-tree qualification is claimed.

Parent owns follow-through. The candidate stays Draft / REVIEW_INCOMPLETE for hosted qualification. Bounded evidence totals and their prior candidate/tests were untouched. No merge, Ready/qualification-label, auto-merge, release, deployment, settings or credential action was taken.

All files below are native execution evidence for this single claim, not a repository-wide status registry. Verify payload sizes and SHA-256 using `manifest.json`.
