# RIPR-SPEC-0266: Bounded coverage binary-ID diagnostics

Status: proposed

Owner: repo-policy / coverage diagnostics

Created: 2026-10-13

Linked issues: #4978, #13116

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

- Version-aware layout metadata in the CI failure manifest. No required gate,
  profile input, profile retention, or original failure outcome changes.
- A focused stable-toolchain job in the existing advisory future-Clippy workflow
  exercises the real compiler/LLVM boundary without running workspace coverage.

## Problem

The retained #4978 failure profile has a binary ID without a retained executable
association. Future failure diagnostics need a bounded association that preserves
unavailable evidence and ambiguity without attributing a test or cause.

## Behavior

The adapter may read at most the bounded prefix supplied by its caller and
recognizes only LLVM raw64 little-endian version 10 and 11 profile ID blocks.
The complete version selects both the header size and reported layout:

| Raw version | Header bytes | `binary_ids.layout` |
| --- | --- | --- |
| 10 | 128 | `llvm_raw64_le_v10` |
| 11 | 152 | `llvm_raw64_le_v11` |

`BinaryIdsSize` remains at byte offset 16; the length-prefixed ID block begins
after the complete version-specific header. LLVM added three uniform-counter
fields in [the v11 format change](https://github.com/llvm/llvm-project/commit/ab95a8e7bb519a91abdfa5722ec40c0e0909f8f4).
The [writer](https://github.com/llvm/llvm-project/blob/ab95a8e7bb519a91abdfa5722ec40c0e0909f8f4/compiler-rt/lib/profile/InstrProfilingWriter.c)
writes the entire header before the binary IDs. The adapter does not interpret
counter data, mask variant flags, or guess a future version's layout. Unknown
versions, flagged variants and other magic values remain unsupported. Available
and partial results identify the selected layout; a short version-specific
header remains unavailable even when it declares an empty ID block.

Each profile retains at most 32 ID entries of at most 128 bytes each, within an
8,192-byte ID block. If a later
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
- Parse v10 and v11 fixtures with exact IDs and layout labels, including a v11
  header containing a decoy at the old v10 ID offset. Require the real ID after
  byte 152 and reject unsupported versions and variant flags.
- Exercise short headers, short ID length/body/padding, oversized declarations
  and partial ID prefixes for both supported layouts. Preserve the selected
  layout and existing unavailable/partial distinctions.
- Exercise the active compiler's genuine profile with its matching
  `llvm-profdata`; retain exactly that version's header and ID block, verify
  executable association, and preserve the shortened profile's merge failure.
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
- Changing instrumentation, profile bytes, coverage thresholds, or required
  merge gates.
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

- `raw_binary_ids_are_exact_and_unknown_layouts_are_explicit` and
  `raw_v11_ids_start_after_the_extended_header`: exact v10/v11 IDs and labels,
  unsupported-version/variant refusal, and a decoy at the obsolete v10 offset.
- `raw_supported_headers_are_required_even_without_ids`,
  `raw_id_block_and_length_limits_remain_bounded` and
  `raw_id_truncation_preserves_only_valid_prefixes`: version-specific header
  admission, bounded declarations, short ID records and retained partial IDs.
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

The stable advisory job is covered in `tools/repo-policy/tests/future_clippy.rs`:

- `stable_compatibility_requires_matching_tools_and_focused_commands` refuses
  missing tools, incorrect activation, conditional preparation, ignored failures,
  missing commands and setup placed after the proof commands.
- `behavior::stable_identity_uses_matching_llvm_and_preserves_tool_failures`
  exercises the workflow's identity script with a selected sysroot containing a
  space, a wrong LLVM tool on `PATH`, and compiler/sysroot/host/LLVM failures.
  These shell fixtures do not substitute for genuine compiler/LLVM execution.

## Implementation Mapping

- `tools/repo-policy/src/coverage_diagnostics/binary_ids.rs` owns bounded
  profile-ID parsing and executable association.
- `tools/repo-policy/src/coverage_diagnostics/linux.rs` attaches the diagnostic
  fields and preserves retained profile hashes during manifest compaction; it
  is the Linux-only capture machinery submodule gated by #7624 reachability.
- `docs/ci/coverage.md` documents the failure-manifest output and claim limits.
- `.github/workflows/future-clippy.yml` runs the bounded stable compatibility
  check alongside the existing pinned advisory lint scan.
- `.ripr/traceability.toml` connects this spec's tests, code and outputs.

## Metrics

- `unit_test_pass_rate`, as registered in `.ripr/traceability.toml`.
- Diagnostic mapping status and truncation fields describe inspected scope;
  they do not measure coverage success or historical producer attribution.
