# RIPR-SPEC-0008: SARIF and CI Policy

Status: proposed

## Problem

`ripr` needs a GitHub-native reporting surface for static exposure findings and
repo seam evidence before CI policy can be useful in real repositories.

JSON and human output are already stable enough for local and agent workflows,
but GitHub code scanning, review artifacts, and baseline-aware policy need a
SARIF contract with stable rule IDs, configured severity mapping, visible
suppression handling, and conservative defaults.

## Behavior

`ripr` should emit SARIF 2.1.0 for two static evidence surfaces:

- diff-scoped Findings from `ripr check --format sarif`;
- repo-scoped classified seams from `ripr check --format repo-sarif`.

The SARIF layer is a renderer over existing analyzer evidence. It must not
classify findings or seams itself.

The SARIF layer should:

- declare `columnKind: "unicodeCodePoints"` on finding and repo-seam runs,
  matching the producers' 1-based Unicode scalar-value columns (SARIF 2.1.0
  section 3.14.27); the declaration is also valid when the result array is empty;
- use stable rule IDs for every Finding exposure class and seam grip class;
- map `ripr.toml` configured severity into SARIF result levels;
- omit results whose configured severity is `off`;
- keep suppressed results visible when the configured severity permits them,
  using SARIF suppression metadata instead of silently deleting them;
- include stable fingerprints so baseline comparison can identify already-known
  results;
- preserve existing human, JSON, GitHub annotation, badge, LSP, and context
  schemas unless a later scoped PR explicitly changes them;
