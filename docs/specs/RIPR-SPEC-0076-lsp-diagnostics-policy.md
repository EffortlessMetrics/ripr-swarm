# RIPR-SPEC-0076: LSP Diagnostics Severity Policy

Status: accepted

Owner: product / swarm

Created: 2026-06-11

Linked proposal:

- None

Linked ADRs:

- None

Linked plan:

- None

Linked issues:

- None

Linked PRs:

- None

Support-tier impact:

- None. This spec records the severity-mapping policy for LSP diagnostics.
  It does not promote any language, surface, or evidence class to a stronger
  support tier.
- Claim boundaries remain governed by the canonical ledger in
  [support tiers](../status/SUPPORT_TIERS.md); nothing here promotes a tier.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- Nothing new beyond the spec itself: no new crate, binary, dependency,
  parser, runtime executor, LSP server, artifact type, or workflow.

## Problem

The LSP sidecar emitted WARNING for any repairable gap record and for
finding classes that map to `ConfigSeverity::Warning` — including advisory
findings (preview language, static limit) that carry no complete repair
packet. A WARNING without an actionable, packet-complete repair route is
decoration noise that reduces editor signal quality and violates the
cockpit principle of RIPR-SPEC-0069.

## Behavior

### Passive Rust corpus currentness

A gap ledger or actionable-gap report with a complete Rust language population
must match the full Rust inventory's captured `files_content_hash` before the
LSP admits its records, diagnostics, actions or repair packet. The existing
source-subject and canonical root authorities remain in force. The producer
must preserve the key from the actual classified analysis, including cache
hits; a derived writer must not replace it with current workspace bytes.

An added, edited or deleted production/test Rust file outside the old named
repair route refuses the artifact as `stale_corpus`, including at unchanged
HEAD. Run status is `stale`; cache and ledger component outcomes disclose the
reason and fresh-analysis regeneration route. Missing/legacy, malformed,
unsupported or unreadable corpus provenance refuses as
`unverifiable_subject` with a named `source_subject_corpus_*` reason and run
status `cache_limited`. Missing language provenance refuses as
`source_subject_corpus_scope_missing`; a preview sibling cannot bypass it.
Command-time packet reads share this admission boundary.

Known preview/mixed artifacts preserve existing named-file admission and
workspace status explicitly discloses
`preview_or_mixed_corpus_provenance_unavailable`. This Rust-only corpus guard
does not establish their currentness or promote their support tier. It adds
no blanket HEAD gate, configuration-semantic or before/after-lineage claim.
Empty commits, same-input branches and dirty non-corpus files do not invalidate
the Rust content authority. Explicit absolute-root historical rerun remains
a current-scope recomputation path independent of passive admission.

Discriminating controls use real capture → ledger → fresh framed LSP sessions:
same-HEAD unrelated-test changes with unchanged named digests refuse; fresh
analysis regenerates a meaningful nonempty admitted result. Cold/warm analysis,
delayed derivation, legacy provenance, scope distinction and active historical
recomputation are independently controlled in `gap_corpus_currentness.rs`.

### Severity rules

**Finding diagnostics** (`diagnostic_for_finding_with_config`):

A finding is advisory when it has `static_limit_kind.is_some()` or
`language_status == Some(LanguageStatus::Preview)`.
Advisory findings are clamped to INFORMATION (never WARNING) because they
lack a complete repair packet by definition.

**Gap-record diagnostics** (`gap_record_diagnostic_severity`):

A gap record has a complete repair packet when:
- `repairability == "repairable"`, AND
- `verification_commands` is non-empty and every legacy display is non-whitespace
  after trimming, AND
- `receipt_command` is present and non-whitespace after trimming.

A gap record is advisory when `language_status == "preview"` or
`static_limit_kind.is_some()`.

Severity:
- WARNING only when `has_complete_packet && !is_advisory`.
- INFORMATION otherwise.

**Seam diagnostics** (structural grip-class signals, exempt):

Seam diagnostics carry structural grip-class signals, not repair packets.
The WARNING/INFORMATION mapping is owned by `SeverityConfig` (per
`diagnostic_severity_for_grip_class_with_config`). No repair-packet
completeness check applies. This exception is documented in the code.

### Finding message

A finding's Problems-panel text is its reconciled next step. When that step
is the generic one and the finding's witness names a first missing
discriminator that the step does not already quote, the message ends with
` Missing check: `<value>`.` so editors that show only the message still name
the concrete input or value. Preview findings, an empty value, a value
longer than 80 characters (counted as characters) and a value containing a
backtick or newline leave the message unchanged. This changes wording only; class, severity and actionability are untouched.

### Overlapping probe families (#11453)

One changed expression can carry several probes of different families, for
example `predicate` and `return_value` on `return a >= b;`. Findings that carry
a canonical gap group by that gap. Gap-less findings publish one diagnostic
together only when they share file, line, column, owner and expression and are
equal in every field except identity and the probe-family seeding (`id`,
`probe.id`, `probe.family`, `probe.delta`, `probe.expected_sinks`,
`probe.required_oracles`): class, RIPR stages, confidence, evidence, missing
facts, related tests, next step, static-limit and currentness state included.
Any differing site or repair signal keeps findings separate; every probe stays
listed in `data.raw_findings` with `group_reason` `same_site_and_repair_signal`.
The grouping owner is `canonical_group_members`, shared by publication, counts
and withheld disclosure; actionability is unchanged.

