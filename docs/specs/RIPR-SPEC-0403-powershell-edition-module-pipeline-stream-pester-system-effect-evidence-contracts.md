# RIPR-SPEC-0403: PowerShell edition, module, pipeline, stream, Pester, and system-effect evidence contracts

Status: proposed

Owner: language-adapter / ecosystem (#5600)

Created: 2026-11-02

Linked issues:

- [#5710](https://github.com/EffortlessMetrics/ripr-swarm/issues/5710) (this contract)
- Parent epic [#5600](https://github.com/EffortlessMetrics/ripr-swarm/issues/5600)
- Programme [#5582](https://github.com/EffortlessMetrics/ripr-swarm/issues/5582)
- Shared architecture [#5526](https://github.com/EffortlessMetrics/ripr-swarm/issues/5526)
- Consumed authorities: [#5531](https://github.com/EffortlessMetrics/ripr-swarm/issues/5531), [#5532](https://github.com/EffortlessMetrics/ripr-swarm/issues/5532), [#5541](https://github.com/EffortlessMetrics/ripr-swarm/issues/5541), [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542), [#5546](https://github.com/EffortlessMetrics/ripr-swarm/issues/5546), [#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559), [#5560](https://github.com/EffortlessMetrics/ripr-swarm/issues/5560)
- Presentation-only boundary: Bash/PowerShell **command-rendering** work does not own PowerShell source-language semantics

Linked specs:

- [RIPR-SPEC-0026](RIPR-SPEC-0026-language-adapter-contract.md) (language-neutral adapter boundary)
- [RIPR-SPEC-0030](RIPR-SPEC-0030-preview-evidence-policy-boundary.md) (preview posture)
- [RIPR-SPEC-0378](RIPR-SPEC-0378-cross-language-edge-registry-v1.md) (registered cross-language edges)

Support-tier impact:

- This spec does not enable PowerShell analysis, promote support tiers, arm default
  gates, badges, baselines, RIPR Zero blocking, PowerShell Gallery publication, or
  product release. All PowerShell evidence governed here remains preview/advisory per
  edition, platform, Pester version, mode, stream family, and effect family until
  separate promotion work cites proof. The canonical support-tier boundary remains
  [support tiers](../status/SUPPORT_TIERS.md).
- Human-facing PowerShell **command quoting** and paste helpers (for example
  RIPR-SPEC-0025, RIPR-SPEC-0122) MUST NOT be cited as PowerShell source-analysis
  support, adapter registration, or oracle eligibility.

Policy impact:

- Register in `policy/doc-artifacts.d/RIPR-SPEC-0403.toml`.
- Add `docs/spec-index.d/RIPR-SPEC-0403.md`.
- Register `.ripr/traceability.toml` behavior `RIPR-SPEC-0403`.
- Register `fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0403.toml` as out of
  scope for runtime mutant corpora until PowerShell judged corpora land.
- No adapter, parser, routing, feature gate, or `LanguageId` registration is
  introduced by this document alone.

## Problem

PowerShell combines edition-specific semantics, module/session/scope state, rich
pipeline object flow, eight distinguishable output channels, native process
edges, Pester's discovery/run split, and unbounded external system effects. Without
a normative contract, passive analysis could treat `.ToString()` as the object,
merge error and success streams, credit mocks as changed-owner reach, or promote
command-rendering portability into source-language support.

Epic #5600 needs one preview **project/edition context** law, one **language
evidence** law, and one **Pester test/oracle** law before parsers, adapters, or
execution-backed proof ship.

## Behavior

### Wire language and routing

The closed wire language set for this spec is exactly:

```text
powershell
```

File routing maps semantic adapters by extension and admitted role **after**
excluding pure data/manifest `.psd1` paths unless explicitly admitted as
executable script content:

| Extension / shape | Wire language | Initial role |
| --- | --- | --- |
| `.ps1` | `powershell` | script |
| `.psm1` | `powershell` | module |
| `.psd1` | `powershell` | manifest_or_data (not executable unless admitted) |
| class/enumeration bodies in admitted script/module | `powershell` | type_member |
| configuration/DSC forms when statically admitted | `powershell` | configuration |

A file extension or host installation alone MUST NOT establish edition, platform,
or compatibility mode. Host prerequisites (for example `pwsh` on PATH versus
Windows PowerShell 5.1) are **context facts** with explicit `currentness`, not
reach or oracle defaults.

Capability cells distinguish at minimum:

- PowerShell 7+ cross-platform (`pwsh`);
- Windows PowerShell 5.1 where separately evidenced;
- repository-selected analysis root and portable module/script identity
  (repo-relative path + module name/version when declared).

### Three machine-readable contracts

This spec defines three separable JSON-shaped contracts. Implementations may
store them in one envelope, but validation and promotion MUST keep the slices
independent:

| Contract | `schema_version` | Owns |
| --- | --- | --- |
| PowerShell project/edition context | `powershell_project_context.v1` | Edition, platform, module roots, manifest/data roles, host prerequisites, selected root |
| PowerShell language evidence | `powershell_language_evidence.v1` | Owners, probes, pipeline/stream typing, limitations, scope/session context |
| PowerShell test/oracle evidence | `powershell_test_oracle_evidence.v1` | Pester registration, activation, `Should` and assertion families |

Each document requires `validate()` after deserialization, uses
`deny_unknown_fields`, and records explicit `currentness` for every fact that
can go stale (module autoload snapshot, session state, `$PSVersionTable`, imported
module set, Pester version pin).

Context facts MUST NOT appear inside language semantic documents except by
reference (`context_ref` id + digest). Language documents MUST NOT claim reach,
exposure, or oracle strength from context fields alone.

#### Project/edition context (`powershell_project_context.v1`)

Portable identities and currentness cover, without executing scripts or modules:

- **Editions**: `pwsh` (7+), `windows_powershell` (5.1), or `unknown` per
  declared `#requires`, `$PSVersionTable` literals in checked-in bootstrap, or
  static config when present.
- **Platforms**: Windows, Linux, macOS, or `unknown`; compatibility mode flags
  when declared.
- **Module and script roots**: repo-relative paths for scripts, modules,
  manifests, nested module graphs, and `ModulesToProcess` when statically listed.
- **Manifest/data `.psd1`**: role `manifest`, `module_data`, or `executable_script`
  only when admission rules pass; default is data/manifest under a bounded parser.
- **Repository-selected root**: exact root chosen for analysis with
  `selection_basis` (`explicit_config`, `single_script_root`, `module_nearest`,
  `ambiguous`).
- **Application vs module vs script repository roles** when declared in manifest
  or project layout metadata.

**Context non-reach law:** Module name, manifest entry, `$PSScriptRoot`, gallery
install path, or co-located `.ps1`/`.psm1` MUST NOT alone establish production
owner identity, test-to-owner reach, or oracle eligibility.

#### Language semantic contract (`powershell_language_evidence.v1`)

Stable owner identities (exact-span probes mandatory):

- Scripts and modules; functions, filters, and advanced/cmdlet-like functions;
  workflows and configurations only when explicitly admitted; classes, enums,
  methods, properties, and constructors.
- Script blocks; `begin`/`process`/`end`/`clean`/`dynamicparam` blocks; traps;
  event, job, and remoting callbacks; closures only where stable
  enclosing/assignment/registration identity exists.
- Parameters, parameter sets, validation/transform attributes, aliases, exported
  members, and nested modules as declaration/context facts.

Exact-span probes bind to changed behavior for:

- conditions, returns, and output;
- terminating and non-terminating errors;
- stream emissions (see below);
- exit codes and process termination signals;
- object/collection construction;
- state, environment, filesystem, registry, service, and process mutation;
- native command invocation and pipeline effects;
- boundary values and preference-variable-sensitive branches when statically known.

#### Pipeline and stream contract (normative separation)

The following channels MUST remain separately typed in evidence. Collapsing them
into one “output” field is forbidden:

```text
success_output        # object / success pipeline stream
error_stream          # error records and terminating exceptions
warning_stream
verbose_stream
debug_stream
information_stream
progress_stream
host_ui               # host/UI rendering, not success objects
native_stdout
native_stderr
native_exit_code      # process termination / $LASTEXITCODE context
return_value          # function return vs emitted pipeline objects
system_effect         # filesystem/registry/environment/process/service/network/remoting
```

Pipeline composition, enumeration/unrolling, formatting, serialization,
redirection, stream merging, preference variables, `-ErrorAction`, `$LASTEXITCODE`,
`$?`, automatic variables, and native argument passing affect evidence typing and
MUST be modeled explicitly when statically known.

**Stream non-collapse law:**

- A string representation is not the object; `Out-String` or formatting layers
  MUST NOT substitute for success-object probes.
- Host output is not success output.
- A successful pipeline stage does not imply native process exit code or a
  downstream stage result.
- Return value versus emitted pipeline objects remain distinct fields.

#### Pester activation contract (`powershell_test_oracle_evidence.v1`)

Pester syntax maps into [#5542](https://github.com/EffortlessMetrics/ripr-swarm/issues/5542)
with explicit phase separation:

| Phase / field | Rule |
| --- | --- |
| `discovery` vs `run` | Discovered is not active; discovery-time code and run-time code are separate spans |
| Describe/Context/It | Nested block identities; each It carries its own activation state |
| BeforeDiscovery/BeforeAll/BeforeEach/AfterEach/AfterAll | Setup hooks are not production owners; hook declaration is not changed-owner reach |
| Data-driven / container-generated cases | Literal versus dynamic case keys; dynamic cases keep `activation = deferred` until static |
| Tags/filters/skip/pending/focus/explicit selection | Selection state gates oracle eligibility |
| Platform/edition/version/module availability | Explicit limit fields when prerequisites fail |
| TestDrive / in-module scope | Scope identity on mocks and assertions |
| Mocks / parameter filters / verifiable expectations | Mock declaration does not establish reach; call verification requires activation |
| Custom plugins/extensions/assertions | Explicit capability/limit rows only |

**Activation precedes oracle eligibility:** Oracle credit is impossible when
activation state is `disabled`, `skipped`, `pending`, `unselected`, `focused_out`,
`unknown`, or `deferred`.

Admitted Pester major versions (initial scope rows): Pester 5.x syntax; Pester 4.x
only when a separate capability cell is registered and evidenced. Custom
assertion plugins map only through separately declared semantics.

#### Oracle and system-effect contract

Bounded `Should` and assertion facts include, where statically established:

- equality, identity, type, null, boolean, containment, match, count, order;
- file/content/exist/throw/error/output/status and versioned operator families;
- array/collection versus scalar/string semantics;
- scriptblock execution and thrown/terminating/nonterminating behavior;
- stream-specific and native-process observations tied to the typed channels above;
- filesystem, registry, environment, process, service, module, and session state
  only when exact subject, before/after or expected state, and failure sink are
  established;
- mocks, spies, call counts, parameter filters, and scope/module/session identity;
- custom assertions only through separately mapped semantics.

Effects on remote machines, providers, COM/.NET/native resources,
credentials/secrets, network, clocks, services, registry, filesystems, and ambient
session state remain **bounded facts and limits**—not static proof of runtime
outcomes.

### Relation and limitation vocabulary

Map or add typed stops (non-exhaustive; extend only via spec amendment):

| Limitation kind | When |
| --- | --- |
| `ps_edition_or_platform_unknown` | Edition, `$PSVersionTable`, or platform branch not evidenced |
| `ps_module_or_scope_unresolved` | Auto-loading, nested modules, or scope/session identity unknown |
| `ps_alias_or_proxy_unresolved` | Alias, proxy function, or command precedence not fixed |
| `ps_dynamic_invocation_unresolved` | `&`, `.`, splatting, `Invoke-Expression`, dynamic parameters |
| `ps_scriptblock_binding_unknown` | Closure/binding/session state unclear |
| `ps_provider_or_path_unresolved` | Provider, drive, wildcard, or current location unknown |
| `ps_remoting_or_job_unresolved` | Remoting, jobs, runspaces, events, background work |
| `ps_native_process_unresolved` | Native argument passing, shell wrapper, or exit status unclear |
| `ps_dotnet_or_com_edge_missing` | .NET/COM/reflection/dynamic members without #5559 edge |
| `ps_pester_phase_or_activation_unknown` | Discovery/run conflation or unknown activation |
| `ps_mock_not_evidence_of_reach` | Mock/spy declaration without call-path evidence |
| `ps_system_effect_unavailable` | External effect subject unsafe or unavailable for static facts |
| `ps_stream_collapsed` | Implementation attempted to merge distinct stream channels |

Unknown reach or effect is not `no_static_path`. A command name, alias, string,
path, mock declaration, or same file does not establish binding.

Cross-language production/test relations require a registered edge under
RIPR-SPEC-0378 ([#5559](https://github.com/EffortlessMetrics/ripr-swarm/issues/5559)).
Co-location with C#, Python, or other languages without an edge remains unrelated.

### Authority boundary

Initial PowerShell support is preview/advisory per edition/platform/Pester
version/mode/stream/effect family. No default feature, gate, badge, baseline,
RIPR Zero, PowerShell Gallery, release, or publication authority from this spec
alone.

### Capability cell registry (normative preview index)

Capability cells use identity shape
`{language}/{framework}/{mode}/{family}` per
[RIPR-SPEC-0365](RIPR-SPEC-0365-capability-manifest-cell-join.md). The table
registers every initial PowerShell cell for docs/traceability/manifest joins;
runtime registry entries land in later implementation slices.

| Cell id | Support state (initial) | Tier ceiling |
| --- | --- | --- |
| `powershell/pester5/diff/static_facts` | `not_proven` | `preview` |
| `powershell/pester5/discovery/static_facts` | `not_proven` | `preview` |
| `powershell/pester5/run/static_facts` | `not_proven` | `preview` |
| `powershell/pester4/diff/static_facts` | `not_proven` | `preview` |
| `powershell/any/repo/static_facts` | `not_proven` | `preview` |
| `powershell/any/diff/repair_routing` | `not_proven` | `preview` |
| `powershell/pwsh7/diff/static_facts` | `not_proven` | `preview` |
| `powershell/winps51/diff/static_facts` | `not_proven` | `preview` |
| `powershell/any/diff/stream_typing` | `not_proven` | `preview` |
| `powershell/any/diff/native_process` | `not_proven` | `preview` |
| `powershell/any/diff/system_effect` | `not_proven` | `preview` |

Product manifest claims for these ids are added only when
`metrics/capabilities.toml` entries explicitly cite implementation proof; this
spec registers the ids for index completeness only.

## Required Evidence

- `cargo xtask check-spec-format`, `check-spec-numbering`, `check-doc-artifacts`,
  and `check-traceability` pass with this spec and its index registrations.
- Acceptance examples in this document are keyed for future PowerShell judged
  corpora as `RIPR-SPEC-0403#<slug>`.
- Implementation slices must add executable proof; this document alone does not
  satisfy runtime, adapter, or execution evidence.

## Non-Goals

- No PowerShell, Pester, module, native command, remoting, provider, or
  system-effect execution during passive analysis.
- No adapter code, routing, feature gates, default enablement, or support-tier
  promotion.
- No generated tests, source edits, or parser dependencies introduced by this
  document.
- No claim that command-rendering support implies PowerShell source analysis.

## Acceptance Examples

1. **context-pwsh-vs-winps-positive**: A script with `#requires -PowershellVersion
   7.0` yields `edition = pwsh` in context; probes on edition-sensitive syntax
   carry `context_ref` and refuse WinPS-only credit without a separate cell.
2. **context-psd1-data-negative**: A `.psd1` module manifest lists `RootModule`
   but is parsed as `manifest_or_data`; lines in the data section MUST NOT spawn
   executable owner probes unless admitted as script content.
3. **module-scope-negative**: Two functions share a name in different modules;
   reach MUST NOT credit a caller without module-qualified identity and activation.
4. **pipeline-object-vs-string-negative**: A changed `Write-Output $obj` probe MUST
   NOT collapse to the string from `$obj.ToString()` or formatting cmdlets.
5. **stream-error-vs-success-negative**: `Write-Error` and `Write-Output` in the
   same function emit separate typed channels; merging names `ps_stream_collapsed`.
6. **stream-native-exit-negative**: A successful pipeline stage followed by a native
   command MUST NOT imply `$LASTEXITCODE = 0` without native channel evidence.
7. **return-vs-pipeline-negative**: `return` and pipeline emission in an advanced
   function keep distinct probe kinds; one MUST NOT satisfy oracle for the other.
8. **pester-discovery-vs-run-negative**: Code in `BeforeDiscovery` MUST NOT receive
   run-phase oracle credit; discovery-time registration stays `activation = discovered`.
9. **pester-hook-not-reach-negative**: A `Mock` of `Get-ChildItem` MUST NOT mark the
   production function as changed-owner reachable without call-path evidence.
10. **pester-dynamic-case-deferred**: Parameterized or generated It blocks without
    static case keys keep `activation = deferred`; no oracle credit from registration.
11. **pester-mock-verifiable-positive**: When activation is `run` and a verifiable
    mock records an expected call, oracle facts cite mock scope and parameter filter
    spans separately from production owners.
12. **should-stream-oracle-positive**: `Should -Throw` and stream-specific assertions
    cite exact assertion spans and activated It identity under #5542.
13. **system-effect-bounded-negative**: An assertion on registry or filesystem state
    without established subject and sink names `ps_system_effect_unavailable`; no
    static proof of outcome is claimed.
14. **alias-precedence-negative**: Bare `ls` MUST NOT bind to `Get-ChildItem` owner
    without alias resolution evidence; unresolved names `ps_alias_or_proxy_unresolved`.
15. **dynamic-invocation-negative**: `& $name` or splatted calls name
    `ps_dynamic_invocation_unresolved` when target is not fixed.
16. **command-rendering-boundary-negative**: Citations of PowerShell paste helpers
    or RIPR human-output quoting MUST NOT appear as PowerShell adapter or Pester
    capability proof in docs or manifest joins.
17. **cross-language-edge-negative**: PowerShell calling .NET without a #5559 edge
    names `ps_dotnet_or_com_edge_missing`.
18. **activation-before-oracle-negative**: A registered It in `Pending` state MUST
    NOT receive `Should` oracle credit.

## Test Mapping

See `.ripr/traceability.toml` behavior `RIPR-SPEC-0403`. Executable tests land
with analysis implementation slices and judged corpora. Until then,
`fixtures/*/verdict-corpus/spec-scopes/RIPR-SPEC-0403.toml` marks the spec out
of runtime mutant corpora.

## Implementation Mapping

- Edition/context and module identity modeling: follow-on adapter work under #5600
- Owner/probe/stream typing: follow-on adapter work under #5600
- Pester discovery/run and `Should` mapping: follow-on adapter work under #5600
- Relations, limits, and output projection: follow-on adapter work under #5600
- Judged corpora and support evaluation: follow-on proof work under #5600

## Metrics

- `powershell_evidence_contract_registered`
- `powershell_capability_cell_index_complete`

## Failure Modes

- Treating project context or module lists as reach or oracle eligibility.
- Collapsing pipeline objects, host text, and native stdout into one channel.
- Crediting Pester discovery, hooks, or mocks as production reach without activation.
- Promoting preview PowerShell findings to gate or badge eligibility without
  promotion policy and proof.
- Citing command-rendering portability as PowerShell source-language support.
