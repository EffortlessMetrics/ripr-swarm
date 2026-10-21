# RIPR-SPEC-0007: Repository Configuration

Status: proposed

## Problem

`ripr` needs a repository-owned configuration surface before SARIF, CI policy,
badge remapping, and editor defaults can become predictable for real
workspaces.

Without a checked-in config file, each entry point has to rely on command-line
flags, editor initialization options, or built-in defaults. That makes CLI,
LSP, reports, and future CI policy harder to keep aligned.

## Behavior

`ripr` should discover and parse the nearest repository-owned `ripr.toml` file
from the selected root.

The configuration layer should:

- walk from the selected root toward its parents for the nearest `ripr.toml`;
- stop after checking a Cargo manifest with a `[workspace]` table or at a
  `.git` boundary; a package-only `Cargo.toml` does not stop discovery;
- use behavior-preserving defaults when the file is absent;
- treat a present `ripr.toml` directory entry, including a dangling or
  self-referencing symlink, as present rather than as built-in defaults
  (`load_for_root`, workspace-status presence, Python repair
  config-profile detection, first-pr preflight, and agent-receipt
  config fingerprinting share that presence rule);
- reject malformed config with actionable errors;
- reject unknown keys so typos do not silently change policy;
- keep explicit CLI options ahead of repository config;
- keep explicit LSP initialization options ahead of repository config;
- allow repository config to set the default analysis mode;
- allow repository config to set whether unchanged tests are included;
- allow repository config to set oracle-strength policy for supported oracle
  shapes;
- allow repository config to set finding and seam diagnostic severity policy;
- allow repository config to point at a relative suppressions file;
- allow repository config to set report related-test caps where the command
  supports that cap;
- make loaded, missing, and malformed config state observable through
  `ripr doctor`;
- provide `ripr config validate [--root PATH]` for isolated ancestor-aware TOML
  validation without workspace tool probes;
- keep output schemas stable unless a later scoped PR explicitly adds config
  metadata.

Precedence is:

```text
explicit CLI or LSP option > ripr.toml > built-in default
```

Scoped amendment (RIPR-SPEC-0136): when the LSP client supports the
server-originated `workspace/configuration` pull model, the five governed LSP
session keys resolve per key as

```text
valid pulled setting > LSP initialization option > ripr.toml > built-in default
```

so initialization options become the compatibility fallback for keys the pull
did not return. In the push-fallback and initialization-only modes the
precedence above is unchanged.

### Literal artifact-root ignore advice (#8751, #9011)

