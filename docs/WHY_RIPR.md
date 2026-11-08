# Why ripr

**Problem:** a change passes review and CI, and the tests would still pass if
the change were wrong. Coverage says the line ran. Nothing says a test would
notice.

```diff
- if amount > discount_threshold {
+ if amount >= discount_threshold {
```

Tests below and above the threshold pass on both versions. Only
`amount == discount_threshold` with an exact assertion tells them apart.
`ripr check` names that missing case from the diff and your tests:

```text
  Changed behavior: amount >= discount_threshold
  Missing check: amount == discount_threshold
  Related test (1 of 2 matched rows): tests/pricing.rs:4 below_threshold_has_no_discount (no test input reaches `amount == discount_threshold`)
  Next step: Add boundary tests for below, equal, and above the changed threshold with exact assertions
```

(Selected lines, in order, from the [boundary example's checked output](../fixtures/boundary_gap/expected/human.txt); `cargo xtask goldens check` keeps that output current.)

## What you get

- **While you write or review.** A specific missing check for the changed
  behavior, with the related test, before CI runs.
- **For a coding agent.** A bounded find-gap, write-test, verify loop with a
  done and stop rule: `ripr init --agents claude|codex|cursor|all`. See the
  [agent quickstart](AGENT_QUICKSTART.md).
- **On a pull request.** `ripr init --ci github` generates an advisory
  workflow. It does not block merges by default.

## How it differs

| Tool | Answers | Cost | Limit |
| --- | --- | --- | --- |
| Coverage | Did this line run? | Runs the test suite | Says nothing about whether a test would notice a wrong result. |
| Runtime mutation testing (for example cargo-mutants) | Does the suite fail when a concrete mutant runs? | Builds and runs tests per mutant | Slow on large diffs; the answer arrives after the run. |
| Formal verification (for example Kani, Verus) | Does this property hold for all inputs? | You write and maintain the specification | Covers only what you specified. |
| ripr | Do the related tests appear to contain a check that would notice this change? | Reads source and the diff; runs no tests | Static and advisory; opaque paths are reported as limits. |

They answer different questions and stack. Use ripr first, on the diff, to
find where a focused test is missing. Use mutation testing to confirm at
runtime. Use formal verification for properties you need proven.
[`ripr calibrate cargo-mutants`](TARGETED_TEST_WORKFLOW.md) compares ripr's
static findings with a cargo-mutants run you did.

## What ripr does not claim

- It does not run your tests or mutants, and does not prove a test would fail.
- An empty result is not a clean bill of health.
- A before/after receipt is not a runtime mutation result.
- It is alpha. Rust is the deepest language; Python and TypeScript are
  preview. See the [status table](../README.md#status).

Measured agreement and shortfalls are in [Proof and limits](PUBLIC_PROOF.md).
Try it: [first useful run](../README.md#the-first-useful-run).
