# Inactive mandatory-test partition prototype

Scope: Steven's partition-plan/config proof assignment, report
`ea51413e5f6d78113f9ee55c3bbede441309fc0a`, under the existing #4841 economics
and #4987 CI-routing work. This is an offline experiment, with no workflow
consumer or activation. Slow fixtures remain with #7825; Clippy composition
with #7826; cache continuity with #7827; actual-G evidence with #4836.

## Current cause and proposed layout

The current required producer is `.github/workflows/rust-gates.yml`: one
`cargo nextest run --workspace --profile ci`, default features, retries zero,
followed by separate `cargo test --workspace --doc`. It retains fresh JUnit
and run context. `.config/nextest.toml` has no selection filters or overrides.
The repository's runner-policy tests deliberately reject narrowing that row.
Changing the row alone would violate that contract; activation needs its own
reviewed change preserving the aggregate required status and all obligations.

Use **native hash partitions**, evaluating **2 first, then 3, at most 4**.
Every partition is mandatory; matrix fail-fast must be false. Use the same
head/tree, runner version, toolchain, target, features, Cargo profile and
nextest `ci` profile. No skip/default-filter, retries, feature reduction or
test-group override is proposed. Keep Cargo doctests separate and mandatory.
Nextest supports stable hashing of binary ID plus full test name; retain the
exact nextest version with the plan. Do not reproduce its hash in this tool.
([Native partition semantics](https://nexte.st/docs/ci-features/partitioning/))

Prefer one compatible build/archive producer plus N execution consumers over
N full rebuilds **only after** verifying runtime resources, source-root
assumptions, archive identity, and measuring archive/transfer cost. Each
consumer needs the same source snapshot; tests can read repository fixtures
or launch Cargo. Reusing a compiled archive does not establish those runtime
dependencies. Per-shard JUnit/context artifacts must have unique names and
fresh directories. Do not merge XML files by overwriting the same path.

Selection is the native command with just `--partition hash:M/N` added. List
the unpartitioned universe and all N partitions before execution, using the
same inputs and `--message-format json`. Verify exact disjoint union, preserving
binary ID and full test name. Ignored tests remain separately visible.
Missing, empty, differently filtered or mismatched inventories refuse.
Do not compare only totals: equal counts can hide omissions and duplicates.

## Budget decision, not yet qualified

The requested `ea51413...` report was not present in the selected environment
or current source. Its type, producer, complete population, and timing values
are **not established**. It is an input reference, not an authenticated report
digest in the prototype. No balanced count or <=20-minute result is claimed.

A separate historical receipt on [#4841](https://github.com/EffortlessMetrics/ripr-swarm/issues/4841#issuecomment-6158875557)
records checkout `ce237e6c3087b95a33133e9bfefb80548a9c6a75`, run 38472100072,
15,398 JUnit cases, 1,321.230 seconds nextest execution and 5,157.305 summed
testcase seconds. Its 3,112-second complete job includes other obligations.
These observations motivate the experiment; they do not replace the named
report or predict a current shard's wall time.

Once the report is accessible, join every timed case to the native full
binary-ID/test-name inventory; report missing, extra, retried and ignored
subjects rather than giving missing cases zero duration. Map timings to the
**native listed** partitions for N=2,3,4. Compare summed durations, largest
individual case, and execution concurrency; equal test counts alone are not
balance. Preserve measured fixture improvements as a separate input revision.

Select the smallest N whose **whole critical path** is <=1,200 seconds and
whose total compute is justified against the same-head unpartitioned control:

```
wall = queue/setup + shared build/archive + max(transfer + shard execution
       + report finalization) + aggregate and remaining mandatory critical path
runner consumption = build producer + sum(all consumer allocations)
                     + aggregation + other mandatory allocations
```

Also record CPU/storage/transfer where available. Summed testcase durations
are neither CPU time nor wall time. A lower bound from durations/concurrency
cannot certify a wall-time budget. Include doctests and all mandatory non-test
steps in the final <=20-minute decision; test-only partitioning may be
insufficient. Reject a count that only redistributes an indivisible slow case
or repeats costly builds. If no N fits economically, keep this inactive and
return the measured remaining bottleneck to its existing owner.

## Offline verifier and failure model

`xtask/examples/mandatory_partitions.rs` consumes native JSON list files. It
launches no processes and writes no reports or workflows. The adjacent TOML
pins inactive status, candidate counts, profile and budget.

```
cargo test -p xtask --example mandatory_partitions
cargo run -p xtask --example mandatory_partitions -- \
  docs/ci/mandatory-partitions.toml full.json hash-1-of-2.json hash-2-of-2.json
```

For the aggregation model, append `--receipts CONTEXT receipts.json`. The
normalized input is an array of objects:

```json
[{"shard":1,"context":"head/tree/config/runner/attempt","exit":0,
  "complete":true,"passed":[["binary-id","full::test::name"]]}]
```

Require exactly N distinct terminal receipts, the caller's exact context,
native exit zero, complete execution, and exact expected passed subjects per
shard. Failures, cancellation, timeout, stale/missing/duplicate receipts and
subject omissions cannot aggregate to success. The future hosted collector
must run unconditionally after matrix completion, require all job conclusions
and separate doctest success, and refuse absent artifacts before parsing.

**Trust boundary:** these are caller-supplied offline model inputs, not trusted
execution receipts or a JUnit parser. The tool does not authenticate context,
parse XML, attest artifact freshness, or authorize CI acceptance. Never let
candidate-authored normalized receipts become a hosted success authority.
A future trusted collector must derive them from native results, job state,
and independently bound input/artifact identities. Unknown JSON/schema states
refuse; the prototype targets nextest 0.9.148 and is not a general adapter for
the separately owned #5177 verification-plan route.

## Proof and next transition

Focused tests cover exact union, duplicate binary-local names, ignored subjects,
empty/missing shards, unexpected selection filters and aggregate refusal.
The execution receipt accompanying this branch distinguishes actual native
nextest fixture evidence from synthetic model controls. Stability must be
checked with native listings before/after unrelated test insertion, retaining
the original binary ID, test name, N and runner version.

No workflow/protection/release change, broad hosted CI, or slow-fixture repair
is authorized by this prototype. Publish the branch without opening a PR that
would trigger the current broad pull-request suite. Next owner: the campaign
coordinator supplies the missing report and decides whether the measured
layout warrants a separately admitted rehearsal. Rollback removes only these
prototype files and its changelog entry.
