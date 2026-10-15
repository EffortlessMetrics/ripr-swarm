# #4987: mandatory qualification critical path

## Result and scope

Investigation for Steven's authorized release-readiness campaign, observed
2026-10-15. Source basis: `db61ee58ae923bcf3289fe16860e3a0400579270`, tree
`c08052c0b8b0a9e8485a97222d339c8cd6b7a4dd`. No demonstrated universal
20-minute qualification exists. The two completed reference runs were different
candidates and populations, not a performance-repair comparison.

This branch publishes evidence only. No product/workflow repair, PR, Ready
transition, dispatch, rerun, merge, release, settings change or hosted
qualification was performed. A branch push is the intended delivery boundary.
Source proof, historical hosted evidence, and future qualification are distinct.

## Current ownership and mandatory route

Refreshed [#4987 and its substantive comments](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987)
and all-state claim-specific PR searches before branch creation. Preserve
#4986 admission, #4836 actual-G/evidence, #4988 runners, #7827 cache continuity,
#4841 economics, and #3794/#3796 semantic-reuse ownership. #7918 already landed
the CI composition repair: `ci-policy` retains local precommit semantics while
the workflow runs one explicit full Clippy obligation. Its earlier measured
warm prefix was slower by 4.369448 seconds; do not claim that removing an
invocation saved a full compile. PR Plan and Ready advisory-review retirement
are already delivered. No rival admission or Windows implementation is proposed.

Current source still requests required qualification on `ready_for_review`
and main push, with manual dispatch retained. Explicit late admission/actual G
is the target migration, not activated main behavior. The PR-only evidence
block and advisory proof-route dry-run cannot qualify a merge-group subject.
The selected required Rust job runs serially after overlapping route/docs jobs:
setup; cheap policy; full ci-policy; honesty/dependency/process/network checks;
default and all-feature checks; full Clippy; full default nextest; doctests;
goldens; fixtures; verdict corpora; PR evidence; optional reports; aggregation.
Its stable protected result is `Ripr Rust Small Result`.

All-feature compilation, default runtime, non-default runtime, Windows, wheel,
release/package and editor obligations retain their distinct applicability and
denominators. The default required job alone does not establish that portfolio.
The source-review hold on Windows evidence in
[comment 6183220409](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6183220409)
is separate, remains with that candidate owner, and is not waived by timing.

## Completed hosted observations

Primary job logs were fetched again in this investigation:
[older job 115553677750](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38472100072/job/115553677750)
and [newer job 116078910676](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/38629886041/job/116078910676).
The newer run's job/step inventory was also read back. The complete original
[timing/identity report](https://github.com/EffortlessMetrics/ripr-swarm/issues/4987#issuecomment-6182558974)
retains queue, attempt, parallel-cohort and platform qualifications.

| Measured component | Older #7720 | Newer #7982 |
| --- | ---: | ---: |
| Run creation to terminal | 3145 s (52m25s) | 2078 s (34m38s) |
| Implementation job wall | 3112 s | 2017 s |
| Job start to policy entry | 54 s | 65 s |
| Policy, including driver bootstrap | 506 s | 268 s |
| Workspace / all-feature / Clippy | 74 / 67 / 56 s | 70 / 68 / 102 s |
| Required nextest step | 1506 s | 1035 s |
| Outer test compilation | 182 s | 174 s |
| Logged test execution | 1321.230 s | 859.265 s |
| PR evidence | 747 s | 250 s |
| Optional reports | skipped | 60 s |
| Parsed PASS records / skipped | 15398 / 2 | 15875 / 2 |
| Sum of PASS durations | 5157.305 s | 3332.946 s |

Older checkout: `ce237e6c3087b95a33133e9bfefb80548a9c6a75`, tree
`d1b54bced1784089d5df2084f45f9efdc6500795`; PR H
`9ca57fa6a5c08323cebfaf22a4d4d5569f1d17c6`.
Newer checkout: `4fc71a48b55b83640c997d46f67bec3181716efe`, tree
`f8df94e7804e87fea74d0d2265ba0e5b081ea00e`; PR H
`bb3395e29a13e19b426759d4fef2bacc3f60a0d0`.
Both are hosted Linux/Rust 1.95.0, build jobs 8, default nextest ci;
neither qualifies this branch/current main or actual G.

PASS-duration sums overlap and can include nested child work; they are neither
CPU consumption nor extra elapsed time to add to the nextest step. The two
longest newer tests were suppression-denominator notice (163.437 s) and
no-unchanged-tests recall notice (121.088 s). Those are better next focused
fixture-inspection subjects than already-optimized six-case sample-output
tests. A slow test is not evidence that its assertions or setup can be removed.

## Ranked changes and evidence limits

Ranked by potential effect on mandatory elapsed time, rather than invocation count:

1. **Reduce/partition actual nextest runtime.** It dominates at 859.265–1321.230 s;
   the older execution alone exceeds 20 minutes. The existing proposal is one
   compatible build/archive and four native partitions, preserving the complete
   nonoverlapping union, ignored tests, features, reports, toolchain and exact
   integration identity. A 480-second longest-shard design budget would save
   379.265 / 841.230 seconds of execution if achieved. This is a conditional
   delta, not a measured speedup. Build/transport/private extraction and native
   shard balance remain unmeasured; extra starts and occupancy must be counted.
   Existing slow CLI fixture owners should first test whether broad repository
   setup can be replaced by a discriminating small workspace.
2. **Reuse compatible PR evidence and attribute its expensive children.** The
   measured 250 / 747-second step is material. #4836 retains a separate
   1636.375-second workload with 1389.269 seconds after canonical production;
   that cannot fit 20 minutes unchanged. Ten shell commands are not ten analyzer
   runs: `--check` consumers validate retained output. #3796's exact-base and
   semantic dependency reuse needs cold/warm parity and changed-test/config
   invalidation before any numeric saving is earned. The observed step is the
   maximum possible local saving ceiling, not an estimate of removable work.
3. **Earn genuinely warm compatible compilation.** Both reference cache restores
   succeeded (14 / 21 s), yet driver bootstrap took 93 / 85 s and outer test
   compilation 182 / 174 s. Eliminating all those compilations has a theoretical
   275 / 259-second ceiling, before cache/transfer costs; actual removable work
   is unproved. #7827 owns cross-subject writer/read scope and cold behavior.
   Do not simply enable cache writes or share mutable Cargo targets. Verdict
   corpora's 53.78-second Perl-feature build is different from the default
   fixture build and is not an established duplicate.
4. **Stop repeating the full fixture population.** `fixtures_impl` and
   `collect_golden_runs` in `xtask/src/reports/fixtures.rs` enumerate the same
   directories, validate each contract, run JSON/human/full surfaces and compare
   outputs. Goldens additionally validates assistant-loop-health. Log group-start
   spans from `fixtures` to the next step are 7.291 / 7.537 s, including shell
   transition overhead: those are upper bounds for avoiding the second pass,
   less report-generation cost. A shared collection must still produce both
   report families, preserve contract/drift failures and missing-directory
   refusal, and bind both canonical obligations to the actual producer outcome.
5. **Deduplicate remaining formatting/dependency/process/network work.**
   `precommit_policy(true)` calls formatting and dependencies; `check_policy_all`
   includes process/network before the standalone workflow steps. The complete
   dependency-through-network spans are only 2.945 / 2.364 s, including shell
   transitions. They are current-run identical source-policy candidates, not
   another whole Clippy saving. Formatting's standalone cost was not isolated.
   Preserve individual reporting and calendar-sensitive dependency expiry; a
   prior-day policy pass is not an unrestricted reusable receipt.
6. **Keep optional reporting off the mandatory critical path where applicable.**
   The newer full-ci request spent 60 s in optional reports. Their scheduling
   can save at most that sample's tail; this is not mandatory coverage removal.
   The parallel editor/full-ci fan-out affects occupancy, not the sum of serial
   protected elapsed time. Coverage/analytics failures and cancelled Windows
   work remain non-passes and cannot be turned into warm qualification evidence.

## Bounded repair decision

Independent source review identified shared fixture collection as the strongest
small repair. Further inspection found the canonical product plan requires
unique commands per gate (`definitions_have_unique_stable_ids_and_commands`),
and the workflow contract explicitly requires separate producer/outcome rows.
Changing only YAML or reading a prior Markdown success would weaken that contract.
The correct repair therefore also needs canonical producer mapping, report
parity and executable failure/execution-count controls. Its observed ceiling is
7.537 seconds (under 0.4% of the newer run). This investigation does not justify
that wider contract change as the performance repair for the 20-minute goal.
No speculative production patch was committed. The bounded implementation
design above is available to the existing owner; no new selector/controller,
receipt authority or generic scheduler is needed.

The newer nextest + policy + PR evidence already total 1553 seconds (25m53s),
before other gates. The shortfall is at least 353 seconds plus remaining work.
Even removing both measured repeat spans saves at most 9.901 seconds in that
sample. These source cleanups alone cannot plausibly establish the target.

## Reuse and next qualification

Reuse dependency/compiler caches only with compatible toolchain, lock, feature,
target/profile/build configuration and trust scope. Preserve private outputs.
Keep the unconditional Cargo freshness check and per-run fixture cache clear
(#3054); sharing warm facts among the three surfaces inside one fresh execution
is already implemented. Do not infer freshness from binary existence or cache
restore success. Keep original identities for historical proof; bind reused
semantic evidence to the same validated subjects/dependencies, not just H.

The previous conditional design budget remains 120 s admission/setup + 300 s
policy + 720 s maximum parallel proof branch + 60 s reconciliation = 1200 s.
It requires actual measurements. Newer test compilation leaves only 66 seconds
of a 240-second build/archive/transport budget. Older evidence 747 s, the separate
1636.375-second evidence workload, Windows subset 973 s, release 1028 s, coverage
2309 s and analytics 1287 s exceed the proposed 720-second branch unchanged.
Preserve applicable obligations; report a miss instead of shortening coverage.
Queue, retries, orchestration and recovery gaps must be reported separately.
Use the next already-required qualifying execution for new timings, not an
extra broad canary. No performance guarantee or source-cutover readiness is claimed.

## Local proof, independent review and retention

Exact-base `cargo check -p xtask --all-targets` passed on Linux/Rust 1.95.0 in
108 seconds, with dependencies fetched as needed. The offline attempt refused
uncached locked tokio-util 0.7.20; that is provisioning evidence, not a failed test.
The unchanged dependency-free `xtask/tests/rust_gate_workflow_contract.rs` was
compiled directly using `rustc --edition=2024 --test`: all 9 tests passed,
including negative controls for omitted, repeated, suppressed and fabricated
gate outcomes and narrowed Clippy. This is focused source-contract evidence,
not Cargo/nextest execution or complete policy/qualification proof.

An independent reader inspected production consumers, duplicate predicates,
freshness controls and the feasibility arithmetic against `8b7a9ba4a7ca2f1212fc3b456d0a2bee876a5e55`.
All relevant reviewed owner files are blob-identical at the source basis above.
Its review does not independently execute the hosted jobs or qualify a repair.
Report review and final candidate identity are recorded in the delivery handoff.

The compact local packet is `/workspace/4987-fixture-proof/`: native logs,
workflow-contract executable and hosted-extract.json (all PASS counts, summed
durations, group timestamps, Cargo completion lines and slowest tests). Extract
SHA256: `652d2ac596f75f46ccd843b43dd51350506b06bd8aecb6b0bc2ab425a35d2c21`.
Original remote job links above are the receiver-readable primary sources;
the packet is supporting local evidence, not a new qualifying receipt.
The task reused the existing target and added approximately 0.93 GB of checked
build state, below its 4 GiB added-state budget; no large archive or new target
was allocated. Existing shared artifacts are retained and not deleted here.

CLI GitHub API/GraphQL reads returned Forbidden while authorized GitHub connector
issue/comment/search/job/log reads and ordinary Git fetch worked. Organization
required workflows/provider settings are not fully visible; absence is not
inferred. Existing protections and item-specific holds remain intact. #4987
stays open; the delivery owner retains the portfolio migration and qualification.
