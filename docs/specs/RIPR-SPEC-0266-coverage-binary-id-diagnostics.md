# RIPR-SPEC-0266: Bounded coverage binary-ID diagnostics

Status: proposed

Owner: repo-policy / coverage diagnostics

Created: 2026-10-13

Linked issues: #4978, #13116, #13221

Linked PRs: #7579, #13121

Standalone reason: #4978 records failed coverage diagnostics from the CI
coverage adapter. This contract governs diagnostic evidence and the bounded
compatibility control for stripping coverage-build debug information; it does
not classify coverage as successful or identify a test, process, or cause.

Support-tier impact:

- None.

This spec concerns unpublished repository automation output; it does not
change any language or platform support tier.

Policy impact:

- Version-aware layout metadata and bounded raw-version-mismatch capture in the
  CI failure manifest. Required gates, validated profile inputs, retention limits
  and the original failure outcome remain unchanged.
- A focused stable-toolchain job in the existing advisory future-Clippy workflow
  exercises the real compiler/LLVM boundary and isolated fixture activation
  through the documented pinned/stable upgrade recipe, without running workspace
  coverage or adding a required check. Its existing 15-minute budget and routing
  remain unchanged.
- Only Coverage's Generate step sets `CARGO_PROFILE_TEST_STRIP=debuginfo`.
  Its mandatory native diagnostics preflight checks coverage equivalence;
  ordinary builds, full test selection, profiles and report failure semantics
  retain their existing contracts.

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

