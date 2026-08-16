# RIPR-SPEC-0153: Rust producer-owned source role

Status: proposed

Issue: #3283 (parent #3213; builds on #3273 and #3286)

## Problem

Source role was derived independently by each consumer from path
fragments: `rust_index::is_test_file` covered `tests/**` for the diff
path, `workspace::is_production_rust_path` excluded `benches/` and
`examples/` for the repo path, and Cargo target declarations were never
read at all. A changed Cargo bench seeded production probes for harness
plumbing (`criterion_group!`, `criterion_main!`), an explicit `[[test]]`
target outside `tests/` could not confirm evidence role, and a confirmed
`*_test.rs` filename convention was indistinguishable from an ordinary
production file.

## Behavior

One typed, producer-owned role model (`SourceRole` in
`analysis/workspace/source_role.rs`) classifies each Rust file:

```text
production_subject                ordinary production: seeds probes/seams
test_evidence                     tests/** layout or declared [[test]] target
bench_evidence                    benches/** layout or declared [[bench]] target
example_evidence                  examples/** cargo-discoverable shapes
fixture_or_receipt_evidence       fixtures/, target/, .git/, .ripr/, ...
production_like_test_infrastructure  explicit opt-in target
unknown_role                      reserved for ambiguous inputs; test-only
```

Role is derived from authoritative context in priority order:

1. the `[analysis] production_like_targets` opt-in in `ripr.toml`
   (workspace-relative paths; absolute or backslashed values fail closed
   at parse time) — restores ordinary production analysis for the
   selected target only;
2. declared Cargo targets: explicit `path = ...` entries on `[[test]]`
   and `[[bench]]` confirm evidence role outside the default layouts.
   Autodiscovered targets live under `tests/`/`benches/` by construction,
   so `autotests = false` / `autobenches = false` need no separate
   handling; malformed manifests yield no targets and layout keeps
   applying (fail-closed toward production);
3. package layout: `tests/` anywhere and a `tests.rs` stem (matching
   the pre-existing contracts); `xtask/`, non-source directories, and
   files without a `src` component (the full pre-#3283 repo production
   contract — routing the repo set through the role cannot widen it);
   `benches/` and `examples/` only in Cargo-autodiscovery shapes
   (`<dir>/<name>.rs`, `<dir>/<name>/main.rs`) — nested src layouts like
   `examples/sample/src/lib.rs` are NOT discoverable targets and stay
   production subjects;
4. everything else is a production subject. A filename convention alone
   (`*_test.rs`, `test_*.rs`) never classifies.

Both diff probe seeding and the repo seam-inventory production set route
through `classify_with`; `benches/**`/`examples/**` harness plumbing no
longer seeds production obligations, closing the diff gap while the repo
exclusion stays consistent. Evidence-role files remain fully indexed:
functions stay available for owner relations, activation input,
sink/oracle evidence, and selectors. `TestFact` semantics are untouched
— source role never registers a helper as an executable test selector.

The opt-in joins the check-artifact config identity as
FindingAffecting (`CHECK_ARTIFACT_CONFIG_IDENTITY_VERSION` 1 → 2) and
the repo-exposure consumed-config list; the workspace cache key already
hashes the `ripr.toml` text, so no generation bump is required for it.

## Required Evidence

- A changed Cargo bench seeds no production probes but stays in
  changed-file accounting: pinned by the `benches_harness_evidence`
  corpus fixture (four bench findings on main, zero after the slice) and
  an in-crate regression test verified failing on main.
- A declared `[[test]] path="src/contract_test.rs"` target's plain
  harness helper seeds no production probes while the same-shaped
  undeclared `src/unconfirmed_test.rs` stays a production subject;
  disabling the declared-target branch makes the fixture fail
  (discriminating-power proof).
- The opt-in restores production analysis for the selected target only;
  sibling test targets stay evidence-only.
- Layout pins: nested `examples/<dir>/src/**` and
  `benches/<dir>/src/**` are production; `<dir>/<name>.rs` and
  `<dir>/<name>/main.rs` are evidence; `tests/` any-segment remains
  evidence; `src/test_helper.rs` remains production.
- Manifest parsing pins: explicit paths extracted, entries without
  `path` contribute nothing, malformed TOML yields no targets,
  autodiscovery flags do not drop explicit targets.
- Config pins: the opt-in parses as relative paths, joins the identity
  canonically, and absolute paths fail closed with a named error.
- `#3273`'s inline `#[cfg(test)]` controls and `#3286`'s helper-evidence
  regression tests remain green.

- The repo production contract carries over exactly: `xtask/`, files
  without a `src` component, and `tests.rs` stems stay non-production;
  the single declared divergence is nested-src layouts under
  `examples/`/`benches/` (not Cargo-discoverable targets, production
  in diff mode since before #3283).
## Required guards

- No filename-only classification: an unconfirmed convention name is a
  production subject.
- The opt-in never leaks to sibling targets.
- Evidence files stay indexed; no evidence path is dropped from the
  index or from related-test discovery.
- `TestFact` registration remains attribute-driven only.

## Acceptance Examples

- Accept: `benches/exposure.rs` changed → indexed, counted as a changed
  file, zero production findings.
- Accept: `[[test]] path="src/contract_test.rs"` → helper in it is
  evidence; `src/unconfirmed_test.rs` without a declaration → production.
- Accept: `production_like_targets = ["tests/api_contract.rs"]` → that
  file analyzed as production; `tests/other.rs` stays evidence.
- Reject: a bench harness call (`criterion_main!`) becoming an
  obligation; a `test_*.rs` name alone excluding a production file.

## Test Mapping

`analysis/workspace/source_role.rs` unit tests pin the layout rules,
Cargo-autodiscovery shapes, override priority, windows normalization,
and the seeding/evidence partition. `analysis/workspace/cargo_targets.rs`
pins manifest extraction. `analysis/language/rust.rs` pins diff seeding
(bench gap regression, declared-target confirmation with a probeable
helper, opt-in restore). `config/tests.rs` pins parsing, identity
classification, and absolute-path rejection.

## Non-Goals

- No assertion-form parity (#3284 owns it).
- No public output/gate role projection (#3285 owns it).
- No cache-generation bump: the workspace cache key hashes the
  `ripr.toml` text, and the fact/classification schemas are unchanged by
  this slice (`FunctionFact.is_test` semantics untouched).
- No generated-file or custom-harness classification (`UnknownRole`
  reserved, test-only until a real producer exists).

## Implementation Mapping

- `analysis/workspace/source_role.rs` — the typed model.
- `analysis/workspace/cargo_targets.rs` — manifest enumeration,
  workspace-root-anchored.
- `analysis/language/rust.rs` — diff seeding and repo production set.
- `analysis/seam_inventory.rs` — inventory and count production sets.
- `config.rs` + `config/model.rs` — the opt-in, its identity role, and
  the consumed-config list.

## Metrics

No new metric; existing counts now exclude bench/example harness
plumbing from production denominators by the same authority.
