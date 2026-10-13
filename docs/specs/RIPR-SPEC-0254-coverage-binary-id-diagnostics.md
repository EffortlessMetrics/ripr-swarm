# RIPR-SPEC-0254: Bounded coverage binary-ID diagnostics

Status: proposed

Owner: repo-policy / coverage diagnostics

Created: 2026-10-13

Linked issues: #4978

Linked PRs: #7579

Standalone reason: #4978 records failed coverage diagnostics from the CI
coverage adapter. This contract governs only the additive diagnostic evidence;
it does not classify coverage as successful or identify a test, process, or
cause.

Support-tier impact:

- None. This is unpublished repository automation output.

Policy impact:

- Additive fields in the CI failure manifest only. No hosted workflow, required
  gate, profile input, profile retention, or original failure outcome changes.

## Behavior

The adapter may read at most the bounded prefix supplied by its caller and
recognizes only LLVM raw64 little-endian version 10 profile ID blocks. It keeps
at most 32 unique IDs of at most 128 bytes each. If a later ID exceeds that
count, already parsed IDs remain available, `binary_ids.status` is `partial`,
and the reason is `id_count_limit`. A partial profile makes the aggregate
mapping partial; unmatched retained IDs are not reported as definitive misses.
An unavailable profile-ID input also prevents definitive misses for IDs from
other profiles. Malformed or unsupported data remains unavailable or partial
and never discards the original profile or changes the recorded failed coverage
result.

Executable associations inspect only direct executable files under the four
declared coverage build roots. A candidate requires an exact GNU build-ID
match and allocated LLVM profile data and counter sections. A structurally
parsed executable without those sections is a definitive noncandidate and does
not make unmatched IDs unavailable. Unreadable, malformed, unsupported,
rebound, or over-budget inputs remain unavailable and prevent a definitive
negative result for unmatched IDs. Duplicate candidate paths remain
ambiguous. This evidence records identifiers and bounded file metadata only;
it makes no test-ownership, process-termination, or root-cause claim.

The scan retains its existing limits: 1,024 entries, 256 files, 2 MiB total
reads, 64 KiB per file, 16 matches, 32 KiB serialized JSON, and two seconds.
Mapping failure cannot abort profile retention or replace the original
coverage failure.

## Required Evidence

- Parse 33 valid IDs and verify the first 32 are retained with explicit
  partiality; verify a retained exact match remains visible and absent IDs
  remain unavailable under partial scope.
- Mix an instrumented wrong-ID executable with a valid uninstrumented
  executable; verify the latter is counted as a noncandidate and does not
  degrade a definitive negative match.
- Keep malformed/unsupported ELF controls unavailable and verify the original
  failed-profile manifest is preserved.
- Exercise exact ID equality, duplicate ambiguity, root and symlink boundaries,
  and each scan limit.

## Non-Goals

- Determining the historical coverage producer or cause of a short profile.
- Changing instrumentation, profile bytes, coverage thresholds, or CI policy.
- Scanning outside the four declared build roots or collecting whole binaries.