When the active compiler's matching LLVM tool reports
`warning: <snapshot path>: raw profile version mismatch: ...`, the manifest
retains the bounded prefix with `error_family: raw_profile_version_mismatch`.
This [LLVM diagnostic](https://github.com/rust-lang/llvm-project/blob/1b9c0d5ff9bbe7634aead059efe6b11a7eeba145/llvm/lib/ProfileData/InstrProf.cpp)
can occur when replaying a retained v10 profile with a newer LLVM reader. The
warning must identify an exact validated snapshot path; unknown warnings and
other paths remain unclassified. Capturing the diagnostic does not convert a
profile or change the failed merge result.

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

### Coverage command ownership

One private Linux owner retains the primary child's PID with non-reaping
observation until owned-group cleanup, then reaps it exactly once. Lost
custody disarms numeric signalling. Native success, failure and signal status
remain observable; an exhausted observation budget never stops the command.
The fixed full-workspace/all-features LCOV builder inherits all streams and
environment variables. Controls require its exact arguments, a confirmed
separate descendant, loss-of-custody refusal and cleanup on owner drop.
The environment control seeds a dedicated variable only in an isolated exact
libtest child. The observed command must inherit that variable without an
explicit override; the parent environment is never changed. Absence of
ambient HOME is exercised in that child. Require its successful native
status, exact nonempty test selection and a fresh acknowledgement written
only after the original exit, phase, stream and profiling assertions pass.
Require the inner final receipt to explicitly confirm successful cleanup.
The parent bounds the child to 30 seconds and each captured stream to 16 KiB.
All witnesses remain under its owned root; errors or panics retain them,
and deletion requires confirmed cleanup and completed validation.
Bounded records use one write of at most 4 KiB and flush immediately; a short
write is an error. The prerequisite uses this owner only in native controls;
the Generate step still directly invokes the fixed Cargo command.

### Coverage-build strip compatibility

The Generate step may remove linked debug information with the literal Cargo
test-profile setting `CARGO_PROFILE_TEST_STRIP=debuginfo`. The setting must not
move to workflow/job scope, apply to plain prebuilds, become `symbols` or
replace the fixed full-workspace/all-features LCOV command. The selected
control must execute in the mandatory diagnostics preflight before Generate,
without a conditional, ignored result or narrower test filter.

The native Linux control builds one dependency-free source fixture in separate
Cargo targets with explicit baseline `none` and candidate `debuginfo` modes.
Both builds use the same compiler and explicit coverage/link-ID arguments;
child-only wrapper and encoded-flag overrides remove outer llvm-cov compiler
interposition without mutating the parent environment. Require exactly one
intended Cargo test artifact and actual test compiler invocation per mode.
The candidate invocation must request `strip=debuginfo`; do not require Cargo
to emit its default `strip=none`. Require a nonempty, nonallocated `PROGBITS`
`.debug_line` baseline section and a smaller candidate executable. The
candidate must remove every debug-prefixed section except the exact nonempty
`PROGBITS` `.debug_gdb_scripts` loader metadata with `ALLOC` and without
write/execute flags. Allocation cannot excuse retained DWARF or another
unknown, compressed or lookalike debug name. The compiler's loader metadata
is not a DWARF table; use the emitted native section evidence to identify an
actual remaining section instead of inferring it from this permitted case.

Require exactly one literal `[Nr]` column header before any numeric row, with
whitespace-separated tokens `Name Type Address Off Size ES Flg Lk Inf Al` in
that order. Reject missing, malformed, duplicate or late column headers and
require numeric unique indices for data rows. Decode and record type/size/flags
only for debug-prefixed rows, converting hexadecimal sizes to byte counts and
requiring unique debug names. Optional empty flags remain valid; nonempty
flags contain only unique bytes from `WAXMSILOGTCERlyopx`, as emitted by the
[pinned LLVM GNU section printer](https://github.com/rust-lang/llvm-project/blob/1cb4e3833c1919c2e6fb579a23ac0e2b22587b7e/llvm/tools/llvm-readobj/ELFDumper.cpp).
Check this lexical flag grammar before applying the retained section policy.
The unnamed `NULL` row is not a debug subject; ordinary section field layouts
and ordinary section-name uniqueness are outside this control's validation.
Reject malformed debug fields and missing, truncated or excessive table
structure. Bound inspection to 256 indexed rows, 32 debug
sections and text fields of 128/32/16 bytes for names/types/flags. Emit typed
sections and bounded actual compiler text before the native assertions. Each
record is at most 4 KiB; compiler and parse-failure text use at most eight
512-character chunks with explicit remaining/truncated markers. Pure controls
must reject retained allocated DWARF, a nonallocated loader, wrong/empty
section types/sizes, writable/executable loader flags, ambiguous tables and
prefix lookalikes while retaining the valid native-shaped alternatives.

Each executable runs the same exact libtest identity twice. Require one test
passed and one post-assertion acknowledgement per invocation, plus a fresh,
nonempty bounded raw profile. The matching compiler-sysroot LLVM tools must
merge both profiles without warnings, match the merged profile ID to that
mode's own executable and export both JSON and LCOV for the exact source.
Require nonempty source regions and an execution count of two for each
authored fixture function, a hot-line count of two and a cold-line count of
zero. Compare the complete owned JSON file record, both authored function
records after removing only their mangled names, and all owned LCOV line
counts between modes. Other exported files and unrelated functions are
outside this compatibility probe; it does not qualify the LLVM exporter or
require whole-export equality. Different image IDs across modes remain valid.
Missing tools, malformed records, duplicate/absent subjects and
zero test selection are errors, not skipped proof. Existing genuine malformed
profile refusal and diagnostic size limits remain exercised.

Native control commands use owned process groups with non-reaping custody,
30-second command bounds and 256 KiB per-stream capture limits. The bounded
receipt identifies actual tools, both executable sizes, IDs and counts. These
control bounds do not shorten the workload timeout. Before explicit success
cleanup, the native fixture retains and reports its root on any error or
panic. Removal requires all commands and assertions to finish successfully
and precedes the passed receipt. Fixture shrinkage proves
that stripping took effect; full-workspace allocation savings and capacity
acceptance require the complete native coverage workload and retained report.
The observed instrumented-build bucket is not a measurement of debug bytes
and does not establish the producer of later, unobserved ENOSPC growth.

### Profile and compiler evidence

- Exercise the genuine retained 160-byte header/ID from run 38613901957 with the
  active compiler's matching LLVM: retain its bytes and failed merge outcome
  for malformed data or a raw-version mismatch; associate only a controlled
  exact-ID instrumented ELF, never claiming a historical producer.
- Capture a known raw-version-mismatch warning only for an exact pinned input
  path. Refuse unknown warnings, inexact warning names and other paths.
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
- Execute the actual marked `docs/ci/coverage.md` upgrade recipe on native Linux
  from a stable SDK first on `PATH` and an absolute stable `RUSTC`. Its first
  pinned check must select the pin despite that initial activation; provisioning
  explicitly installs the pin even when automatic installation is disabled.
- Require all four ordered successful commands: pinned diagnostics, stable
  diagnostics, stable `frontdoor`, and strict stable package Clippy. Each test
  phase must report exactly one nonempty successful outer summary; each diagnostic
  phase must report exactly one genuine profile association. Retain compiler,
  LLVM, raw version/layout and native failure output. Filtered diagnostics and a
  libtest progress prefix before the association remain valid.
- Refuse missing, duplicate, empty or malformed recipe boundaries; omitted,
  commented or dead commands; zero tests, absent or duplicate observations,
  out-of-order completion, and stale source/run/attempt evidence. Native Cargo
  or log-capture failure must stop proof and preserve its failure status.
- Execute the production-block workflow controls before the native recipe.
  Controlled tools prove selection and refusal semantics only; they do not
  substitute for the native compiler/LLVM execution required above.

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

The strip compatibility controls are:

- `tools/repo-policy/src/coverage_diagnostics/strip_tests.rs::native_test_profile_strip_preserves_coverage_and_reduces_fixture_size`:
  actual Cargo compilation, exact acknowledged execution, strict matching LLVM
  merge/export, source/counter equivalence, own-image IDs and active stripping.
- `tools/repo-policy/src/coverage_diagnostics/strip_tests.rs::debug_sections_require_dwarf_removal_and_exact_allocated_loader_marker`:
  one exact GNU column header before data, numeric indexed rows,
  unnamed/optional-field handling, unique producer flag bytes, exact loader
  identity/type/flags and nonempty baseline DWARF, with allocated-DWARF,
  wrong-name/type/size/flag, missing/misplaced/duplicate header, malformed and
  duplicate-subject refusals.
- `tools/repo-policy/src/workflow_tests.rs::coverage_strip_is_generate_only_and_requires_native_controls`:
  mandatory preflight, exact Generate-only setting and full Cargo command,
  with global/other-step, missing, duplicate, stronger-strip, skipped-control
  and reduced-command counterexamples. The selected fixed-command control
  independently pins every workspace/all-features/LCOV argument.

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

The Linux capture seam is covered in
`tools/repo-policy/src/coverage_diagnostics/linux.rs`:

- `raw_profile_version_mismatch_capture_requires_known_warning_and_exact_path`
  retains the recognized diagnostic and bytes only for an exact pinned input;
  unknown warning text and unrelated paths remain unclassified.

The stable advisory job is covered in `tools/repo-policy/tests/future_clippy.rs`:

- `stable_compatibility_requires_matching_tools_and_focused_commands` refuses
  missing tools, incorrect activation, conditional preparation, ignored failures,
  missing diagnostic, `frontdoor` or Clippy commands, duplicate step selection,
  omitted executed controls, and setup placed after the proof commands.
- `behavior::stable_identity_uses_matching_llvm_and_preserves_tool_failures`
  executes the actual extractor, native runner and verifier with synthetic SDK
  paths containing spaces. It checks exact Cargo order, explicit/PATH/nested
  compiler selection, matching LLVM, provisioning, missing/nonexecutable/wrong
  Clippy, original Cargo failure statuses and failed log capture.
- `behavior::documented_recipe_boundaries_refuse_ambiguous_or_stale_execution`
  mutates the actual document and refuses malformed boundaries before Cargo,
  including when an old successful extracted script was present.
- `behavior::documented_recipe_requires_all_four_native_checks` omits, comments
  and disables each real recipe call; wrong explicit/PATH/nested selection and
  omitted explicit pin installation cannot satisfy the independent tool oracle.
- `behavior::documented_recipe_evidence_refuses_zero_subjects_and_stale_runs`
  refuses empty, failed or duplicate test/profile evidence, missing Clippy
  completion, missing/duplicate/reordered command records, a previous phase's
  summary, stale run identity and a seeded old successful log.

These shell fixtures do not substitute for genuine compiler/LLVM execution.

## Implementation Mapping

- `tools/repo-policy/src/coverage_diagnostics/process.rs` owns command fidelity,
  process custody and bounded record writes; `process_tests.rs` retains their
  real status, descendant and lost-custody controls.
- `tools/repo-policy/src/coverage_diagnostics/strip_tests.rs` owns the native
  strip comparison and ELF-section parser controls, registered alongside the
  process/workflow controls in `.ripr/traceability.d/4978-coverage-strip.toml`.

- `tools/repo-policy/src/coverage_diagnostics/binary_ids.rs` owns bounded
  profile-ID parsing and executable association.
- `tools/repo-policy/src/coverage_diagnostics/linux.rs` attaches the diagnostic
  fields and preserves retained profile hashes during manifest compaction; it
  is the Linux-only capture machinery submodule gated by #7624 reachability.
- `docs/ci/coverage.md` documents the failure-manifest output and claim limits.
- `.github/workflows/future-clippy.yml` runs the bounded stable compatibility
  check by extracting the marked documentation recipe alongside the existing
  pinned advisory lint scan.
- `.ripr/traceability.toml` and
  `.ripr/traceability.d/13221-native-coverage-upgrade.toml` connect this spec's
  tests, code and outputs.

## Metrics

- `unit_test_pass_rate`, as registered in `.ripr/traceability.toml`.
- Diagnostic mapping status and truncation fields describe inspected scope;
  they do not measure coverage success or historical producer attribution.
