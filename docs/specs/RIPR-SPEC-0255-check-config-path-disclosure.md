# RIPR-SPEC-0255: Check config path disclosure

Status: proposed

Owner: ripr-swarm

Created: 2026-10-12

Linked issues: #7536, #7468

Support-tier impact:

- None. This spec discloses which already-loaded `ripr.toml` governed a
  `ripr check` run; it changes no language support, platform, or packaging
  claim. See [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md).

Policy impact:

- None. No new process, network, or file-policy surface. The disclosed path
  is the config file the run already read.

## Problem

`ripr doctor` names the loaded config (`loaded ripr.toml at <path>`), and
#7535 warns on stderr when an implicit `check` root skips a closer
`ripr.toml`. But `ripr check` itself shows only `mode:` in its human header
plus an unresolvable `config_identity` fingerprint in JSON. A run that did
load a file — workspace root, explicit `--root`, or no skip — has no stable
machine-readable path, so CI logs that discard stderr and JSON consumers
cannot tell which file governed (#7536).

## Behavior

- Human header: when a `ripr.toml` was actually loaded for the run, the
  header prints `config: <path>` directly after the `root:` line, rendered
  with the same human path rule `ripr doctor` uses. When no file was loaded,
  the header is byte-identical to before — absence is the explicit signal
  for built-in defaults, never a fake path.
- Check JSON: when a `ripr.toml` was actually loaded, the top-level envelope
  carries `config_path` (stable slash-separated spelling, like `root`)
  directly after `root`. When no file was loaded, the member is absent.
  Additive in check schema `0.2`; no `schema_version` bump.
- Both surfaces read the one producer fact — the loaded config's
  `source_path` — through the render-time config the CLI already passes.
  A bound `--candidate-tree` subject discloses no path: its governing file
  lives in the immutable tree, its `source_path` is cleared by design
  (#3279), and its config provenance stays the `identity.config_identity`
  fingerprint of the tree text. Claiming a worktree path for it would name
  a file that did not govern.
- No change to which file `check` loads, to config discovery, or to the
  `doctor`/`pilot` surfaces.

## Controls

1. A human render with a loaded config names its path; a defaults render
   contains no `config:` line.
2. A JSON render with a loaded config emits `config_path` naming the file;
   a defaults render omits the member entirely.
3. The disclosed path is the file that governed: two runs loading different
   files name different paths, and editing only render-time presentation
   never changes which path is named.

## Acceptance Examples

- Given a workspace whose root holds a `ripr.toml`, when the developer runs
  `ripr check`, then the human header reads `root: <root>` followed by
  `config: <root>/ripr.toml`, and `ripr check --format json` carries
  `"config_path": "<root>/ripr.toml"` after `"root"`.
- Given a workspace with no `ripr.toml`, when the developer runs either
  form, then no `config:` line and no `config_path` member appear.
- Given a bound `--candidate-tree` subject whose tree carries a
  `ripr.toml`, when the developer runs either form, then no config path is
  disclosed (the file is not in the worktree), and the tree config's
  fingerprint appears as `identity.config_identity`.

## Test Mapping

- `crates/ripr/src/output/human.rs::tests::header_names_the_loaded_config_path`
- `crates/ripr/src/output/human.rs::tests::render_includes_summary_counts_and_empty_findings_message`
- `crates/ripr/src/output/json/report.rs::harness_projection_tests::config_path_names_the_loaded_file_and_defaults_omit_it`

## Implementation Mapping

- `crates/ripr/src/output/human.rs`: `render_header_summary` prints the
  loaded config path after `root:` from the render-time config.
- `crates/ripr/src/output/json/report.rs`: `render_with_config` emits the
  top-level `config_path` member after `root` from the same config.
- `schemas/ripr/check.schema.json`: optional `config_path` member.
- `docs/OUTPUT_SCHEMA.md`: envelope documentation.

## Metrics

- Every check render with a loaded config names that exact file on both
  surfaces (pinned by the focused render tests); defaults-only renders stay
  byte-identical to before this change.

## Required Evidence

- Focused render tests for both surfaces (Test Mapping below), green.
- `cargo xtask check-output-contracts`, `check-traceability`,
  `check-spec-format`, and `check-spec-numbering` green.
- A real-binary run against a workspace with a `ripr.toml` showing the
  `config:` header line and `config_path` member, and a defaults-only run
  showing neither.

## Non-Goals

- Changing which config file `check` loads (per-crate overlay stays
  #2671 / #2965).
- Rebinding `doctor`/`pilot` root resolution to make the commands agree.
- Disclosing the path in SARIF, GitHub, badge, or diff-report formats.
