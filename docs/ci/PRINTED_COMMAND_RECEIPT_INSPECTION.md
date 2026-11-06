# Source-only printed-command receipt inspection

`repo_policy::printed_command_receipt::inspect` is a pure, unwired observer of
`ripr.printed-command-terminal.v1` and its exact JSONL inventory. It performs no
network or file I/O and cannot authorize, reuse, skip or publish any result.
Every report has status `NotEstablished`, including one with no structural
findings. Disclosed known gaps remain attached to their executed cases, and
PowerShell exclusions remain explicit. A main replay test completing with
known gaps is not an unconditional correctness result.

The caller independently supplies the selected repository, H/B/G/tree/workflow
generation, request, run and attempt, and the original whole-route 1200-second
budget. Exactly one Ubuntu, macOS and Windows cell is required. Required shell
sets are pinned to the reviewed workflow matrix, independently checked in tests,
and cannot be reduced by receipt fields. Additional recognized shell records
are inspected and retained if present.

Each cell supplies receipt bytes, inventory bytes and a normalized closed JSON
observation (`ripr.printed-command-observation.v1`). The observation names the
same subject, workflow, job key and OS, completed-success job/replay/upload
outcomes, successful entry/completion deadline outcomes, ordered completion
times within the selected budget, and distinct artifact IDs, exact artifact
names, same run/attempt labels and SHA-256 of both byte streams. See the typed
`Observation`, `JobObservation`, `DeadlineObservation` and `ArtifactObservation`
structures and the fixture builder in the module's tests for the exact format.
This is an adapter input schema, not a GitHub API response or attestation.

Closed typed decoding rejects unknown, duplicate and missing fields, non-UTF-8,
nonfinite numbers, fractional or boolean case IDs, and trailing JSON documents.
Each receipt and inventory is bounded at 16 MiB, each normalized observation at
64 KiB, and inventories at 15 through 100,000 sequential cases. Every supplied
shell must account for its complete inventory-derived execution/exclusion set;
known-gap IDs must be unique executed cases with nonempty reasons.

## Evidence still missing

All input data, including separate hashes, artifact custody labels, job statuses
and timestamps, can be fabricated by an untrusted caller. Their structural
agreement does not authenticate them. A future protected adapter must retrieve
current same-run artifacts and job/step outcomes from trusted services, bound
retrieval before buffering, verify actual source/G and workflow generation, and
preserve ownership/currentness through use. Inventory artifact names themselves
contain no run or attempt. This helper cannot establish completeness of printed
command discovery or the upstream known-gap classifier and ledger. No workflow
invokes it, and it does not change the existing result reconciler.

Focused local controls:
`cargo test -p repo-policy --lib printed_command_receipt`.
