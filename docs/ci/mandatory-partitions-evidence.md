# Partition prototype execution receipt — 2026-10-15 UTC

Basis: `d4c952532ad29ac005fcf231748357ae41f6519b` (`origin/main` at creation).
Branch: `codex/inactive-mandatory-partitions`, isolated Linux/Bash worktree.
Rust/Cargo 1.95.0. Nextest 0.9.148, commit
`cd1d6d5467322dbc0d39a80cb671d000ac302798` (official Linux binary).
Prototype source SHA256 at these observations:
`b72d958d94ab9611b461a819c5a4afc847752c127894fad7a570f2ef4ad35687`.

## Observed results

`cargo test -p xtask --example mandatory_partitions`: **5 passed**, zero
failed/ignored/filtered. These are offline model tests, not the product suite.
The CLI verified native two-shard inventories and normalized successful native
results, exiting zero. Replacing shard 2's receipt with the real failing run's
normalized results produced `failed, stale, repeated or incomplete shard 2`,
exit **1**. Synthetic tests additionally refuse missing/duplicate/stale receipts,
timeouts/cancellation/nonzero exits, omitted subjects, wrong binary identity,
unexpected filters, ignored execution and count drift.

Native nextest fixture proof used **the exact current repository ci config**:
Git blob `c30d32d5a714e28988a586cd95500e0bd915a6f9`, SHA256
`0d25d75967d86d7426cc96de297a65abca466b2aab29eb8f510f317e9fb12a46`.
No retries/default-filter/group changes. Package `partition-witness` 0.0.0,
Rust edition 2024, library tests:

```rust
#[cfg(test)] mod tests {
    #[test] fn a() {} #[test] fn b() {} #[test] fn c() {} #[test] fn d() {}
    #[test] fn e() {} #[test] fn f() {} #[test] fn g() {} #[test] fn h() {}
    #[test] #[ignore] fn ignored() { panic!("ignored witness"); }
    #[test] fn failure_witness() {
        assert!(std::env::var_os("PARTITION_FAIL").is_none());
    }
}
```

Native list command: `cargo nextest list --workspace --profile ci
--message-format json`, then the same command with each `--partition hash:M/N`.
Native run command: `cargo nextest run --workspace --profile ci
--partition hash:M/N`. All operated inside the separate temporary fixture,
not the product workspace. Every shard's parsed JUnit passed-subject set
matched its native selected list; all native successful exits were zero.

| N | Runnable subjects per shard | Union | Ignored |
|---|---|---|---|
| 2 | 6, 3 | 9, disjoint | 1 |
| 3 | 1, 5, 3 | 9, disjoint | 1 |
| 4 | 2, 1, 4, 2 | 9, disjoint | 1 |

Adding `#[test] fn unrelated_inserted_case() {}` outside the module and
relisting every shard for N=2,3,4 left **all nine original memberships
unchanged**. The original source was restored afterward. A real native
`PARTITION_FAIL=1` execution of hash:2/2 returned **100**, with exactly
`tests::failure_witness` failed in JUnit. The failing run is not a passing
fixture qualification. These cases establish selection/stability/failure
mechanics for this fixture, not repository suite timing or general adequacy.

## Baseline, boundaries and remaining uncertainty

Initial `cargo xtask worktree doctor` completed with no findings after building
the baseline xtask. The initial `check-fast` ran six policy gates and skipped
six with an independently confirmed zero tracked diff at the base; the added
prototype files were still untracked. **That selector result is not candidate
qualification.** Focused example compilation/execution above exercised the
added code. Committed-candidate checks/review are recorded in the branch's
publication handoff, separately from this initial floor.

No report identified as `ea51413e5f6d78113f9ee55c3bbede441309fc0a` was found
locally or in the completed finding-specific connector searches. The shell
GitHub API's issue-search and PR-search operations, and GET
`repos/EffortlessMetrics/ripr-swarm/commits/ea51413e5f6d78113f9ee55c3bbede441309fc0a`,
returned **Forbidden**. Those denied targets were not retried by another route.
This is an access failure, not evidence that the report does not exist.

The normalized aggregation model does not parse/authenticate JUnit, job state,
or source context. No real repository-wide partition, full mandatory pipeline,
archive reuse, hosted test, Windows test, current report-based balance, total
compute saving or <=20-minute result has been established. The plan remains
inactive until the report and a separately admitted exact-input rehearsal can
answer those questions. No slow-fixture source was changed.

Raw local fixture/source, native JSON/XML/logs, model logs and the reproducible
native driver are retained at `/workspace/partition-evidence` for the coordinator
and independent reviewer. This path is executor-local; this committed receipt
and the fixture above are the GitHub-readable evidence, not a claim that local
files have been uploaded. The worktree/target are retained for pending review;
the original checkout and owner branches were not edited.

## Scoped repair and independent oracle challenge

Independent review of `df158cbd656b2a633659335fa458804951eb2d27`, tree
`6c21481086ef075501491f6cc7af893ace1e6a11`, independently recomputed the native
inventories, XML and insertion-stability comparisons. Its separate scratch
baseline passed 5/5; individually removing the exit, complete, context or
passed-subject comparison made the unchanged aggregation test fail (exit 101
for each). Scratch mutation sources differ only in the named comparison and
the relocated `include_str!` fixture path. These are controls on that original
candidate, carried forward because the aggregation implementation/tests are
unchanged by the following parser repair.

Review also confirmed a small CLI robustness defect: naming the required
full-inventory file literally `--receipts` caused an inverted slice and exit
101. The repaired parser recognizes the optional marker only after required
inputs. The same real CLI invocation now prints `coverage complete; runnable
per shard: [6, 3]; ignored: 1`, exit 0. A persistent argument-boundary test
also covers the literal filename together with a later real receipts marker.

Repaired example source SHA256:
`46a3a6a936f53fed5c225de0712c4b8f897254b842416dcd13602a2c4d8c956f`.
`cargo test -p xtask --example mandatory_partitions`: **6/6 pass**.
`cargo clippy -p xtask --example mandatory_partitions -- -D warnings`: pass.
Changelog-fragment, no-panic-family, allow-attributes, doc-index and Markdown
link checks pass. No full precommit/product-suite/hosted claim is made.

Committed initial-candidate `check-fast` selected the independently verified
five-path diff and passed six gates, skipping six. Its existing category
heuristic excludes `xtask/examples/` from automatic Rust Clippy/file-policy
selection; this was inspected in `categorize_changed_files`. The direct
focused Clippy and example tests above cover the new Rust surface. The selector
was not changed to expand this assignment. Final head/currentness and the
affected-head review disposition are in the publication handoff.

Remaining report lookup: Personal Context returned `Personal context is
unavailable for this conversation`; no timings or prior decisions were
invented from that unavailable source. The required report input is still
missing. Local raw proof is retained for the coordinator, including the
reviewer's mutation receipts at `/workspace/partition-review-evidence`.
