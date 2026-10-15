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

Affected-head independent review at 2026-10-15 04:08 UTC inspected repair
`1937cf1b601097689dbdb13ce3b252f6fdfbee1d`, tree
`ecd3be3668e8be8ea794fe274728a5882e92abb7`. Disposition:
**REVIEW_INCOMPLETE; no blocking findings for inactive branch publication**.
The reviewer independently ran all six tests and both literal-filename CLI
forms (with and without a later real receipts marker), observed exit 0,
checked the Clippy log and source hash, and confirmed the appendix's
carried-forward mutation scope. The parser finding is repaired. This review
does not establish merge/activation readiness, balance or <=20-minute proof.
The publication-only receipt update leaves that reviewed Rust source and
configuration unchanged.

## Task 17 proof extension (2026-10-15 UTC)

Resume candidate `2272722f60549910120bdedc279590bb5cae5d92` on disjoint branch
`codex/partition-result-controls-task17`. Acceptance: exercise the retained CLI
argument/file/JSON path with a valid nonempty control, duplicate/absent/failed
shard results and wrong binary/test subjects. Reject duplicate passed-subject
rows before collection normalization can erase them. Preserve inactive status,
existing caller, workflows, doctest obligation and caller-supplied trust boundary.
The duplicate-row control must fail against the inherited implementation before
the scoped repair. Existing function-level controls remain useful separately.

Task-owned resources: clone/common Git directory
`H:\Temp\ripr-task17-partition[\.git]`; proof/temporary outputs
`H:\Temp\ripr-task17-proof`; reviewer scratch, if needed,
`H:\Temp\ripr-task17-review`. No shared build directory is writable by this task.
H: had approximately 2.4 TB free at discovery; conservative additional budget is
1 GiB for clone, small prototype executables and fixtures, with no full product
build admitted. Cached dependency artifacts are read-only inputs from
`H:\Temp\ripr-task5-build\debug\deps`; retain their hashes and compiler identity.
Queue heavy Cargo baseline/policy gates while existing heavy builds are active;
direct prototype-only compilation establishes neither those gates nor full
Cargo dependency qualification. Retain exact baseline/negative/current witnesses
and logs through review and parent handoff; no unrelated cleanup is authorized.

### Task 17 observed proof

Native Windows PowerShell, `rustc 1.95.0 (59807616e 2026-04-14)`,
`x86_64-pc-windows-msvc`. Baseline six tests passed (exit 0). After adding the
CLI controls, the corrected pre-repair suite ran eight tests: seven passed;
`cli_refuses_duplicate_passed_subject_rows` failed because aggregation returned
`Ok(())` for a repeated passed-subject row (exit 101). An earlier harness run
also had an incorrect duplicate-shard diagnostic expectation; that test-oracle
error was corrected before retaining the isolated red witness.

The repaired source SHA256 is
`71363f5ccfdf7b0337018436aeee9bb6d323743dac58560dfad7389a9e674b05`.
All eight tests pass, zero ignored/filtered, exit 0. Independent literal JSON
receipts exercise three subjects in two binaries, including the same local test
name; a reordered two-subject shard still passes. Negative cases preserve
otherwise valid inputs to isolate duplicate receipt IDs, absent receipts,
nonzero exits 1/100/124/130, incomplete/stale state, absent passed subjects,
wrong binary, wrong test name and duplicate passed rows. The earlier unit
control for duplicate receipt IDs now copies the complete first receipt, so
subject mismatch cannot mask removal of the distinct-shard check.

Six separate native CLI processes also ran: valid input exited 0; duplicate
passed row, absent shard, duplicate shard, failed shard and wrong subject each
exited 1 with the expected aggregation diagnostic. This confirms the real
entry point's status behavior, not only `run(args)` assertions. Rustfmt 1.95.0
on the changed Rust source and `git diff --check` pass.

Focused compilation deliberately avoided Cargo's product dependency rebuild.
Reproduction from the repository root (PowerShell; add `--test` and select a
test-output filename for the eight-test executable):

```powershell
$deps = 'H:\Temp\ripr-task5-build\debug\deps'
rustc +1.95.0 --edition=2024 xtask/examples/mandatory_partitions.rs `
  --crate-name mandatory_partitions -L "dependency=$deps" `
  --extern "serde=$deps\libserde-721fe3bf8bf6b84c.rlib" `
  --extern "serde_json=$deps\libserde_json-05e5baa929c68c83.rlib" `
  --extern "toml=$deps\libtoml-55f65ee380479082.rlib" `
  -C debuginfo=0 -o H:\Temp\ripr-task17-proof\mandatory_partitions.exe
```

The three direct rlib SHA256 values, captured before use, respectively are
`2a5287b983a38cc35cf7e52163132ab51b4e2d61b485f8007f6deeec64d4984c`,
`1b706340f3618f1050f4247724210ef5d7570d18c4ed4ee8da1c5d874ac7e1f9`,
`d30ee4c0749df88d481734672c61c4c70545a298f7abff074b1abacfd95474cc`.
These read-only artifacts were accepted by the pinned compiler; their reuse is
not a new source/dependency or Cargo qualification. The current test executable
SHA256 is `42f030bd9f818024655bbdd2860a7ceed9c70bcc8208dce3b10449de0be34780`;
the isolated pre-repair test executable SHA256 is
`ebdc802bc30379f510e0d8ac6ecd7290e67af449bae92aa8b700a4cb3beaf020`.

Exact native fixtures, logs, hashes and executables are retained at
`H:\Temp\ripr-task17-proof`; this committed receipt and test source are the
portable deliverables. Baseline `worktree doctor`, `check-fast`, narrow Cargo
compile, precommit and full hosted qualification have not run in this Windows
lane: multiple existing Cargo/rustc builds remained active, and rebuilding
xtask's product dependency stack would be expensive. No readiness, balance,
native product-suite, nextest timing, trusted receipt or activation claim is
made. Original Linux evidence above keeps its original subjects and epochs.
