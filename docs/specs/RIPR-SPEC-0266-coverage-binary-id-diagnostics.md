# RIPR-SPEC-0266: Bounded coverage binary-ID diagnostics

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

- None.

This spec concerns unpublished repository automation output; it does not
change any language or platform support tier.

Policy impact:

- Additive fields in the CI failure manifest only. No hosted workflow, required
  gate, profile input, profile retention, or original failure outcome changes.

## Problem

The retained #4978 failure profile has a binary ID without a retained executable
association. Future failure diagnostics need a bounded association that preserves
unavailable evidence and ambiguity without attributing a test or cause.

## Behavior

The adapter may read at most the bounded prefix supplied by its caller and
recognizes only LLVM raw64 little-endian version 10 profile ID blocks. Each
profile retains at most 32 ID entries of at most 128 bytes each. If a later
entry exceeds that per-profile count, already parsed entries remain available,
`binary_ids.status` is `partial`, and the reason is `id_count_limit`. The
aggregate mapping deduplicates across captured profiles and retains at most 32
unique IDs total; further unique IDs set `profile_ids_truncated` and make the
aggregate mapping partial. A partial profile makes the aggregate mapping
partial; unmatched retained IDs are not reported as definitive misses.
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
negative result for unmatched IDs. Distinct instrumented candidates with the
same matching ID remain ambiguous; duplicate directory entries of one
executable, such as cargo's hard-linked `debug/<name>` copy of
`debug/deps/<name>-<hash>`, collapse to one candidate by `(device, inode)`
file identity. This evidence records identifiers and bounded file metadata only;
it makes no test-ownership, process-termination, or root-cause claim.

The scan retains its existing limits: 1,024 entries, 256 files, 2 MiB total
reads, 64 KiB per file, 16 matches, 32 KiB serialized JSON, and two seconds.
Mapping failure cannot abort profile retention or replace the original
coverage failure.

## Required Evidence

- Exercise the genuine retained 160-byte header/ID from run 38613901957 with
  matching LLVM: retain its bytes and failed merge outcome; associate only a
  controlled exact-ID instrumented ELF, never claiming a historical producer.

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

## Acceptance Examples

- A supported profile with 33 ID entries retains the first 32 and reports
  partiality; a retained exact match stays visible and an absent ID remains
  inconclusive under that partial scope.
- An instrumented wrong-ID executable and a parsed uninstrumented executable
  produce a definitive negative within the inspected scope, with the latter
  counted as a noncandidate.
- Two instrumented candidates with the same matching ID remain ambiguous;
  a mapping failure preserves the original profile manifest and failed result.

## Test Mapping

The committed controls in
`tools/repo-policy/src/coverage_diagnostics/binary_ids_tests.rs` include:

- `profile_id_count_limit_keeps_the_valid_prefix_and_marks_mapping_partial`
  and `unavailable_profile_id_input_prevents_a_definitive_negative_mapping`:
  retained prefix and inconclusive negative evidence.
- `wrong_id_and_same_id_without_instrumentation_do_not_match` and
  `exact_instrumented_matches_preserve_duplicate_ambiguity`: exact identity,
  noncandidates and ambiguity.
- `cargo_hardlinked_root_entry_is_one_candidate_not_duplicate_ambiguity`:
  cargo's hard-linked root and `deps` entries of one executable collapse to a
  single unambiguous candidate while both entries stay inspected.
- `malformed_and_platform_elf_headers_and_notes_are_bounded`,
  `every_scan_limit_is_visible_even_without_a_successful_candidate`,
  `outside_roots_symlinks_and_changed_file_identity_are_refused`,
  `fifo_replacement_is_nonblocking_and_never_read` and
  `final_mapping_json_budget_includes_profile_scope_metadata`: unavailable
  layouts, scan budgets, file custody and serialized-byte limits.
- `retained_october_14_header_maps_only_instrumented_exact_id_and_preserves_failure`:
  genuine recurrence prefix, real LLVM rejection, exact instrumented association
  and unchanged retained bytes; controlled ELF inputs are not historical attribution.
- `mapping_failure_cannot_abort_the_original_profile_manifest`: preservation
  of the original failed-profile evidence.
- `real_rust_llvm_binary_id_matches_sparse_instrumented_elf_metadata`: native
  LLVM association; missing matching LLVM tools are a proof failure.

## Implementation Mapping

- `tools/repo-policy/src/coverage_diagnostics/binary_ids.rs` owns bounded
  profile-ID parsing and executable association.
- `tools/repo-policy/src/coverage_diagnostics/linux.rs` attaches the diagnostic
  fields and preserves retained profile hashes during manifest compaction; it
  is the Linux-only capture machinery submodule gated by #7624 reachability.
- `docs/ci/coverage.md` documents the failure-manifest output and claim limits.
- `.ripr/traceability.toml` connects this spec's tests, code and outputs.

## Metrics

- `unit_test_pass_rate`, as registered in `.ripr/traceability.toml`.
- Diagnostic mapping status and truncation fields describe inspected scope;
  they do not measure coverage success or historical producer attribution.
