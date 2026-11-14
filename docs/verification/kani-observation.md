# Kani native observation journal

Owner: #14472 under #12109 / #12904. Normative lane:
[RIPR-SPEC-0371](../specs/RIPR-SPEC-0371-kani-bounded-model-checking-lane.md).
This implementation continues the existing `cargo xtask kani` runner and the
#14469 cover-admission candidate. It does not replace #14478's document auditor.

## Implemented observation boundary

The same process owner, command construction, native-block join and verdict
classifier remain in `xtask/src/kani_lane.rs`. Its private `machine` module owns
one typed report and its JSON/Markdown projections. No second verifier executor,
claim registry, native-result classifier or public product schema is introduced.

The internal envelope uses `ripr.kani-observation.v1`, not the proposed complete
`NativeProofRunV1` claim contract. It records what this runner can observe now.
Every report has `registered_claims = 0` and `claim_assurance = not_established`;
rows have null claim IDs/generations. Harnesses must not silently become
registered propositions merely because their native output looks successful.

The existing command accepts the same arguments and writes:

```text
target/ripr/reports/kani.json
target/ripr/reports/kani.md
target/ripr/reports/kani-runs/<run-id>/checkpoint-<n>.json
target/ripr/reports/kani-runs/<run-id>/checkpoint-<n>.md
```

The [schema](kani-observation-v1.schema.json) defines the envelope shape.
The [example](kani-observation-v1.example.json) is a synthetic not-started
stimulus, not output captured from Rust or Kani. Schema conformance alone does
not authenticate a producer or establish a successful verification.

## What is captured

The report includes the parsed request, selected package/file/harness records,
SHA-256 commitments to the exact UTF-8 proof files seen by discovery, a sorted
source-observation commitment, and before/after proof-file equality. It records
preflight and per-package arguments, their actual deadline allowance where the
existing API provides one, elapsed duration, numeric exit and exit-success
observations, timeout and capture errors.

Both process streams retain the strings returned by the existing process
capture helper, their UTF-8 byte lengths and hashes. These are **captured text**,
not a claim to preserve original pipe bytes: that helper may decode lossily.
Native block names and normalized block commitments come from the existing
splitter. Cover summaries reuse the existing pinned-format parser, including
satisfied, total, undetermined and unreachable counts. No independently
maintained cover or verification parser is added.

Source-file names do not establish native module identity. The current
conservative unique-name attribution remains visible, including its limitations.
A selected harness with no observation stays in the result denominator, even
when the tool cannot start. Unexpected native blocks are retained explicitly
and prevent a clean lane result rather than disappearing from the denominator.

## Result and failure law

Admission requires equality between the unique selected harness-file set and
its observed source-file set, with exactly one observation per file. Missing,
extra, duplicated or substituted observations refuse clean lane success even
when all observed files recheck successfully. Several harnesses in one file
legitimately share one file observation; harness count is not file count.
`--allow-empty` does not waive this invariant. This check closes an internal
construction-divergence gap; it does not establish complete native discovery
or the source/configuration closure consumed by the verifier.

The existing lane verdict is authoritative for native row classification.
The reporting layer can refuse incomplete observations; it cannot turn a
failed native lane into success. A completed row survives a later process
failure or timeout, but overall `lane_success` remains false. A proof-file edit
or failed recheck likewise refuses the aggregate without erasing the historical
row. Markdown and JSON share those exact rows and the overall lane disposition.

`lane_success` describes this bounded internal command only. It is not a current
registered claim, proof of exact native subject identity, specification adequacy,
CI admission or permission to merge. An explicitly allowed empty run still has
zero registered assurance. Unknown source custody and process-tree cleanup are
not filled with optimistic defaults.

## Publication and stale-file handling

A run acquires an exclusively created directory. The first checkpoint is
published before discovery or tool execution and is explicitly incomplete.
Further checkpoints follow selection, tool discovery and each package. A final
checkpoint records the terminal lane decision and proof-file recheck.

Each checkpoint JSON contains its typed report, inline Markdown, Markdown hash
and immutable Markdown path. The producer round-trips the DTO and compares the
Markdown commitment before publication. Files use exclusive creation and file
sync. Convenience aliases use a same-filesystem staged rename, with `kani.json`
published last as the commit point. This is not a multi-file filesystem
transaction or a power-loss durability claim.

A consumer must bind the expected run ID from the invocation, check phase and
lane disposition, and verify the inline Markdown/hash or the named immutable
pair. Reading `kani.md` beside an unrelated `kani.json` is insufficient.
Concurrent invocations have distinct directories; aliases are **last published**,
not a declaration that the most recently started run is current. A publication
failure makes the command fail. Failure before the first marker can leave an
older alias; no consumer may interpret mere file presence as the new run's proof.

The existing workflow's artifact includes the self-contained JSON and Markdown
aliases. Inline Markdown and captured text make the JSON inspectable without
uploading every historical checkpoint. Checkpoints remain local diagnostic
history. A workflow upload is not producer authentication or retention policy
for every local run.

