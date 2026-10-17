# RIPR-SPEC-0170: Stable Cross-Platform Path Text

Status: proposed

Owner: product-analysis

Created: 2026-08-31

Linked issues: #3609

Linked PRs: #3611

Support-tier impact: none; see [support tiers](../status/SUPPORT_TIERS.md).
This is a deterministic identity and rendering contract for paths already
accepted by the analyzer. It does not promote arbitrary-byte path support on
platforms whose native path type cannot carry those bytes.

Policy impact: none. The existing diff-path decoder and native filesystem path
remain the authorities; this spec defines their shared textual projection.

## Problem

Diff paths and native filesystem paths can reach textual identities used by
partial-diff normalization, probe IDs, and rendered output. The shared
`stable_path_text` projection escaped `%` only while walking Unix raw bytes.
Consequently, the same textual path could be encoded differently on Windows,
and a literal `%FF` could retain the reserved byte-escape spelling there.

## Behavior

`stable_path_text` produces a platform-consistent textual projection:

- path separators are rendered as `/`;
- every literal `%` is rendered as `%25`, on every platform;
- on Unix, valid UTF-8 is retained and invalid native bytes are rendered as
  uppercase `%XX` byte escapes;
- on non-Unix platforms, the existing native lossy text form is retained, but
  its literal `%` characters use the same `%25` escape;
- native `Path` values remain the authority for filesystem access and lexical
  confinement.

Git's C-quoted path input must preserve native identity before this projection,
including the named `\a`, `\b`, `\v`, and `\f` escapes for bytes `07`, `08`,
`0B`, and `0C`. These bytes are legal Unix filename characters; this does not
extend native Windows filename support. Successfully decoded quoted old/new
headers remain file boundaries in plain unified input when the previous hunk
has ended. A header-looking pair still inside a declared hunk remains source
payload. Unquoted paths with spaces are also file boundaries when a tab
separates the path from optional metadata: Git emits ASCII space paths in
this form without C-quoting them. Undelimited unquoted whitespace remains
ambiguous with payload. Native path confinement remains in force.

The `%25` rule reserves `%XX` for the Unix invalid-byte representation. Thus a
Unix raw byte `0xFF` renders as `%FF`, while a valid path literally containing
the three characters `%FF` renders as `%25FF`.

## Required Evidence

- a cross-platform unit test proves literal `%` becomes `%25`;
- a Unix unit test proves an invalid byte and literal `%FF` remain distinct;
- the existing diff-path regression continues to prove native Unix bytes are
  preserved through parsing and downstream path consumers.
- Real Git CLI fixtures retain named control-byte paths, distinct from unchanged
  letter mimics, with staged and unstaged edits. Default invocation from a
  subdirectory and explicit working-tree selection analyze the live predicate;
  committed selection excludes it, and untracked source stays disclosed.
- Literal two-section plain patches preserve quoted whitespace paths and each
  section's exact old/new lines through bounded and unbounded parser entrypoints.
- Tab-delimited unquoted space paths preserve each section's old/new coordinates;
  a header-looking pair inside declared hunk spans remains source payload.
- Real Git plain and extended patches yield the same nonempty public findings
  for ordinary, spaced and Unicode/non-BMP Rust and Python paths with CRLF
  contents. Repeated CLI JSON is deterministic and its existing Python
  source-subject entries name each exact path with the digest of its original
  CRLF bytes. Generic Rust predicate findings do not currently produce those
  stamp entries; the control checks their file and line locations separately.

## Non-Goals

- changing native path confinement;
- adding arbitrary-byte path support to Windows or other non-Unix platforms;
- changing JSON schemas, probe-ID formats beyond the reserved escaping rule,
  or support tiers;
- claiming cross-platform filesystem byte identity where the native path type
  cannot represent the original bytes.

## Acceptance Examples

1. `pricing_%FF.rs` renders as `pricing_%25FF.rs` on Unix and non-Unix.
2. On Unix, native `pricing_\xFF.rs` renders as `pricing_%FF.rs`.
3. On Unix, the two inputs above have different textual identities.
4. Existing slash normalization and native-path confinement are unchanged.

## Test Mapping

- `crates/ripr/src/analysis/mod.rs::tests::stable_path_text_escapes_reserved_percent_on_every_platform`
- `crates/ripr/src/analysis/mod.rs::tests::stable_path_text_keeps_invalid_byte_encoding_distinct_from_literal_escape`
- `crates/ripr/src/analysis/diff/load.rs::tests::given_quote_path_disabled_repo_when_diff_loaded_then_raw_byte_and_mimic_names_stay_distinct`
- `crates/ripr/src/analysis/diff/path.rs::tests::named_c_control_escapes_keep_native_path_identity`
- `crates/ripr/src/analysis/diff/path.rs::tests::quoted_whitespace_old_headers_keep_plain_file_sections_separate`
- `crates/ripr/src/analysis/diff/path.rs::tests::quoted_control_header_pair_inside_declared_hunk_stays_source_payload`
- `crates/ripr/src/analysis/diff/path.rs::tests::tab_delimited_plain_headers_keep_paths_and_line_coordinates`
- `crates/ripr/src/analysis/diff/path.rs::tests::tab_delimited_header_pair_inside_declared_hunk_stays_source`
- `crates/ripr/tests/cli_smoke/portable_diff_paths.rs::plain_patch_json_keeps_portable_locations_and_exact_crlf_provenance`
- `crates/ripr/tests/cli_smoke/diff_selection.rs::git_named_control_paths_select_the_live_predicate_from_root_and_subdirectory`

## Implementation Mapping

- `crates/ripr/src/analysis/mod.rs` owns the shared textual projection.
- `crates/ripr/src/analysis/diff/path.rs` owns the C-quoted decoder feeding
  native path identity into the projection; its `DecodedPath` split (valid
  text vs Unix raw bytes) is the byte-preserving source this contract
  renders.
- `crates/ripr/src/analysis/syntax/ra.rs` consumes the projection for owner
  symbol identity so distinct raw-byte files keep distinct owners.
- `crates/ripr/src/app/check_artifact.rs` renders non-UTF-8 finding paths
  through the projection at the artifact boundary.
- `crates/ripr/src/analysis/language/rust/mod.rs`,
  `crates/ripr/src/analysis/probes/ids.rs`, and
  `crates/ripr/src/output/path.rs` consume that projection for their existing
  textual identity/rendering surfaces.

## Metrics

- `stable_path_text_regression_pass_rate`
- `unit_test_pass_rate`
