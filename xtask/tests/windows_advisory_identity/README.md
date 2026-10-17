# Owner-pin captured diagnostics (#8445)

`owner_pin_execution-39023007917.log` is the literal contiguous 83-line
segment retained in [issue #8445](https://github.com/EffortlessMetrics/ripr-swarm/issues/8445).
It was copied from decoded job-log LF-split positions 27634–27716, sample 1,
10:35:32–10:36:20 UTC on 2026-10-17. Timestamps and ANSI bytes are retained.
SHA-256: `8229096c59523e6bb2a81d2159842844a34b171ce16871a68c56460c72cf41f0`.
The successor independently fetched the decoded job log and verified exactly
one match at those positions after converting its CRLF line endings to LF.
The retained file is that LF representation, not the original download bytes.

Historical execution: [Windows Advisory run 39023007917, job 117368411978](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/39023007917/job/117368411978).
Requested head: `5ee4e2425164f3b2399c2deac96ee11093b4b6bb`.
Actual checkout: `79630d582033d276a9a24fc573e9ae696b055e68`, merging that head
into `d1f87dff46b999437ea3ce0dd34fad514cbe90da`.
The issue records observed totals 16,359 pass/64 fail and 16,358 pass/65 fail;
this retained segment and these tests do not qualify either whole sample.

`projection()` removes only the leading timestamp plus one separating space
from each line; the actual summarizer removes ANSI. The projection is parser
input, not a second claim of raw job bytes. `streams()` extracts the original
captured child stdout/stderr. The framing tests invoke the shared assertions
used by the real owner-pin test and replace only the historical panic payload
with their actual caught message. The historical location/header, outer rows,
failure list and completion remain fixture bytes. Added control-looking text
and the missing-subject variant are explicitly synthetic.

The command harness adds its existing independent 15 seam-control subjects and
passes explicit Cargo status 101 for both sample artifacts. A usable framed
report must contain 47 passes (15 controls + 32 historical outer passes) and
exactly one outer failure. The child supplies zero owning subjects. Summarizer
exit zero means usable advisory evidence; both sample states must remain
`nonzero_with_observed_test_failures`. Raw capture must still be incomplete.
This is a cross-platform parser/producer regression, not native Windows repair
or evidence that the underlying runtime assertion now succeeds.
