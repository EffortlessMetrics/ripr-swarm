# Historical queue bootstrap payloads

These are test inputs outside `.github/workflows`. They cannot allocate Actions
jobs and must not be copied back as ordinary development request routes. The
current source contract is native `merge_group: checks_requested`; activation
remains a separate protected transaction in [merge flow](../../../docs/MERGE_FLOW.md).

- `routed-rust.yml` is the retained label/main/manual predecessor staged with
  commit `05c1266ba7c34d740542e2de50cc67e54720f876`. Existing shard/fallback and
  PR-readiness controls continue to exercise this historical payload.
- `merge-ready-conflict-notice.yml` and `pr-staleness-watchdog.yml` are exact
  bytes from commit `f6b604dfb0641024e321d6330ed3ba69ed30a7a1`, immediately before
  their removal. Their native shell controls retain head/base currentness,
  in-flight refusal, retries, and comment/status ownership coverage.
- `ripr-pr.yml` is exact bytes from commit
  `82c835bc2e502aa565855d35da665c63561dc43d`. The product's eight behavioral
  refusal-reporting tests keep exercising its actual analyze/comment scripts,
  including diagnostic redaction and forged-artifact refusals. This is retained
  historical payload coverage, not a restored reviewer or the generated adopter
  workflow's independent test suite.

`xtask/tests/pr_readiness_workflow_contract.rs::current_queue` checks the live
caller, retained explicit capabilities, and refusal of these automatic roots.
The fixtures do not replace live-source workflow policy or qualify activation.