- keep CI behavior advisory by default;
- make baseline-aware blocking opt-in;
- when the candidate-actionable filter (#3281 / RIPR-SPEC-0152) drops
  findings from diff-scoped `results`, record
  `runs[].properties.omitted_not_current` as that count so an all-omitted
  complete run is not indistinguishable from a clean analysis (#5091).
  The property is absent when the count is zero. Omitted findings stay
  out of `results`; the property only discloses the filter, matching the
  GitHub annotation denominator notice (#4393);
- fold diff-scoped results that share a rule, file, line and suppression
  state into the first one, so one changed line is one code-scanning alert
  (#11219). The kept result is the one with the smallest fingerprint (input order never
  changes it), keeps its message and fingerprint, lists the
  folded results in `properties.folded_results`, and the run records the
  count in `runs[].properties.folded_same_line_results` (absent when zero).
  Different classes never fold: a weaker class does not hide a stronger one.

## Rule IDs

Finding rule IDs are:

| Exposure class | Rule ID |
| --- | --- |
| `exposed` | `ripr.finding.exposed` |
| `weakly_exposed` | `ripr.finding.weakly_exposed` |
| `reachable_unrevealed` | `ripr.finding.reachable_unrevealed` |
| `no_static_path` | `ripr.finding.no_static_path` |
| `infection_unknown` | `ripr.finding.infection_unknown` |
| `propagation_unknown` | `ripr.finding.propagation_unknown` |
| `static_unknown` | `ripr.finding.static_unknown` |

Seam rule IDs are:

| Seam grip class | Rule ID |
| --- | --- |
| `strongly_gripped` | `ripr.seam.strongly_gripped` |
| `weakly_gripped` | `ripr.seam.weakly_gripped` |
| `ungripped` | `ripr.seam.ungripped` |
| `reachable_unrevealed` | `ripr.seam.reachable_unrevealed` |
| `activation_unknown` | `ripr.seam.activation_unknown` |
| `propagation_unknown` | `ripr.seam.propagation_unknown` |
| `observation_unknown` | `ripr.seam.observation_unknown` |
| `discrimination_unknown` | `ripr.seam.discrimination_unknown` |
| `opaque` | `ripr.seam.opaque` |
| `intentional` | `ripr.seam.intentional` |
| `suppressed` | `ripr.seam.suppressed` |

Rule IDs are stable public integration strings. A later schema version may add
new rule IDs, but existing IDs must not be renamed.

## Severity Mapping

Configured severities come from `ripr.toml`, with built-in defaults when the
file is absent.

| `ripr` severity | SARIF result behavior |
| --- | --- |
| `warning` | emit result with `level: "warning"` |
| `info` | emit result with `level: "note"` |
| `note` | emit result with `level: "note"` |
| `off` | omit result |

`ripr` SARIF does not emit `level: "error"` in v1. Static exposure evidence is
advisory unless an explicit CI policy command treats new configured-warning
results as blocking.

Finding severities cannot be `off` in repository config today. Seam severities
can be `off`; the default seam policy omits `strongly_gripped`, `intentional`,
and `suppressed` from SARIF unless the repository explicitly configures those
classes to a visible severity.

## Result Shape

Every SARIF result should include:

- `ruleId` matching the tables above;
- `level` from configured severity;
- one primary physical location when file and line are known;
- a short message that uses static exposure vocabulary; finding results (not seam results) lead with the first missing check when one exists and bound that evidence head to 240 characters (the reconciled next step is appended whole per RIPR-SPEC-0088), with full evidence kept in `properties` (#11219);
- `partialFingerprints.riprFingerprintV1`;
- `properties.tool = "ripr"`;
- `properties.kind = "finding"` or `"seam"`;
- stable identifiers such as `finding_id`, `probe_id`, or `seam_id` when
  available;
- class metadata such as `classification`, `probe_family`, `seam_kind`, or
  `grip_class`;
- static evidence summaries already computed by the analyzer, when available.

Finding fingerprints should prefer:

```text
rule_id | finding_id | probe_id | normalized_file
```

Seam fingerprints should prefer:

```text
rule_id | normalized_file | identity_hash | ordinal
```

The finding's display line is intentionally absent. When the producer's native
finding/probe identity remains stable, moving the same changed behavior updates
its physical location without creating a new fingerprint. Rust, Python and
TypeScript retain their existing native identity rules and collision limits.
The Perl projection uses the named-owner semantic payload described in
[SPEC-0064](RIPR-SPEC-0064-perl-fact-packet-contract.md#ripr-derived-identities);
unnamed or ambiguous owners retain an explicitly location-bound fallback.
The renderer does not infer semantic identity from display text.

A seam fingerprint is independent of the display line, the byte offset and the
offset-derived `seam_id` (#11219), so an unrelated edit above a seam keeps its
code-scanning alert. `identity_hash` is a 128-bit stable hash (32 lowercase hex
characters) over the length-prefixed seam owner, seam kind and
expression with whitespace between tokens collapsed to one space; the
contents of string literals (including raw strings) are kept byte for byte, so
`"a  b"` and `"a b"` stay distinct. `ordinal` is the zero-based source-order rank
among seams of the same file, owner, kind and expression in the classified
set, assigned before severity filtering so another seam's class or `off`
setting never renumbers it. Seams that differ in file, owner, kind or
expression get different hash inputs, and textual duplicates are told apart by
ordinal, so distinct seams do not share a fingerprint by construction; the
hash is not cryptographic, so an accidental 128-bit collision between
different inputs is negligible rather than mathematically excluded. The grip class
stays in `rule_id`, so a reclassified seam is a new alert. Reordering
identical duplicate seams, or a seam-limit cap that changes which duplicates
are analyzed, can swap ordinals because static evidence cannot tell them
apart. Renaming the owner, changing the seam kind or editing the expression
is a different seam identity and opens a new alert; the fingerprint follows
the seam's text and scope, not a tracked "same seam" across edits.
`properties.seam_id` is unchanged.

Both `partialFingerprints.riprFingerprintV1` and
`fingerprints.riprFingerprintV1` carry the same string. File and rule segments
remain part of identity: moving a finding to a different file or changing its
exposure class is not a pure line move. Native exposure suppressions still
match exact `finding.id` values, independently of SARIF fingerprint text.
Distinct native filenames must remain distinct after path normalization. In
particular, Unix `src/a\b.rs` and `src/a/b.rs` name different files; retaining a
file segment alone does not establish this property. The native-filename
control below covers equal producer IDs at different display lines, where a
line-free fingerprint must still preserve the file boundary.

Dropping the finding line changes existing fingerprint strings once, including
stationary findings. The policy reader compares opaque strings and supplies no
legacy alias, so an old SARIF baseline may report unchanged findings as new.
Review and refresh that baseline after the transition. Perl's new native IDs
also require reviewed replacement of old finding-ID selectors; an ambiguous old
owner-based selector must never suppress either or both new behaviors by alias.
Unmatched selectors remain unapplied with a visible warning. These contracts do
not establish live code-scanning ingestion, producer digest correctness or
universal collision freedom.

`normalized_file` and the physical location `uri` do not depend on how
`--root` is spelled: `.`, `./` and the checkout's absolute path render the
same text, because an absolute root prefix is removed before rendering. A
relative root keeps its spelling, since it already names a path under the
invoking directory.

For retained UTF-8 finding paths (`--format sarif`), separators follow the
native host: Windows backslashes become `/`, while a literal backslash in a
Unix filename remains part of that filename. In particular, `src/a\b.rs`
on Unix must emit `src/a%5Cb.rs`, distinct from `src/a/b.rs`; its fingerprint
retains the literal backslash too. Path conversion leaves ordinary
slash-separated file components unchanged; finding fingerprint migration is
described above. Repo-scoped seam construction still
normalizes backslashes before rendering; the renderer cannot restore native
filename bytes already discarded by that producer.

The physical location `uri` is that path percent-encoded as an RFC 3986
relative reference: every byte of the UTF-8 path outside `A-Z a-z 0-9 - . _ ~ /` is written
as `%XX` (so `é` becomes `%C3%A9`), so a space, `#`, `?`, `%` or `:` in a file name cannot change which file
the uri names. Fingerprints hash the unencoded `normalized_file`, so the
encoding does not move alert identity.

Fallback fingerprints may use `rule_id | normalized_file | line | message`,
but only when the stable finding or seam identifier is unavailable.

## Suppressions

Suppressions are audit metadata, not silent deletion, unless configured severity
is `off`.

For v1:

- `kind = "exposure_gap"` suppressions matching a Finding ID should attach a
  SARIF `suppressions` entry to that result.
- Expired or malformed suppressions must not suppress a result.
- Seam results whose grip class is `suppressed` are omitted by default because
  `severity.seams.suppressed = "off"`. If a repository configures that class to
  `info`, `note`, or `warning`, the result should be emitted with SARIF
  suppression metadata.
- Suppression metadata should include the configured justification when SARIF
  can represent it.

The SARIF renderer must not invent suppressions. It consumes the same configured
suppression path used by badge/report policy.

## CI Policy

CI policy is opt-in and baseline-aware.

Modes:

| Mode | Behavior |
| --- | --- |
| `advisory` | Emit reports and exit successfully even when results exist. This is the default. |
| `baseline-check` | Compare current SARIF to a baseline and report new configured-warning results. |
| `fail-on-new-warning` | Compare current SARIF to a baseline and exit non-zero when new configured-warning results appear. |

The first implementation should prefer an `xtask` policy command over a public
`ripr` CLI policy surface. This keeps CI blocking behavior repository-automation
scoped until the shape has been exercised.

Baseline comparison should:

- compare unsuppressed results using `ruleId` plus `partialFingerprints`;
- treat `warning` as the default blocking threshold;
- ignore `note` results when the threshold is `warning`;
- ignore results omitted by configured `off` severity;
- report missing baseline as advisory by default unless an explicit flag says
  missing baseline is an error;
- never enable a default workflow that blocks pull requests.

## Required Evidence

SARIF and CI policy evidence should cover:

- SARIF rule IDs for all Finding exposure classes;
- SARIF rule IDs for all seam grip classes;
- configured severity mapping for Findings and seams;
- configured `off` omitting seam results;
- suppression metadata for matched exposure-gap suppressions;
- SARIF JSON validity;
- baseline comparison passing when no new configured-warning results exist;
- baseline comparison flagging new configured-warning results;
- threshold handling that ignores notes when threshold is warning;
- docs showing advisory upload and opt-in blocking;
- diff-scoped SARIF recording `omitted_not_current` when the
  candidate-actionable filter drops findings, and omitting the property
  on a zero-finding complete run.

## Non-Goals

This spec does not require:

- branch protection changes;
- default-on CI blocking;
- GitHub workflow changes in the contract PR;
- badge count remapping;
- new analyzer classifications;
- new suppression semantics beyond the existing configured suppression path;
- runtime mutation outcome language in static SARIF results.

## Acceptance Examples

### Finding SARIF uses configured severity

```text
Given ripr.toml configures weakly_exposed findings as warning,
when a weakly_exposed Finding is rendered as SARIF,
then the result uses ruleId ripr.finding.weakly_exposed and level warning.
```

### Seam SARIF omits off severity

```text
Given ripr.toml configures strongly_gripped seams as off,
when repo seam SARIF is rendered,
then strongly_gripped seam results are omitted.
```

### Baseline policy is opt-in

```text
Given a SARIF report with warning-level results,
when no baseline policy mode is requested,
then CI artifact generation remains advisory and does not block by default.
```

### Diff-scoped SARIF counts omitted not-current findings

```text
Given a complete analysis whose findings are all not candidate-actionable,
when those findings are rendered as SARIF,
then results is empty, run_status is complete, and
runs[].properties.omitted_not_current equals the omitted count.
```

```text
Given a complete analysis with zero findings,
when it is rendered as SARIF,
then results is empty, run_status is complete, and
runs[].properties.omitted_not_current is absent.
```

### Same-line findings of one class fold into one alert

```text
Given two current findings of the same class on one file and line,
when they are rendered as SARIF,
then results holds one entry with properties.folded_results naming the
other, and runs[].properties.folded_same_line_results equals 1.
```

```text
Given findings that differ in class or line,
when they are rendered as SARIF,
then every one is its own result and folded_same_line_results is absent.
```

### Baseline policy flags new warnings

```text
Given a baseline SARIF report and a current SARIF report,
when the current report has a warning-level fingerprint absent from the
baseline,
then fail-on-new-warning mode reports the new result and exits non-zero.
```

## Test Mapping

Implemented renderer tests:

- `crates/ripr/tests/sarif_uri_identity.rs::ordinary_escaping_controls`
- `crates/ripr/tests/sarif_uri_identity.rs::native_backslash_does_not_alias_directory`
- `crates/ripr/tests/sarif_uri_identity.rs::native_windows_separators_are_slashes`
- `crates/ripr/src/output/sarif.rs::tests::sarif_renders_findings_with_stable_rule_ids`
- `crates/ripr/src/output/sarif.rs::tests::sarif_renders_seams_with_stable_rule_ids`
- `crates/ripr/src/output/sarif.rs::tests::sarif_finding_identity_moves_locations_but_retains_file_and_rule_boundaries`
- `crates/ripr/src/output/sarif.rs::tests::sarif_seam_identity_is_line_independent_but_file_and_rule_bound`
- `crates/ripr/src/output/sarif.rs::tests::sarif_finding_identity_keeps_distinct_native_unix_filenames` (`#[cfg(unix)]`: `src/a.rs` is a nested path on Windows, so this control does not run there; Windows separator conversion is pinned by `sarif_uri_identity.rs::native_windows_separators_are_slashes`)
- `crates/ripr/src/output/sarif.rs::tests::sarif_uses_configured_finding_severity`
- `crates/ripr/src/output/sarif.rs::tests::sarif_uses_configured_seam_severity`
- `crates/ripr/src/output/sarif.rs::tests::sarif_omits_off_seam_class`
- `crates/ripr/src/output/sarif.rs::tests::sarif_attaches_suppression_metadata`
- `crates/ripr/src/output/sarif.rs::tests::sarif_projects_python_repair_card_properties`
- `crates/ripr/src/output/sarif.rs::tests::sarif_projects_python_static_limit_no_action_properties`
- `crates/ripr/src/output/sarif.rs::tests::sarif_projects_python_ordinary_no_action_properties`
- `crates/ripr/src/output/sarif.rs::tests::sarif_run_with_only_not_current_findings_records_omitted_property`
- `crates/ripr/src/output/sarif.rs::tests::sarif_empty_findings_run_omits_omitted_not_current_property`
- `crates/ripr/src/output/sarif.rs::tests::sarif_mixed_current_and_not_current_records_omitted_count_beside_results`
- `crates/ripr/src/output/sarif.rs::tests::same_line_same_class_findings_fold_into_one_alert_that_names_the_rest`
- `crates/ripr/src/output/sarif.rs::tests::findings_that_differ_in_class_or_line_stay_separate`
- `crates/ripr/src/output/sarif.rs::tests::the_kept_alert_does_not_depend_on_input_order`
- `crates/ripr/src/output/sarif.rs::tests::a_suppressed_finding_is_not_folded_into_an_active_one`
- `crates/ripr/src/output/sarif.rs::tests::findings_without_a_line_are_never_folded`

Implemented policy tests (the `sarif_identity_*` controls need the Perl adapter and compile only with `cargo test -p xtask --features perl-identity`, run in the Perl lane so the shared workspace graph keeps the shipped feature set):

- `xtask/src/main.rs::tests::sarif_policy_passes_when_no_new_results`
- `xtask/src/main.rs::tests::sarif_policy_flags_new_warning_result`
- `xtask/src/main.rs::tests::sarif_policy_ignores_note_when_threshold_warning`
- `xtask/src/main.rs::tests::sarif_policy_missing_baseline_is_advisory_by_default`
- `xtask/src/reports/sarif_identity_tests.rs::sarif_identity_preserves_perl_changes_and_supported_line_shifts`
- `xtask/src/reports/sarif_identity_tests.rs::sarif_identity_keeps_native_suppression_exact_and_legacy_selector_stale`
- `xtask/src/reports/sarif_identity_tests.rs::sarif_identity_legacy_finding_baselines_require_explicit_transition`
- `xtask/src/reports/sarif_identity_tests.rs::sarif_identity_native_paths_survive_policy_baselines_and_line_moves`

## Implementation Mapping

Implemented renderer:

- `crates/ripr/src/output/sarif.rs` renders SARIF.
- `crates/ripr/src/output/mod.rs` exposes the private renderer to `app`.
- `crates/ripr/src/app.rs` dispatches SARIF formats without changing existing
  JSON schemas.
- `crates/ripr/src/cli/parse.rs` accepts SARIF output format names.

Implemented policy:

- `xtask/src/main.rs` owns baseline-aware policy comparison.
- `docs/OUTPUT_SCHEMA.md` and `docs/CI.md` document the emitted SARIF and
  policy usage.
- `docs/CI.md` also documents the copyable non-blocking GitHub code-scanning
  upload workflow for adopters.

## Metrics

- `sarif_results_total`
- `sarif_results_by_rule`
- `sarif_results_by_level`
- `sarif_new_warning_results`