For an in-repository artifact root selected from `RIPR_CACHE_DIR` or
`[perl].cache_dir`, doctor's artifact-ignore warning and the changed-scope
artifact disclosure must recommend a root-anchored, directory-only Gitignore
rule for that literal root. Escape `\`, `*`, `?`, `[` and `]` in the rule;
ordinary roots retain their existing spelling. For example, `build/cache*`
requires `/build/cache\*/`, which covers its descendants without ignoring
source siblings such as `build/cache1/user.rs` or `build/cache-other/user.rs`.

Gitignore rule encoding is separate from shell quoting and Git pathspec
semantics. The ignore probe still passes the plain filesystem pathname to
`check-ignore`; tracking and untracking retain plain roots under Git's global
`--literal-pathspecs` mode, and emitted shell commands retain argument quoting.
This obligation does not change artifact-root ownership, input filtering,
configuration precedence, or output schemas. Newline-containing and non-UTF-8
path advice is outside this bounded contract.

### Doctor blocking and advisory marks (#9033)

Doctor's human output marks a blocking failure with `!` and an advisory with
`~`. A failed core check or a failed required runtime probe is blocking: it
uses `!` and the completed run exits 3 (#5102). An advisory never changes the exit status: the
artifact root not covered by git ignore rules, the artifact root tracked by
git, and an unwritable cache each use `~`. `ripr doctor --help` documents both
marks. This obligation does not change JSON fields or output schemas.

### Suppression manifest declarations

The configured suppression file and explicit `--suppression-policy` input share
`parse_suppressions_manifest`. Repeated top-level `schema_version` headers and
repeated field names within one `[[suppressions]]` entry are invalid, including
identical values and empty or malformed first values. Each duplicate diagnostic
names the field, repeated declaration line, and original declaration line;
third declarations still cite the first line. Later declarations do not overwrite the
first parsed value or declaration identity. Both loaders reject the manifest
when violations are present.
Both loaders name the supplied policy path in every parser diagnostic. The
implicit loader uses the configured root-relative path; duplicate diagnostics
retain the original and repeated declaration lines. Diagnostic values and
terminal escaping are preserved.

Declaration tracking resets for each entry, so field names may be reused in
separate entries. This is distinct from the existing duplicate exact-selector
rule across entries: repeating a `finding_id` or `(test, path)` selector is
invalid. Duplicate path-glob selectors are outside that exact-selector check.

Supported bare keys, double-quoted entry string values, and the integer schema
header retain their existing syntax. Single-quoted string values and quoted
keys remain unsupported. This repair adds no broader TOML or calendar support.

### Unused default suppression disclosure (#7759)

After successful analysis, `ripr check` in human, human-full, JSON or GitHub
format emits one stderr line when no explicit `--suppression-policy` was
supplied and `.ripr/suppressions.toml` resolves to a file under the analyzed
root. The line names the unused default path and the explicit flag needed
to apply it. It is a presence disclosure, not validation: malformed unused
content remains nonfatal. The command does not parse or apply this file,
change stdout or change exit semantics. A foreign working-directory file,
a missing file, or a directory at that path does not trigger the note.
An explicit policy retains its loader and typed refusal contract from
RIPR-SPEC-0241. Implicit SARIF/badge and repository consumers do not receive
a false skipped-policy note and keep their existing policy authority.

## Required Evidence

Repository configuration evidence should cover:

- absent config preserving previous defaults;
- valid config changing each supported policy surface;
- nested selected roots discovering the nearest config and respecting
  repository boundaries;
- explicit CLI options overriding config values;
- explicit LSP initialization options overriding config values;
- malformed values returning actionable errors;
- unknown keys failing closed;
- duplicate suppression headers and same-entry fields naming both declaration
  lines and failing closed through the implicit and explicit loaders;
- empty or malformed first values and third declarations retaining the original
  declaration identity, with valid field-name reuse across separate entries;
- unsafe relative-path shapes being rejected where paths are configurable;
- escaped NUL characters in relative paths being rejected before filesystem
  use, while Unicode, spaces, and existing path normalization remain supported;
- empty or NUL-bearing `perl.executable` values failing shared configuration
  validation with a key-named diagnostic that does not echo the path; omitted
  values keep the default and valid paths retain exact spelling and the existing
  user opt-in boundary, without existence or execution probes;
- `ripr doctor` never executing a Perl exporter PATH hit whose canonical
  path lies under the analyzed root unless
  `RIPR_ALLOW_REPO_PERL_EXECUTABLE=1` is set (#5349): the hit is reported as
  refused (not run) with the canonical resolved path inside the checkout as the reason
  and the opt-in / move-outside-the-repo next step; a symlink or relative
  PATH entry pointing into the root is refused by canonicalization, while a
  relative PATH entry resolving outside the root still probes as before;
  the refusal is shown on the default (non-verbose) human screen as well
  as in `--verbose` and as `exporter.state: "refused"` in `--json`, names
  the canonical path that was judged, and takes precedence over an
  incompatible hit for the other exporter name; a candidate that cannot
  be canonicalized is judged by its ancestor-resolved location (the
  longest existing ancestor is canonicalized and the remaining suffix
  appended lexically) and is refused when that location lies inside the
  root, so unresolvable in-checkout candidates fail closed while
  provably outside candidates still probe; an approved PATH hit is
  executed through the canonical path that was judged, so swapping the
  relative or symlink PATH spelling after judgement cannot redirect
  execution (residual: an actor who can replace the canonical target
  file itself, outside the checkout, between judgement and spawn can
  still affect what runs; doctor does not claim to close that race);
- `ripr doctor` reporting loaded config path, missing-config defaults, and
  malformed config errors without printing config source text;
- `ripr doctor` separating installed-binary analysis readiness (the default
  `analysis` profile) from RIPR source-build prerequisites
  (`--profile source-build`), with the Cargo/rustc probes run in the selected
  root;
- `ripr config validate` accepting valid config, following the existing
  missing-config defaults path, and returning path-qualified errors for
  malformed or policy-invalid config;
- `ripr config validate --root --root` reporting a missing path even when a
  directory named `--root` exists; `--root ./--root` still selects that directory;
- this option-token rule remaining local to `config validate`; other dash-prefixed
  directory paths remain accepted, and shared value parsing is unchanged;
- output schemas remaining unchanged when severity or report caps come from
  config;
- literal artifact-root rule advice from both consumers, with a real Git
  descendant-positive/source-sibling-negative control and separate raw-path
  versus encoded-rule assertions;
- docs and example config staying aligned with supported keys.

## Non-Goals

This spec does not require:

- SARIF output;
- CI blocking policy;
- badge count remapping;
- user-global config;
- hidden `.ripr/ripr.toml` discovery;
- automatic config migration;
- broad analyzer refactors;
- unsaved-buffer overlays or deep editor analysis by default.

## Acceptance Examples

A valid default policy matching `src/**` leaves ordinary check findings
unchanged without the flag, while stderr explains how to apply it. With
`--suppression-policy .ripr/suppressions.toml`, matching findings are marked
suppressed and no skipped-policy note appears. An invalid unused file keeps
the ordinary JSON document unchanged; the same file supplied explicitly
produces the existing non-consumable `suppression_policy_invalid` refusal.

### Missing config preserves defaults

```text
Given a workspace without ripr.toml,
when ripr check runs,
then the command uses built-in defaults that match the generated conservative
policy profile.
```

### Repository config sets defaults

```text
Given ripr.toml sets analysis.mode = "deep",
when ripr check runs without an explicit mode flag,
then the analysis input uses deep mode.
```

### Explicit CLI options win

```text
Given ripr.toml sets analysis.mode = "deep",
when ripr check runs with --mode fast,
then the analysis input uses fast mode.
```

### Explicit LSP initialization options win

```text
Given ripr.toml enables LSP seam diagnostics,
when the editor sends initializationOptions.seamDiagnostics = false,
then the LSP server keeps seam diagnostics disabled for that session.
```

### Valid pulled LSP settings win over initialization options (pull mode)

```text
Given the LSP client supports workspace/configuration,
and initializationOptions.checkMode = "fast",
when the pulled ripr section returns checkMode = "ready",
then the LSP session uses ready mode,
and initialization options supply only the keys the pull did not return.
```

### Malformed config is actionable

```text
Given ripr.toml contains an unknown key or invalid value,
when ripr loads the config,
then the user-facing error names the config path and parse problem.

Given --candidate-tree selects an immutable tree with malformed ripr.toml,
when check refuses that config,
then the human diagnostic explains how to fix or remove the candidate's file
and rerun with a new candidate-tree identity; editing only the worktree file
cannot repair that immutable subject. A config-free candidate uses the built-in
defaults, not a parent or worktree config. The JSON refusal retains its redacted
first-line message, config_invalid identity and analysis/config-load route.
```

### Duplicate suppression declarations fail closed

```text
Given one suppression entry declares expires twice,
when either suppression loader reads the policy,
then the policy is rejected with the repeated and original declaration lines;
and an explicit --suppression-policy error names the supplied file.

Given the top-level schema_version header is declared twice,
when the policy is loaded,
then duplicate-header diagnostics name both declaration lines and loading fails.

Given two suppression entries reuse owner, reason, and expires field names
with distinct exact selectors,
when the policy is loaded,
then those field-name declarations remain valid;
and repeating an exact finding_id or (test, path) selector across entries still
fails under the separate exact-selector rule.
```

### Doctor makes config state inspectable

```text
Given ripr doctor runs for a workspace,
when repo config is loaded, missing, or malformed,
then doctor reports the config path or default state and never prints the
config source text.
```

### Implicit check names a closer config it skipped

```text
Given a Cargo workspace with ripr.toml at the workspace root and a different
ripr.toml in a member crate,
when ripr check runs from that member without --root,
then the workspace file still governs the run,
and stderr names both files and which one governed,
and `--root .` from that directory loads the closer file instead.
```

### Doctor enable tip produces a loadable configuration

```text
Given a workspace with detected source for a preview language whose adapter
is compiled into this binary and eligible for an enable tip (TypeScript and
Python; Perl never gets a tip), and that language is not enabled,
when ripr doctor runs,
then doctor prints one `[languages] enabled` snippet that keeps every language
already enabled, names only values `languages.enabled` accepts (JavaScript
maps to `typescript`), and names the enable step beside the recommended first
command;
and a workspace with no such language gets neither.
```

### Present dangling or unreadable config is not built-in defaults

```text
Given a workspace whose ripr.toml is a symlink to a missing file,
or a regular file whose lookup or read fails with a non-absence error,
when first-pr preflight or an agent-receipt config fingerprint runs,
then the config is present but unreadable,
and it is not described as using built-in defaults.
```

### Doctor exit codes distinguish findings, usage errors, and clean runs (#5102)

```text
Given a workspace where a doctor check fails,
when ripr doctor completes its diagnosis (human or --json),
then the full report is written and doctor exits 3 via
CommandError::Decision, with stderr naming `doctor found issues`;

Given an invalid doctor invocation (for example `ripr doctor --bogus`),
when no check has run,
then doctor exits 2 via CommandError::Failure;

Given a workspace where every doctor check passes,
when ripr doctor runs,
then doctor exits 0.
```

Operational failures before a report exists stay exit 2.
`ripr help --json` projects this as the `doctor_findings` exit kind for
`cmd:doctor`.

### Doctor separates analysis readiness from source-build prerequisites

```text
Given a Rust workspace whose selected toolchain is missing cargo or rustc, or
whose rustc is older than RIPR's build MSRV,
when ripr doctor runs with the default analysis profile,
then the cargo and rustc checks are reported as advisory, the report status is
pass, and doctor exits 0;
and a missing cargo discloses that evidence read from `cargo metadata` is
withheld.

Given the same workspace,
when ripr doctor runs with --profile source-build,
then the cargo and rustc checks fail, the report status is fail, and doctor
exits 3;
and a missing cargo or rustc names an install route on its own failing line;
and the run does not print the analysis-profile start-here packet, recovery
states, proof-rail note, or "Recommended first command: ripr check";
and enabled language runtimes are reported but do not decide that profile's
status.

Default human `ripr doctor` is a compact screen (≤24 physical lines) with
failures first and a `Fix:` line each; `-v`/`--verbose` restores the full
detail dump. `ripr doctor --json` remains the complete typed report. Compact
`--profile source-build` still reports enabled primary language runtimes as
advisory lines; optional extra tools stay behind verbose. Compact still
discloses a refused `[perl].executable` (not run). A failing `cargo_toml`
or required runtime does not recommend `ripr check`. The compact enablement
tip is one physical line and names `enabled = [...]` under `[languages]`,
not as same-line TOML. A tool probe that ran and then exited nonzero
(`git --version failed`) or timed out gets a `Fix:` that names repair of
the binary on PATH, not a missing-tool install; JSON evidence is
unchanged. Compact advisory lines spell control characters. Human path
and evidence text spell control characters so a newline-bearing root stays
within the line budget; JSON keeps the raw path. Verbose config failures
keep source-free parser reasons and the allowlisted misplaced-key hint.

Given a selected root with its own rustup toolchain selection,
when doctor probes cargo or rustc,
then the probe runs in the selected root rather than the caller's directory.

Given a git root whose Cargo workspace lives in a subdirectory (for example
`rust/Cargo.toml` with `[workspace]` and no Cargo.toml at the git top level),
when ripr doctor runs from that git root,
then the cargo_toml check passes and names the nested manifest (Cargo.toml
found at rust/Cargo.toml via workspace root rust/);
and a root with Rust sources but no Cargo.toml, or an empty root under the
Rust-only default, still fails cargo_toml;
and doctor's selected --root is not rebound to the nested workspace.

Given a git root with a TypeScript marker (`package.json` or `tsconfig.json`)
and a nested Cargo workspace (`rust/Cargo.toml`) and no Cargo.toml or `.rs`
files at the selected root,
when ripr doctor runs from that git root,
then the cargo_toml check stays required and passes naming the nested
manifest;
and a TypeScript-only or Python-only root with no nested or ancestor
Cargo.toml still skips cargo_toml;
and `[languages] enabled = ["typescript"]` still skips even when a nested
Cargo.toml exists;
and doctor's selected --root is not rebound;
and language detection remains a shallow root scan (the locator stays
cargo-only).
```

### Literal artifact-root rule excludes no source sibling

Given the configured artifact root `build/cache*` exists without an ignore
rule, doctor's warning recommends `/build/cache\*/`. When that exact advice
is written to `.gitignore`, Git ignores `build/cache*/nested/a.json` and leaves
`build/cache1/user.rs` and `build/cache-other/user.rs` unignored; doctor then
reports the artifact root as ignored. The changed-scope disclosure recommends
the same escaped rule when it excluded a file under that root.

### Doctor advisories never block

Given a repository whose ignore rules do not cover the artifact root, or whose
index tracks an artifact root, or whose cache directory is unwritable, doctor
prints each advisory starting with `~ ` and exits with the status of its core
checks alone; only a failed core check or required runtime probe prints `!`
and exits 3.

### Doctor refuses a checkout-local Perl exporter on PATH (#5349)

```text
Given a Perl checkout with `bin/perllsp` on a relative or checkout-local PATH,
when ripr doctor runs without RIPR_ALLOW_REPO_PERL_EXECUTABLE=1,
then doctor reports the exporter as refused (not run), states that the
resolved path is inside the analyzed checkout, and names the opt-in or
moving the exporter outside the repo as the next step;
and the exporter is never executed.
Given the same checkout with RIPR_ALLOW_REPO_PERL_EXECUTABLE=1, or an
exporter outside the root reached through a relative PATH entry,
when ripr doctor runs,
then the exporter is probed as before.
Given a refused `perl-ripr-facts` and a version-only `perllsp` outside
the root, when ripr doctor runs, then the report is the refusal, not
the incompatible hit.
Given a PATH candidate that cannot be canonicalized but whose
ancestor-resolved location lies inside the root, when ripr doctor
runs without the opt-in, then it is refused and never executed.
Given an approved exporter reached through a symlink, when ripr
doctor runs, then doctor executes the canonical target it judged,
not the symlink spelling.
```

## Test Mapping

- `crates/ripr/src/analysis/diff/path.rs::tests::artifact_root_gitignore_entry_escapes_patterns_separately_from_paths`
  pins literal rule escaping and unchanged raw path spelling.
- `crates/ripr/src/cli/commands/doctor.rs::tests::ripr_artifact_tracking_untrack_names_only_indexed_roots`
  pins the `~ ` advisory mark on the tracked-artifacts warning.
- `crates/ripr/src/cli/commands/doctor.rs::tests::ripr_artifact_ignore_names_the_gitignore_entry`
  pins the `~ ` advisory mark on the not-ignored artifact-root warning.
- `crates/ripr/tests/chaos_interrupted_runs.rs::doctor_says_when_the_cache_cannot_be_written`
  pins the `~ ` advisory mark and successful exit for an unwritable cache.
- `crates/ripr/src/cli/commands/doctor.rs::tests::ripr_artifact_ignore_advice_covers_only_the_literal_root`
  feeds the actual advice to Git and distinguishes descendants from siblings
  (Unix-only literal-star filesystem fixture).
- `crates/ripr/src/analysis/pipeline.rs::tests::ripr_artifact_disclosure_escapes_gitignore_metacharacters`
  pins the second consumer's emitted literal rule.

- `crates/ripr/tests/default_suppression_disclosure.rs` — real CLI explicit
  opt-in, unchanged full stdout, malformed-unused versus explicit refusal,
  analyzed-root/foreign-CWD/directory controls, and implicit-consumer note
  exclusion. Expected prose and finding paths are literal consumer values.

Current tests:

- `crates/ripr/tests/cli_smoke.rs::config_validate_does_not_consume_root_flag_as_a_path`
- `crates/ripr/src/output/suppressions.rs::audit_controls::rejects_duplicate_owner`
- `crates/ripr/src/output/suppressions.rs::audit_controls::rejects_duplicate_expiry`
- `crates/ripr/src/output/suppressions.rs::audit_controls::rejects_duplicate_header`
- `crates/ripr/src/output/suppressions.rs::audit_controls::valid_policy_preserves_fields`
- `crates/ripr/src/output/suppressions.rs::audit_controls::repeated_fields_in_distinct_entries_are_valid`
- `crates/ripr/src/output/suppressions.rs::audit_controls::every_supported_field_rejects_identical_duplicate`
- `crates/ripr/src/output/suppressions.rs::audit_controls::duplicate_error_names_both_lines_and_preserves_first_value`
- `crates/ripr/src/output/suppressions.rs::audit_controls::blank_value_cannot_be_repaired_by_duplicate`
- `crates/ripr/src/output/suppressions.rs::audit_controls::public_loaders_fail_closed_and_name_the_supplied_policy`
- `crates/ripr/src/output/suppressions.rs::audit_controls::duplicate_empty_optional_values_are_not_silently_replaced`
- `crates/ripr/src/output/suppressions.rs::audit_controls::supported_quoted_values_keep_delimiters_and_empty_metadata`
- `crates/ripr/src/output/suppressions.rs::audit_controls::unsupported_literal_values_and_quoted_keys_still_fail`
- `crates/ripr/src/output/suppressions.rs::audit_controls::repeated_malformed_assignment_preserves_the_original_location`
- `crates/ripr/src/config.rs::tests::missing_config_uses_behavior_preserving_defaults`
- `crates/ripr/src/config.rs::tests::config_load_parse_error_strips_windows_verbatim_prefix`
- `crates/ripr/src/config.rs::tests::parse_error_hint_matches_discovery_after_the_broken_file_is_removed`
- `crates/ripr/src/config.rs::tests::unreadable_config_error_ends_with_a_recovery_line`
- `crates/ripr/tests/cli_smoke.rs::check_json_malformed_config_emits_redacted_refusal`
- `crates/ripr/tests/cli_smoke/candidate_config_recovery.rs::malformed_candidate_config_names_recovery_without_changing_json_refusal`
- `crates/ripr/tests/cli_smoke/candidate_config_recovery.rs::repaired_and_config_free_candidates_ignore_a_broken_worktree_config`
- `crates/ripr/tests/cli_smoke.rs::doctor_verbose_malformed_config_omits_the_loader_recovery_line`
- `crates/ripr/src/workspace_status.rs::tests::dangling_ripr_toml_symlink_is_present_not_built_in_defaults`
- `crates/ripr/src/app/python_repair_binding.rs::tests::dangling_ripr_toml_symlink_is_subject_config_not_defaults`
- `crates/ripr/src/app/python_repair_verification.rs::python_repair_verification_semantics::dangling_ripr_toml_symlink_is_subject_config_not_defaults`
- `crates/ripr/src/output/first_pr/preflight.rs::tests::dangling_ripr_toml_symlink_is_present_not_built_in_defaults`
- `crates/ripr/src/output/first_pr/preflight.rs::tests::unreadable_regular_ripr_toml_is_present_not_ok`
- `crates/ripr/src/output/first_pr/preflight.rs::tests::fifo_ripr_toml_is_present_not_ok_and_does_not_block`
- `crates/ripr/src/cli/commands_agent_support.rs::tests::dangling_ripr_toml_symlink_is_present_not_built_in_defaults`
- `crates/ripr/src/cli/commands_agent_support.rs::tests::unreadable_ripr_toml_lookup_is_not_built_in_defaults`
- `crates/ripr/src/config.rs::tests::config_file_sets_core_operational_defaults`
- `crates/ripr/src/config.rs::tests::closer_config_is_skipped_when_the_analysis_root_is_the_workspace`
- `crates/ripr/src/config.rs::tests::closer_config_is_skipped_when_the_workspace_has_no_ripr_toml`
- `crates/ripr/src/config.rs::tests::closer_config_from_a_nested_source_dir_is_still_skipped`
- `crates/ripr/src/config.rs::tests::closer_config_is_skipped_when_the_files_have_identical_contents`
- `crates/ripr/src/config.rs::tests::closer_config_is_not_skipped_when_invocation_is_the_analysis_root`
- `crates/ripr/src/config.rs::tests::closer_config_is_not_skipped_when_only_the_workspace_has_ripr_toml`
- `crates/ripr/src/config.rs::tests::closer_config_is_not_skipped_when_the_member_file_is_a_symlink_to_the_workspace_file`
- `crates/ripr/src/config.rs::tests::closer_config_names_the_member_entry_when_it_is_a_symlink_to_a_different_file`
- `crates/ripr/src/cli/commands/check.rs::closer_config_warning_tests::skipped_closer_config_warning_names_both_files_and_the_recovery_root`
- `crates/ripr/src/cli/commands/check.rs::closer_config_warning_tests::skipped_closer_config_warning_names_built_in_defaults_when_the_workspace_has_no_file`
- `crates/ripr/src/cli/commands/check.rs::closer_config_warning_tests::skipped_closer_config_warning_keeps_root_dot_when_the_invocation_path_has_an_apostrophe`
- `crates/ripr/src/cli/commands/check.rs::closer_config_warning_tests::skipped_closer_config_warning_stays_quiet_at_the_analysis_root`
- `crates/ripr/src/cli/commands/check.rs::closer_config_warning_tests::skipped_closer_config_warning_names_the_member_entry_when_it_is_a_symlink_to_a_different_file`
- `crates/ripr/tests/cli_smoke.rs::check_from_a_member_crate_warns_when_it_skips_the_closer_ripr_toml`
- `crates/ripr/src/config.rs::tests::explicit_cli_mode_wins_over_config_mode`
- `crates/ripr/src/config.rs::tests::config_mode_applies_when_cli_mode_is_not_explicit`
- `crates/ripr/src/config.rs::tests::malformed_or_unknown_config_is_actionable`
- `crates/ripr/src/config.rs::tests::config_rejects_unsafe_suppression_paths`
- `crates/ripr/src/config.rs::tests::relative_config_paths_reject_nul_before_filesystem_use`
- `crates/ripr/src/config.rs::tests::relative_config_paths_preserve_normalization_spaces_and_unicode`
- `crates/ripr/src/cli/commands/config.rs::tests::validate_rejects_a_nul_path_in_valid_toml`
- `crates/ripr/src/config/perl_executable_tests.rs::perl_executable_rejects_an_empty_path`
- `crates/ripr/src/config/perl_executable_tests.rs::perl_executable_rejects_nul_without_echoing_the_path`
- `crates/ripr/src/config/perl_executable_tests.rs::perl_executable_preserves_valid_paths_and_the_opt_in_boundary`
- `crates/ripr/src/cli/commands/config.rs::tests::validate_rejects_unusable_perl_executable_paths`
- `crates/ripr/src/config.rs::tests::oracle_policy_rewrites_configurable_oracle_strengths`
- `crates/ripr/src/lsp/config.rs::tests::repo_config_sets_defaults_when_initialization_options_are_missing`
- `crates/ripr/src/lsp/config.rs::tests::initialization_options_override_repo_config_defaults`
- `crates/ripr/src/lsp/config.rs::tests::pulled_settings_override_initialization_options_for_returned_keys`
- `crates/ripr/src/lsp/config.rs::tests::repository_reload_preserves_pulled_overrides`
- `crates/ripr/src/app/tests/rendering_contracts.rs::configured_finding_severity_applies_to_human_json_and_github`
- `crates/ripr/src/lsp/diagnostics.rs::seam_diagnostic_tests::configured_seam_severity_can_disable_a_class`
- `crates/ripr/tests/cli_smoke.rs::doctor_reports_missing_config_defaults`
- `crates/ripr/tests/cli_smoke.rs::doctor_exit_codes_distinguish_findings_usage_and_clean`
- `crates/ripr/src/cli/help_json.rs::tests::orchestrator_exit_contracts_match_the_implemented_mapping`
- `crates/ripr/src/cli/commands/doctor.rs::tests::doctor_human_projection_fails_for_missing_root`
- `crates/ripr/src/cli/commands/doctor.rs::tests::doctor_json_and_tool_failures_return_errors`
- `crates/ripr/tests/cli_smoke.rs::doctor_reports_loaded_config_path`
- `crates/ripr/tests/cli_smoke.rs::doctor_reports_malformed_config_error`
- `crates/ripr/src/output/doctor.rs::tests::old_workspace_compiler_is_advisory_for_analysis_and_fails_source_build`
- `crates/ripr/src/output/doctor.rs::tests::rust_root_with_missing_cargo_discloses_verification_limitation`
- `crates/ripr/src/output/doctor.rs::tests::source_build_missing_toolchain_names_install_fix_and_keeps_other_failures`
- `crates/ripr/src/output/doctor.rs::tests::doctor_wait_failure_stays_off_the_source_build_install_route`
- `crates/ripr/src/output/doctor.rs::tests::nested_workspace_under_git_root_passes_cargo_toml`
- `crates/ripr/src/output/doctor.rs::tests::nested_package_under_git_root_passes_cargo_toml`
- `crates/ripr/src/output/doctor.rs::tests::cargo_toml_inside_target_does_not_pass`
- `crates/ripr/src/output/doctor.rs::tests::cargo_toml_inside_venv_does_not_pass`
- `crates/ripr/src/output/doctor.rs::tests::cargo_toml_inside_fixtures_does_not_pass`
- `crates/ripr/src/output/doctor.rs::tests::cargo_toml_inside_ripr_dir_does_not_pass`
- `crates/ripr/src/output/doctor.rs::tests::sibling_nested_workspaces_pass_naming_one`
- `crates/ripr/src/output/doctor.rs::tests::ancestor_workspace_from_member_src_passes_cargo_toml`
- `crates/ripr/src/output/doctor.rs::tests::ancestor_package_from_src_passes_cargo_toml`
- `crates/ripr/src/output/doctor.rs::tests::cargo_toml_walk_does_not_cross_the_git_top_level`
- `crates/ripr/src/output/doctor.rs::tests::nested_cargo_toml_walk_does_not_follow_directory_symlinks`
- `crates/ripr/tests/cli_smoke.rs::doctor_git_root_with_nested_workspace_passes_cargo_toml`
- `crates/ripr/src/output/doctor.rs::tests::polyglot_git_root_with_typescript_and_nested_workspace_keeps_cargo_toml_required`
- `crates/ripr/src/output/doctor.rs::tests::nested_workspace_does_not_override_rust_disabled_in_languages`
- `crates/ripr/src/cli/commands/doctor.rs::tests::detect_languages_stays_shallow_when_nested_rust_lives_under_typescript_root`
- `crates/ripr/tests/cli_smoke.rs::doctor_polyglot_git_root_with_typescript_passes_nested_cargo_toml`
- `crates/ripr/src/output/doctor.rs::tests::doctor_cargo_probe_uses_selected_root`
- `crates/ripr/src/output/doctor.rs::tests::a_missing_root_skips_root_bound_probes_instead_of_blaming_the_tools`
- `crates/ripr/src/output/doctor.rs::tests::a_file_root_is_not_reported_as_missing`
- `crates/ripr/src/output/doctor.rs::tests::a_directory_named_like_a_file_still_passes_root_directory`
- `crates/ripr/src/output/doctor.rs::tests::doctor_root_path_classify_follows_symlinks_and_splits_file_from_missing`
- `crates/ripr/src/output/doctor.rs::tests::an_unreadable_root_is_not_reported_as_missing`
- `crates/ripr/src/output/doctor.rs::tests::a_symlink_into_an_unreadable_directory_is_not_reported_as_a_file`
- `crates/ripr/src/output/doctor.rs::tests::doctor_root_path_recovery_guidance_does_not_borrow_sibling_wording`
- `crates/ripr/src/cli/commands/doctor.rs::tests::doctor_core_report_fails_closed_for_file_root`
- `crates/ripr/src/cli/commands/doctor.rs::tests::doctor_human_projection_fails_for_file_root`
- `crates/ripr/tests/cli_smoke.rs::doctor_file_root_is_not_reported_as_missing`
- `crates/ripr/src/cli/commands/doctor.rs::tests::source_build_profile_keeps_enabled_language_runtime_failure_advisory`
- `crates/ripr/src/cli/commands/doctor.rs::tests::required_runtime_failure_gets_a_fix_line`
- `crates/ripr/src/cli/commands/doctor.rs::tests::cargo_toml_failure_suppresses_check`
- `crates/ripr/src/cli/commands/doctor.rs::tests::compact_discloses_ignored_perl_executable`
- `crates/ripr/src/cli/commands/doctor.rs::tests::perl_path_exporter_refused_only_inside_root_without_opt_in`
  pins canonical-inside refusal, the opt-in boundary, and outside-root allowance (#5349).
- `crates/ripr/src/cli/commands/doctor.rs::tests::perl_path_exporter_refused_follows_symlink_into_root`
  pins symlink-into-root refusal by canonicalization (#5349, Unix).
- `crates/ripr/src/cli/commands/doctor.rs::tests::perl_exporter_lines_report_refused_hit_with_next_step`
  pins the refused report wording and next step (#5349).
- `crates/ripr/tests/cli_smoke.rs::doctor_refuses_repo_local_perl_exporter_on_relative_path`
  proves a checkout-local exporter on a relative PATH is not executed (no marker) (#5349).
- `crates/ripr/tests/cli_smoke.rs::doctor_runs_repo_local_perl_exporter_with_opt_in`
  proves the opt-in executes the same exporter as before (#5349).
- `crates/ripr/tests/cli_smoke.rs::doctor_runs_perl_exporter_outside_root_via_relative_path`
  proves a relative PATH entry outside the root still probes (#5349).
- `crates/ripr/tests/cli_smoke.rs::doctor_refuses_perl_exporter_symlinked_into_root`
  proves a PATH symlink into the root is refused and not executed (#5349, Unix).
- `crates/ripr/src/cli/commands/doctor.rs::tests::compact_screen_discloses_refused_perl_exporter`
  pins the refused exporter on the default (non-verbose) screen (#5349).
- `crates/ripr/src/cli/commands/doctor.rs::tests::perl_path_exporter_refused_unresolvable_symlink_ancestor_into_root`
  pins fail-closed ancestor resolution for an unresolvable candidate (#5349, Unix).
- `crates/ripr/tests/cli_smoke.rs::doctor_json_refuses_repo_local_perl_exporter`
  proves the `--json` refusal (state refused) and non-execution end to end (#5349).
- `crates/ripr/tests/cli_smoke.rs::doctor_refuses_repo_local_perl_exporter_on_default_screen`
  proves the default human screen surfaces the refusal without `--verbose` (#5349).
- `crates/ripr/tests/cli_smoke.rs::doctor_refused_perl_exporter_takes_precedence_over_incompatible`
  proves a refusal is not masked by an incompatible hit (#5349).
- `crates/ripr/src/cli/commands/doctor.rs::tests::compact_newline_root_stays_one_physical_line_per_entry`
- `crates/ripr/src/cli/commands/doctor.rs::tests::verbose_invalid_config_keeps_parser_reasons_without_source`
- `crates/ripr/src/config.rs::tests::config_error_display_keeps_misplaced_mode_hint_without_source`
- `crates/ripr/src/config.rs::tests::config_error_display_keeps_unclosed_table_reason`
- `crates/ripr/tests/cli_smoke.rs::doctor_newline_bearing_root_still_fits_one_screen`
- `crates/ripr/src/cli/commands/doctor.rs::tests::source_build_compact_reports_enabled_runtime_as_advisory`
- `crates/ripr/src/cli/commands/doctor.rs::tests::compact_hides_optional_non_primary_runtime`
- `crates/ripr/src/cli/commands/doctor.rs::tests::path_command_in_ignores_a_repo_local_prove_cmd`
- `crates/ripr/src/cli/commands/doctor.rs::tests::path_command_in_prefers_path_prove_over_repo_local_prove_cmd`
- `crates/ripr/src/cli/commands/doctor.rs::tests::path_command_in_windows_prefers_pathext_over_extensionless`
- `crates/ripr/src/cli/commands/doctor.rs::tests::is_cwd_path_entry_treats_empty_dot_and_backslash_dot_as_cwd`
- `crates/ripr/src/cli/commands/doctor.rs::tests::path_command_in_unix_prove_cmd_is_not_a_prove_binary`
- `crates/ripr/src/cli/commands/doctor.rs::tests::doctor_program_keeps_explicit_paths_and_skips_bare_names_off_path`
- `crates/ripr/tests/cli_smoke.rs::doctor_does_not_treat_a_repo_local_prove_cmd_as_path_prove`
- `crates/ripr/tests/cli_smoke.rs::doctor_reports_a_real_path_prove_despite_a_repo_local_prove_cmd`
- `crates/ripr/tests/cli_smoke.rs::doctor_discloses_missing_verification_tools_but_still_checks_manifest`
- `xtask/tests/ripr_public_guide_contract.rs::doctor_exit_code_guide_distinguishes_analysis_and_source_build`

Planned tests:

- fixture-backed config examples once SARIF and CI policy consume the config
  surface;
- extension smoke coverage for editor settings flowing through LSP session
  config.

## Implementation Mapping

Current implementation:

- `crates/ripr/src/analysis/diff/path.rs::artifact_root_gitignore_entry` owns
  literal Gitignore rule encoding. Its consumers are
  `crates/ripr/src/cli/commands/doctor.rs::ripr_artifact_ignore` for the doctor
  warning and
  `crates/ripr/src/analysis/pipeline.rs::ripr_artifact_disclosure` for the
  changed-scope disclosure. Neither consumer uses that encoder for Git argv
  or shell arguments.

- `crates/ripr/src/config.rs` owns config parsing, defaults, validation,
  presence (`config_entry_present` / `config_present_at_root`), skipped-closer
  detection after an implicit analysis-root rebind
  (`closer_config_skipped_by_analysis_root`), and precedence helpers.
  Workspace status and Python repair config-profile detection consume that
  presence rule instead of `Path::is_file`.
- `crates/ripr/src/cli/commands.rs` loads repo config for `check`, `explain`,
  and `context`, and reports config status through `doctor`.
- `crates/ripr/src/output/doctor.rs` projects missing cargo/rustc as analysis
  advisories or source-build failures with an install route. The `cargo_toml`
  check locates a selected-root, git-bounded ancestor, or nested Cargo.toml
  without rebinding doctor's `--root`. `rust_toolchain_scope` consults that
  locator so a polyglot git root with a TypeScript marker does not skip the
  check (#7506); `detect_languages` stays a shallow root scan.
- `crates/ripr/src/cli/commands/doctor.rs` prints start-here / first-command
  guidance for the analysis profile only.
- `crates/ripr/src/app.rs` provides config-aware orchestration for CLI and LSP
  adapters.
- `crates/ripr/src/lsp/config.rs` merges repo config with LSP initialization
  options.
- `crates/ripr/src/output/human.rs`, `crates/ripr/src/output/human/`,
  `crates/ripr/src/output/json/report.rs`, and
  `crates/ripr/src/output/github.rs` apply configured finding severity.
- `crates/ripr/src/output/suppressions.rs` owns suppression declaration parsing
  and both loaders: the configured path for badge reports and the explicit
  check policy. Declaration-line tracking is local to each pending entry;
  both loaders fail closed on parser violations, with explicit-path renaming.
- `ripr.toml.example` documents the supported v1 shape.

## Metrics

- `repository_config`