## Resource and custody limits

Captured-text retention is capped at 16 MiB per stream and 32 MiB per run. A larger
stream retains its byte count/hash, omits the text and refuses a clean evidence
result. These are retention bounds, not bounds on the existing subprocess
capture's allocation, source scan or total runtime. Native block metadata and
JSON escaping have their own overhead. No peak-memory saving is claimed.

The original 4 MiB/8 MiB envelope did not fit the actual pinned-provider output
in hosted run 43428466597: the `ripr` and `ripr-contract` stdout observations were
14,616,478 and 4,198,103 UTF-8 bytes. Both were omitted, and that historical run
remains NOT-ESTABLISHED. The larger bounded envelope retains the same inline
text/length/hash shape and refusal law; it does not restore historical bytes,
change native verdicts or confer assurance. Tests use those byte counts with
synthetic text, not a fabricated provider replay.

Capacity qualification must account for all immutable checkpoints and aliases,
not only the final compressed artifact. JSON escaping can expand retained text
by up to six times: the 32 MiB retained-text budget alone can contribute up to
192 MiB per JSON document, before metadata, Markdown and source observations.
The observed two-package run published checkpoints 0 through 5 plus the alias;
that shape can therefore contribute up to 1,344 MiB of escaped captured text on
disk. Other invocation shapes must use their actual checkpoint count. This is
an arithmetic component bound, not a measured whole-process memory/disk bound:
DTO cloning, serialization, round-trip parsing, native metadata and the existing
subprocess capture all remain additional costs. Over-limit capture still omits
text and refuses success, with package/stream/byte/limit diagnostics identifying
whether the per-stream or aggregate retention budget was exhausted.

Selected proof-source observations are bounded to 8 MiB per file and 64 MiB total;
rechecks use bounded reads and reject non-regular final paths. The original
scanner still owns discovery and its allocation. A before/after equality check
cannot rule out intervening changes or prove which bytes the compiler consumed.
The complete helper/dependency/configuration/feature/target closure remains
unestablished, as do independently authenticated provider and solver identities.

Checkpoints duplicate bounded captured text. Retain the exact failing/current
run needed for review; after acceptance or explicit handoff, use the repository's
existing storage closeout to remove only confirmed lane-owned obsolete run
directories. This code does not delete previous runs or introduce a new timer.
Automatic bounded retention is not claimed.

Preflight timeout composition remains with #12581. Its existing unbounded
preflight is reported with no invented deadline. Do not call this slice a
complete bounded execution programme or close #14472 on that basis.

## Maintenance and validation

Use the repository-pinned toolchain. First exercise the new pure observation
controls, the unchanged parser/native-fixture controls and the actual synthetic
process tests:

```bash
cargo test --locked -p xtask --bin xtask kani_lane::
cargo test --locked -p xtask --test kani_lane_process --test kani_machine_report
cargo clippy --locked -p xtask --bin xtask -- -D warnings
cargo fmt --check
cargo xtask check-traceability
cargo xtask check-process-policy
cargo xtask check-workflows
cargo xtask check-doc-index
cargo xtask ci-policy
```

These commands are qualification instructions, not assertions that they have
run. Source/test execution receipts belong to the exact candidate. The process
fixtures use a synthetic cargo child: they prove report/exit/identity behavior,
not native solver success. Preserve #14469's pinned native captures and execute
real Kani separately when qualifying the composed producer.

Regression controls must preserve positive output and challenge stale success,
missing tool/result, extra native subjects, ambiguous and duplicate identities,
source edits, partial timeout, retained-text limits and JSON/Markdown agreement.
The `machine::inventory_tests` module specifically challenges a two-file
selection with one missing commitment, extra and duplicate observations,
same-count wrong-file substitution, a valid shared-file selection, ordering,
empty opt-out and the independent changed-source refusal. These controls use
actual file rechecks and synthetic native observations; they are not solver runs.
A compile failure is not the intended behavioral negative control. Keep failing
results distinct from an unavailable execution environment.

Before protected integration, compose the existing #12581 preflight and #14469
cover work, retain current-main workflow event retirement, then qualify the
combined source using the accepted queue route. This slice changes artifact
payloads, not workflow subscriptions, required contexts or settings.

## Remaining programme work

#14472 still owns full consumed-source/configuration custody, registered claim
binding when available, independently observed provider/toolchain identity and
production acceptance of its complete native-run contract. #14473 owns the
registry and current-source/result assurance scoreboard. #14474 / #14500 own
safe admitted-group selection and execution topology; #6452 / #6456 own proof
weakening/replacement/retirement. The user-facing #12596 / #12597 relationship
and installed journey are independent. Reuse this producer rather than adding
another command runner or interpreting the offline auditor's exit as proof.

Rollback is an ordinary protected revert of this child. It preserves the
parent's cover-admission and existing process behavior and does not delete
historical artifacts or change branch protection.
