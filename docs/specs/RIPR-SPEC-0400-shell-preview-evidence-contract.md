# RIPR-SPEC-0400: Shell Preview Evidence Contract

Status: proposed

Owner: language-adapter / shell

Created: 2026-11-02

Linked issues:

- [#5723](https://github.com/EffortlessMetrics/ripr-swarm/issues/5723) — normative shell preview contract (this document)
- [#5601](https://github.com/EffortlessMetrics/ripr-swarm/issues/5601) — shell language epic
- [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582) — ecosystem language-expansion programme

Shared architecture (consumed by reference, not redefined here):

- [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526) — multi-language architecture
- [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531) — language capability matrix
- [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532) — static witness vocabulary
- [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541) — seam and probe contracts
- [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542) — test activation and lifecycle
- [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546) — oracle strength model
- [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559) — cross-language edge registry
- [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560) — corpus evaluation framework

Related boundaries:

- [#5600](https://github.com/EffortlessMetrics/ripr-swarm/issues/5600) owns PowerShell source analysis; POSIX/Bash shell semantics in this spec must not reuse PowerShell semantics.
- Product shell-command rendering and translation (verify/receipt Bash/PowerShell display lines) is a separate presentation capability and cannot establish shell-language source-analysis support.

Linked specs:

- [RIPR-SPEC-0026: Language adapter contract](RIPR-SPEC-0026-language-adapter-contract.md)
- [RIPR-SPEC-0378: Cross-language edge registry v1](RIPR-SPEC-0378-cross-language-edge-registry-v1.md)

Support-tier impact:

- This spec does not promote shell, Bats, or any dialect cell. Shell evidence remains
  `language_status = preview` and `authority_boundary = preview_advisory_only` until a
  separate support-tier PR cites adjudicated corpora. See
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/`, `docs/spec-index.d/`, and
  `.ripr/traceability.d/`.
- Register spec scope in each verdict corpus under `fixtures/*/spec-scopes/`.
- No adapter, parser, routing, output, gate, badge, baseline, RIPR Zero, release, or
  publication authority is introduced by this spec alone.

## Problem

Shell behavior is expressed through dialect-specific expansion, exit status, stdout/stderr,
pipelines, environment mutation, file/process effects, and external commands—not through
ordinary typed return values. A POSIX/Bash/Bats preview adapter cannot reuse PowerShell
semantics, TypeScript call graphs, or Python import graphs without inventing false reach.

Without a normative contract, early shell work can:

- infer Bash from a `.sh` suffix or POSIX from Bash parsing;
- treat pipeline text as proof of execution or `$output` as proof of `$status`;
- credit Bats hooks/helpers without selected active test execution;
- treat PATH stubs, function mocks, or bats-mock wrappers as real external-command reach;
- promote preview facts to gates, badges, or support tiers by accident.

This spec defines admissible static claims and refusals for the first shell preview slice:
POSIX `sh`, Bash, and Bats. Implementation, parsers, corpora, and output projection follow
in epic #5601 (#5724–#5734).

## Behavior

### Authority and wire language

The wire language identifier is `shell`. Every emitted finding uses
`language = "shell"` and `language_status = "preview"`. Dialect, framework,
platform, invocation mode, and effect-family cells below are independent capability
dimensions; no cell inherits support from another.

Initial posture:

```text
authority_boundary = preview_advisory_only
execution_during_analysis = forbidden
default_enablement = forbidden
gate/badge/baseline/RIPR Zero/release/publication = forbidden
```

Product command rendering (`bash`/`powershell` verify lines, human paste helpers) does
not count as shell source-analysis capability (#5601, #5723).

### Capability cell registry

Each row is a distinct cell. Unsupported cells remain explicitly unsupported or limited;
they do not inherit POSIX, Bash, or Bats support.

| Cell id | Family | Meaning |
| --- | --- | --- |
| `shell/posix/sh/source` | dialect | POSIX `sh` script/source analysis under declared POSIX mode |
| `shell/bash/source` | dialect | Bash script/source analysis under declared Bash version/features |
| `shell/bats/test` | framework | Bats test file registration, hooks, `run`, and capture semantics |
| `shell/shebang/routed` | identity | Positive shebang or configured interpreter evidence routes dialect |
| `shell/path/suffix/configured` | identity | `.sh`/`.bash` routed only under explicit dialect configuration |
| `shell/extensionless/executable` | identity | Extensionless executable script with positive shebang evidence |
| `shell/sourced/library` | identity | Sourced file/library under caller dialect/options context |
| `shell/expansion/literal` | expansion | Fully literal words after quoted/static analysis |
| `shell/expansion/bounded` | expansion | Finite alternative sets with provenance |
| `shell/expansion/dynamic` | expansion | Explicit dynamic/unresolved expansion (limitation, not clean gap) |
| `shell/pipeline/stage_status` | process | Per-stage exit status facts |
| `shell/pipeline/aggregate_status` | process | Pipeline aggregate status under declared `pipefail`/dialect |
| `shell/stream/stdout` | process | Stdout capture/redirection subjects |
| `shell/stream/stderr` | process | Stderr capture/redirection subjects |
| `shell/stream/merged` | process | Merged redirect (`2>&1`, `&>`) subjects |
| `shell/process/subshell` | process | Subshell/group isolation context |
| `shell/process/background_job` | process | Background job and `wait` status context |
| `shell/process/signal_trap` | process | Trap and signal-handling context |
| `shell/external/command` | command | External command invocation semantics (static bounds only) |
| `shell/env/declared_context` | environment | Declared options, IFS, locale, PATH policy in subject |
| `shell/effect/owned_fixture_fs` | effect | Filesystem intentions on owned fixture paths |
| `shell/effect/owned_fixture_env` | effect | Environment mutations on owned fixture subjects |
| `shell/effect/owned_fixture_process` | effect | Process intentions on owned fixture subjects |
| `shell/effect/external_integration` | effect | Explicit integration/manual boundary outside owned fixtures |
| `shell/bats/run_capture` | bats | `run` command with captured `status`/`output`/`lines` |
| `shell/bats/assert/support` | bats | bats-support/bats-assert/bats-file mapped by exact identity |
| `shell/bats/mock_stub` | bats | Function/PATH/bats-mock stub semantics (refutes real execution) |
| `shell/platform/context` | platform | Declared OS/tool/runtime prerequisites |
| `shell/unsupported/zsh` | limitation | zsh-specific extensions unsupported unless future cell |
| `shell/unsupported/ksh` | limitation | ksh-specific extensions unsupported unless future cell |
| `shell/unsupported/fish` | limitation | fish/Nushell unsupported unless future cell |
| `shell/unsupported/ci_dsl` | limitation | CI workflow expressions, Make, Dockerfile `RUN`, package scripts unsupported unless future cell |

Future adapters register these cells in the runtime capability registry only after producer
proof; this spec defines the vocabulary and join keys (#5531, RIPR-SPEC-0365).

### Dialect and project identity

Dialect resolution is evidence-first:

1. A recognized shebang or configured interpreter line selects POSIX `sh` or Bash when the
   interpreter token and admitted version features are explicit.
2. `.bats` files are Bats test subjects executed under Bash dialect for test semantics; they
   are not generic `.sh` files.
3. `.sh` defaults to POSIX `sh` unless repository configuration or an explicit project cell
   selects Bash for that path/root.
4. `.bash` selects Bash only when the path is admitted; it does not imply Bats.
5. Extensionless executable scripts require positive shebang or project evidence; path
   proximity alone is insufficient.

Negative rules:

- POSIX support cannot be inferred from Bash-only syntax (`[[ ]]`, arrays, `pipefail`, brace
  expansion where Bash-specific).
- Bash support cannot be inferred from a `.sh` suffix alone.
- zsh, ksh, dash-only extensions, BusyBox ash, fish, Nushell, CI DSL, Make recipes,
  Dockerfile `RUN`, npm/yarn script bodies, and embedded snippets in other languages require
  their own cells or remain unsupported.

Every routed subject carries:

- `dialect_cell` (one of the admitted dialect cells or `unknown`);
- `invocation_mode` (`script`, `sourced`, `bats_test`, `helper_library`, `unknown`);
- repository-relative script identity (`repo:path` plus stable internal id);
- exact source spans for dialect evidence (shebang line, config anchor, or limitation stop).

### Owners and probes

Owners (repository-relative, span-backed):

- scripts and sourced libraries;
- shell functions under declared dialect/options/context;
- simple commands, command lists, pipelines, compound commands, groups/subshells,
  conditionals, loops, case arms, traps, coprocesses, background jobs, and async callbacks
  at a granularity compatible with #5532 witness preservation;
- sourced-file and helper entry identities;
- Bats tests, hooks, and helper entry points under #5542 activation rules.

Probes (exact-span, typed):

- predicates and branch conditions;
- exit statuses (`exit`, `return`, last-command status contexts);
- stdout/stderr/redirection targets;
- pipeline stages and aggregate status subjects;
- files/directories/environment/shell variables/cwd/process/signal/job boundaries;
- command invocations and resource-effect intentions on owned fixtures;
- Bats `$status`, `$output`, `$lines`, and stderr-capable captures by admitted version.

Binding law:

- Simple command names, aliases, functions from an ambient shell, PATH lookup, filename
  proximity, or display strings do not establish command/function/script binding.
- A pipeline command's textual presence does not prove execution or success.

### Expansion and parsing contract

Expansion stages remain separately typed with provenance:

```text
lexical token / quoted or unquoted word
parameter expansion
command substitution
arithmetic expansion
process substitution (Bash cell only)
tilde expansion
brace expansion (Bash cell only)
pathname expansion / globbing
word splitting (IFS)
quote removal
here-doc / here-string
redirection
array / associative / nameref (Bash cell only)
```

Each stage emits one of:

- `literal` — fully known bytes at analysis time;
- `bounded` — finite alternative set with provenance to source spans;
- `dynamic` — unresolved without runtime/eval/indirection;
- `unsupported_in_dialect` — construct outside admitted dialect cell.

Rules:

- Parse-time versus run-time behavior stays explicit.
- Shell options and environment that alter expansion (`set`, `shopt`, `IFS`, locale,
  `nocaseglob`, `globstar`, `extglob`, etc.) attach to the declared context cell, not host
  defaults.
- Do not reconstruct argv from display strings or assume one shell's expansion rules for
  another.
- `eval`, indirect expansion (`${!name}`), namerefs, dynamic `source`, generated code,
  aliases, and unquoted dynamic expansions emit typed limitations unless bounded facts prove
  otherwise.

### Process and pipeline contract

These subjects stay distinct:

```text
simple_command_exit_status
pipeline_stage_exit_status
pipeline_aggregate_exit_status
pipefail_state
stdout
stderr
redirected_or_merged_streams
subshell_or_group_context
background_job_and_wait_status
signal_or_termination_state
command_substitution_output_and_status
function_or_script_return_or_exit
current_process_vs_child_process
external_command_semantics
```

Pipeline law:

- Last-stage status is not universal: POSIX default vs `set -o pipefail` vs errexit/negation
  contexts differ and must be modeled separately.
- Captured stdout text does not prove exit status; captured status does not prove stderr
  content.
- `set -e`/errexit, traps, `!`, `&&`/`||`, conditional contexts, subshells, functions,
  command substitutions, and asynchronous jobs retain exact context metadata; they are not
  collapsed into a single “exit code” field.

### Shell-option and environment contract

Declared/selected context may include:

- POSIX mode vs Bash mode;
- `errexit`, `nounset`, `pipefail`, `xtrace`, `noclobber`;
- globbing options (`nullglob`, `failglob`, `globstar`, `extglob`);
- `IFS`, locale, encoding;
- `PATH` and command lookup policy as declared data, not host state;
- working directory and source/include roots;
- inherited vs script-set environment;
- traps and signal disposition declarations.

Non-canonical unless explicitly selected:

- host defaults, interactive startup files, user rc/profile;
- ambient aliases/functions;
- current directory, live PATH, live environment, locale, umask;
- open file descriptors, jobs table, and process state.

### Bats activation contract (#5542)

Map admitted Bats versions/syntax into shared activation vocabulary:

| Subject | Contract |
| --- | --- |
| `@test` and admitted alternative forms | Register tests with exact selected identity |
| `setup`/`teardown`, `setup_file`/`teardown_file` | Lifecycle scope distinct from production reach |
| tags/filters/skip | Selected vs skipped vs unknown remain distinct |
| helper loading / `bats_load_library` | Helper-bound vs active test execution distinct |
| `run` and captures | `status`, `output`, `lines`, stderr-capable captures by version |
| literal vs generated tests/helpers | Known static registration vs dynamic limitation |
| platform/dialect/interpreter prerequisites | Explicit prerequisite cells or limitations |
| mocks/stubs/wrappers | Mapped only through stub semantics; refute real external execution |

States that must not collapse:

```text
registered
selected
active
skipped
helper_bound
generated
platform_limited
dialect_limited
unknown
```

Hook or helper presence does not prove the changed command/function under test executes.
Test activation precedes oracle eligibility (#5542, #5546).

### Oracle and system-effect contract (#5546)

Bounded oracle subjects:

- shell/Bats status assertions;
- stdout/stderr line/substring/regex/cardinality/order assertions on captured subjects;
- owned-fixture filesystem/directory/symlink/permission/content intentions;
- owned-fixture environment/cwd/process/signal intentions;
- external/native command argv/cwd/environment/stream/status/timeout subjects as separate
  static facts, not runtime outcomes;
- repository-owned function/script/source/helper/pipeline relations before actionability;
- bats-support, bats-assert, bats-file (or equivalent) through exact package/function identity;
- mocks/stubs/wrappers: interaction observation vs real external execution remain distinct;
- custom helper/assertion libraries as explicit cells or limitations.

Static syntax does not prove:

- a file/process/network/service/system effect succeeded;
- a command exists on PATH at runtime;
- a remote resource responds;
- a test runs on a CI worker.

Effects outside owned fixtures remain integration/manual/runtime boundaries. Cross-language
edges require #5559 evidence; unknown reach or effect is not `no_static_path`.

### Relation and limitation vocabulary

Typed stops (non-exhaustive; producers map to #5532/`static_limit_kind` extensions):

- unresolved dialect/shebang/invocation mode/options;
- quoting/word-splitting/globbing/expansion/IFS/locale/encoding unresolved;
- `eval`, indirect/nameref, dynamic source/load, generated code;
- alias/function/PATH lookup/command-hashing ambiguity;
- source/include path and cwd ambiguity;
- pipeline/pipefail/errexit/trap/subshell/group/background/wait/signal context unresolved;
- command/function/script binding unavailable;
- external command semantics unavailable;
- filesystem/environment/process/service/network/remote/system state unavailable;
- Bats dynamic/helper/library/mock/custom assertion semantics unavailable;
- platform/tool/version/prerequisite uncertainty;
- cross-language/native/resource edges without registered #5559 edge.

A command name, filename, test description, output string, or source proximity does not
establish binding.

### Shared architecture consumption

This spec does not redefine #5526–#5560. Producers and consumers must:

- emit witnesses compatible with #5532;
- place probes under #5541 seam rules;
- activate tests under #5542 before oracle credit;
- grade oracle strength under #5546;
- join capability claims to registry cells under #5531/#5532 and RIPR-SPEC-0365;
- register cross-language edges only through #5559 / RIPR-SPEC-0378;
- adjudicate support only through #5560 corpora (#5734), not this spec alone.

## Required Evidence

This spec slice is documentation-only. Proof for the contract itself:

- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-doc-artifacts`
- `cargo xtask check-doc-index`
- `cargo xtask check-traceability`

Future implementation slices (#5724–#5734) owe executable evidence mapped in
`.ripr/traceability.d/` and #5560 corpora; none of that is claimed here.

## Known Limits

- No parser, adapter, or Bats runtime is specified at the AST/token algorithm level; #5724
  selects parser boundaries.
- Capability cells are normative vocabulary only until a producer registers them in the
  runtime registry without inventing gate authority.
- Host shell state, external command behavior, and CI worker environments remain outside
  passive analysis.

## Non-Goals

- No shell, Bats, external-command, network, filesystem, service, remote, or system-effect
  execution during analysis.
- No adapter/scaffold/routing/output/Rust code in this spec slice.
- No default enablement, support-tier promotion, release, or publication.
- No zsh/ksh/fish/Nushell/CI-DSL/Make/Docker/package-script support inferred from POSIX/Bash.
- No generated tests or source edits.

## Acceptance Examples

Positive (admissible static claims when future producers implement this contract):

1. A `scripts/deploy.sh` file with `#!/usr/bin/env bash` and `set -o pipefail` registers
   `shell/bash/source`, records `pipefail_state = enabled` on an exact span, and models a
   pipeline's stage statuses separately from aggregate status.
2. A POSIX `#!/bin/sh` script using only `[` tests and literal quotes registers
   `shell/posix/sh/source`; a nested Bash `[[` test in the same file without Bash evidence
   emits `unsupported_in_dialect`, not POSIX credit.
3. A `.bats` `@test` block with `run ./bin/tool` registers the test as `selected`/`active`
   only when filters/tags permit; `$output` assertions map to stdout oracle subjects without
   crediting `$status` unless a status assertion exists.
4. A test that defines a function named `curl` before `run curl …` maps stub interaction and
   refutes real external `curl` execution (#5546 mock law).
5. An owned-fixture assertion `[ -f "$BATS_TEST_TMPDIR/out" ]` may state a filesystem
   intention on an owned path; the same pattern against `/etc/passwd` emits
   `shell/effect/external_integration` limitation.

Negative (required refusals):

1. `.sh` suffix without shebang or config → POSIX dialect is not assumed; emit unresolved
   dialect limitation, not Bash parsing.
2. `echo $(cmd)` stdout capture in a test → `$output` oracle does not prove `cmd`'s exit
   status without a separate status assertion.
3. Pipeline `a | b | c` with default POSIX aggregate rule → stage failures hidden from
   aggregate status unless `pipefail` cell is positively enabled in subject context.
4. `source ../lib/helper.sh` with unresolved caller cwd → no reach to helpers inside the
   sourced file without bounded path facts.
5. `@test` with only `setup` calling the production script → hook execution does not activate
   oracle credit for commands inside the test body unless the test is selected/active.
6. Product `verify_command` Bash line for `ripr check` → does not register
   `shell/bash/source` capability.
7. Unresolved `${!ref}` → typed dynamic expansion limitation, not `no_static_path`.
8. Command name `git` in script text → no binding to `/usr/bin/git` or repository hooks
   without repository-owned relation evidence.

## Test Mapping

Planned tests:

- `shell_posix_dialect_requires_explicit_evidence_not_sh_suffix`
- `shell_bash_brace_expansion_unsupported_under_posix_cell`
- `shell_pipeline_stage_and_aggregate_status_remain_distinct`
- `shell_pipefail_changes_aggregate_without_collapsing_stages`
- `shell_output_oracle_does_not_imply_status_oracle`
- `shell_bats_run_active_test_required_before_oracle_credit`
- `shell_bats_setup_hook_does_not_prove_test_body_command_reach`
- `shell_function_stub_refutes_external_command_execution`
- `shell_unresolved_eval_emits_typed_limitation_not_no_static_path`
- `shell_product_verify_bash_line_does_not_promote_source_analysis`

Planned corpora (#5734, #5560): POSIX, Bash, Bats core, expansion, pipeline/process,
external-command, local-effect, external-effect, legacy, mixed-language, and embedded-context
judged rows remain separate.

## Implementation Mapping

Planned producer/consumer surfaces (epic #5601):

- `#5724` — ADR/parser boundary
- `#5725`–`#5727` — shell analysis adapter scaffold, roots, owners/probes
- `#5728`–`#5730` — Bats activation and oracle/library mapping
- `#5731`–`#5732` — evidence relations and repository completeness
- `#5733` — shared output projection
- `#5734` — judged corpora and support adjudication

No implementation is claimed in this spec slice.

## Metrics

- `spec_format_compliance`
- `capability_cell_registry_completeness`
- `planned_test_obligation_registry`
