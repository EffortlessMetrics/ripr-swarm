# Uncertain-delivery helper attribution witness

Tracks [#4478](https://github.com/EffortlessMetrics/ripr-swarm/issues/4478) and
[#7818](https://github.com/EffortlessMetrics/ripr-swarm/issues/7818).
`github_delivery.rs` is the entire unchanged source from
`EffortlessMetrics/ub-review` PR #1440 head
`bcc008f67cbe9753ede6daf8a2eb914de42adfad`, blob
`e29796eaa0181ffb9a00e0462985e3c1ad72c857`. `receipt.json.full_diff` preserves its full source-file
diff from `2b8e66ebe5f11608fb9e85da4af7c5747252f86f`. Preserve the real helper,
both callers, transport factory and native `ensure!` assertions. The diff is
JSON-encoded to retain exact context whitespace without whitespace lint noise.

Run the public API witness:

```sh
cargo test -p ripr --test uncertain_delivery_helper --locked
```

The API test materializes this file under a minimal library manifest. It is a
one-file parser projection, not a standalone compilable UB workspace or a
simulation of GitHub delivery. It requires nonempty production findings and
both real caller rows. It pins no production probes in the cfg-test helper
and no oracle credit for its callers. Source lines are resolved from the
frozen source instead of treating historical line numbers as identities.

The argument/return/observer chain retains one limitation and one positive control:

1. Both native tests tail-return `uncertain_delivery_retry(false/true)`. The
   helper defines and invokes `make_transport`, then observes delivery results,
   receipt existence and `retry.sends.len()` before returning `Ok(())`.
   Same-file helper admission currently refuses a helper containing any
   closure. Its calls and assertions never widen the callers. The local
   mutable transport and fallible helper result are not established observer
   links for the changed production owner. Recognition of `ensure!` exists.
2. A reduced plain fallible helper copies its `gate(input)` call and
   `assert_eq!` oracle, and since #8089 the calling test is credited with that
   panic assertion (`Exposed`, exact-value oracle). This replaces the earlier
   downstream `could not identify the test function uniquely` refusal. Direct
   in-test observation remains the positive control. Sibling, uninvoked and
   wrong-binding controls cannot expose the changed owner.

These are limitation witnesses, not an endorsement of missing assertions.
Do not broadly relax closure, binding, execution or effect attribution based
on them. Any repair must establish the exact invocation and observer chain.

`receipt.json` pins a separate full-workspace CLI comparison on the exact UB
head and diff. Current-main producer `459ad8f2c` emits 40 findings and no helper
probes; downloaded release 0.10.0 emits 145 findings including 75 helper-span
probes. The listed construction/return rows are historical producer output,
not current-main failures. Report hashes identify retained raw reports; the
same command regenerates them from the original consumer checkout. The
historical hosted gate used merge checkout `64614cd2`, reports 13/max0, and is
not that head-side CLI replay. No gate clearance or analyzer adoption follows
from this comparison.

Native proof was run in the original full UB workspace, separately from the
API projection: all 32 delivery tests pass, zero ignored. Ignoring errors from
the two production `reserve_delivery_attempt` calls compiles and makes each
real retry caller fail with `retry sent duplicate review`; restoring the
source passes all 32 again. The reduced direct/plain/factory/sibling runtime
controls reject the boundary mutant; uninvoked and wrong-binding controls
survive it. Runtime discrimination and static admission remain separate.

This witness changes no analyzer production code, thresholds, suppression,
dependency pin, UB code or assertion. It does not alter #7805 value provenance,
#7794 sibling effects, #1728 derived/terminal/tuple flow, or #7809 MatchArm
admission. Changelog exemption: test-only inputs and regression witnesses with
no product, workflow or public documentation behavior change.