### Limited / stale run policy

When `snapshot_run_status` returns anything other than `"full"`
(i.e. `"stale"`, `"cache_limited"`, `"limited"`,
`"limited_incomplete_input"`, or `"seams_deferred"`):

- Finding diagnostics that would be WARNING are downgraded to INFORMATION.
- Seam diagnostics that would be WARNING are downgraded to INFORMATION.
- Gap-record diagnostics are suppressed entirely (none emitted).

The limited state is surfaced by `ripr.collectWorkspaceStatus`, not by
per-file diagnostic spam.

`snapshot_run_status` is computed from findings (any `static_limit_kind`
present → `"limited"`) and gap-artifact rejections
(`StaleArtifact` → `"stale"`, any other rejection → `"cache_limited"`,
nothing → `"full"`). The interactive open/save path additionally defers the
full-repo seam inventory and reports `"seams_deferred"` when no other
limitation applies; see RIPR-SPEC-0105. `"seams_deferred"` is a member of this
limited family for the downgrade/suppression policy above.
An incomplete or unsupported producer-owned `AnalysisOutcome` maps to
`"limited_incomplete_input"`; the typed outcome is retained on the snapshot
and status surfaces, and it cannot authorize full-run gap diagnostics.

## Non-Goals

- HINT severity (needs a `ConfigSeverity::Hint` variant) — deferred.
- ERROR-for-regressed-gap (needs ledger diffing) — do not implement.
- No config knob for the WARNING threshold on gap records — v1 is
  hardcoded to the packet-completeness rule.

## Required Evidence

- Spec registered in `docs/specs/README.md` and `policy/doc-artifacts.toml`.
- 7 reject-list tests in
  `crates/ripr/src/lsp/diagnostics.rs::diagnostic_policy_tests`.

## Acceptance Examples

- A finding with `static_limit_kind = Some(DynamicDispatch)` and class `WeaklyExposed`
  emits a diagnostic with severity INFORMATION, not WARNING, regardless of the base
  `ConfigSeverity::Warning` mapping for that class.
- A finding with `language_status = Some(Preview)` emits INFORMATION even when the
  exposure class maps to WARNING by default config.
- A gap record with `repairability = "repairable"`, an all-non-whitespace
  `verification_commands` list, a non-whitespace `receipt_command`, and
  `language_status = "stable"` emits WARNING.
- The same gap record with `receipt_command = None` emits INFORMATION.
- A gap record with `language_status = "preview"` and an otherwise complete packet
  emits INFORMATION.
- When `snapshot_run_status` returns `"stale"` (a `StaleArtifact` rejection is
  present), no gap-record diagnostics are emitted and finding/seam WARNINGs are
  downgraded to INFORMATION.
- When a zero-finding snapshot carries an incomplete typed analysis outcome,
  its run status is `"limited_incomplete_input"`, no gap-record diagnostics
  are emitted, and finding/seam WARNINGs are downgraded to INFORMATION.

## Test Mapping

- `no_warning_for_finding_with_static_limit` — WeaklyExposed + static_limit_kind=Some → INFORMATION.
- `no_warning_for_preview_finding` — WeaklyExposed + language_status=Preview → INFORMATION.
- `warning_only_when_gap_record_has_complete_packet` — complete packet → WARNING; missing verify or receipt → INFORMATION.
- `no_warning_for_preview_gap_record` — complete packet + language_status=preview → INFORMATION.
- `no_warning_for_static_limit_gap_record` — complete packet + static_limit_kind=Some → INFORMATION.
- `limited_run_downgrades_finding_warnings` — snapshot with static_limit finding → run_status="limited" → finding WARNING downgrades to INFORMATION.
- `stale_run_suppresses_gap_record_diagnostics` — StaleArtifact rejection → run_status="stale" → gap records suppressed (is_full_run=false).
- `generic_next_step_names_the_missing_check_when_the_witness_has_one` — generic next step + witness missing value → message ends with ``Missing check: `value`.``; a step that already quotes the value, no or whitespace-only value, a value over 80 characters or containing a backtick or newline, or a preview finding is unchanged; padding is trimmed and multibyte values count by character.
- `gapless_family_overlap_publishes_one_diagnostic_with_every_probe_listed` / `gapless_overlap_keeps_findings_apart_when_any_repair_signal_differs` — overlapping gap-less probe families collapse to one diagnostic in either input order with all probes in `raw_findings`; any differing class, next step, evidence, line, column, expression, owner or a canonical gap keeps them separate.
- `typed_incomplete_outcome_never_projects_as_full` — incomplete producer outcome → run_status="limited_incomplete_input" → full-run repair diagnostics suppressed.

## Implementation Mapping

- `crates/ripr/src/lsp/diagnostics.rs` — `finding_is_advisory`,
  `gap_record_has_complete_packet`, `gap_record_is_advisory`,
  `gap_record_diagnostic_severity`, `snapshot_run_status`, and the
  assembly policy in `workspace_diagnostics_with_config`.

## Metrics

- `lsp_diagnostics_policy_warning_rate_advisory_zero` — no advisory finding
  or gap record emits WARNING (enforced by test reject list).
- `unit_test_pass_rate`
