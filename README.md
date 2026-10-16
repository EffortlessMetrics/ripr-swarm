# Standalone native CLI output audit — 2026-10-16

Source: EffortlessMetrics/ripr-swarm `d08c77d144c016078364fcc1fcf2fa593d21cefc` (live main rechecked before publication). Development binary SHA-256 `73441c38399894fd6c21f9dd1893d4eb90505b54f509033233afac781d29591c`; Rust 1.95.0, Linux x86_64, default features. Exact tree, tool paths, build command and caveats: `identity.json`; successful build log: `build.log`.

Read root AGENTS.md, `.agents/skills/prepare-issue/SKILL.md`, `.agents/skills/prepare-proof/SKILL.md` and current output/exit/spec owners. No production edits, PR, fanout changes, Ready/merge/thread resolution, external bot contact, release, deployment, spending, credential/security changes, destructive cleanup or workflow changes.

## Result

130 bounded standalone native invocations; every JSON/SARIF output selected for parsing parsed as one complete document. Raw stdout and stderr, exact argv, environment overrides, terminal statuses, fixtures and source/search identities are retained here. This is local behavior evidence, not hosted qualification or a full test suite.

One narrow verified-current defect: preview-language analyzed disclosure ignores the selected partial partition. With Rust + Python and Rust + TypeScript controls, file budget 1 selected only `src/lib.rs`, zero preview changed files and only a Rust finding. Nonetheless `preview_languages[].analyzed=true`, file_count=1, with the omitted preview path in sample_paths; human output claimed one preview file analyzed. Budget 3 selected the preview source and produced its finding. The top-level incomplete outcome, badge warning and advisory exit 0 were correct; do not change those semantics to repair the disclosure.

Evidence: `results/mixed-partial-budget1/`, `results/mixed-partial-ts-budget1/`, and budget3 counterparts; `partition-control-receipt.json`. Main source `analysis/pipeline.rs` filters adapter input through `partial_scope.selects` (around 1010), but constructs preview advisories from `analysis_changed_files` (around 1121). `analysis/mod.rs::PreviewLanguageAdvisory::analyzed` checks configuration/count/non-success rows without the selected partition. JSON and human consume that mismatch.

Prior art: closed #4372 / merged #4472 corrected generated/vendor exclusions and explicitly left partial-scope overcount outside that patch. #3252/#3253 corrected failed-adapter claims. Open #5546 → #6397/#6399 and #6468/#6475/#6479 own broader mixed aggregation/projection. The narrow disclosure finding is follow-up evidence, not a replacement aggregation implementation. All-state finding-specific issue/PR searches are retained in `search-receipt.json`; broad keyword searches were treated as leads only. No exact active repair was identified in the retrieved results; search is not an exhaustive repository-wide absence proof.

## Observed contract matrix

| Native case | Exit | Machine evidence / human guidance |
|---|---:|---|
| Rust/Python/TypeScript ordinary check | 0 | complete_with_findings; expected finding retained |
| Empty named diff / clean Git range | 0 | no_scope, complete; no behavior analyzed |
| Garbage or valid prefix plus truncated hunk, all three languages | 2 | unsupported_input, incomplete, retained valid prefix finding and recovery; single document |
| Committed conflicts, all three languages, check and diff | 2 | unsupported_input in JSON; human report plus stderr diagnostic |
| Disabled Rust/Python/TypeScript | 0 | partial_with_limitations; badge warns; enable-language guidance |
| Malformed Python/TypeScript source | 0 | partial_with_limitations, parser limitation; advisory evidence |
| Missing Rust changed file / unsupported Go sibling | 0 | partial_with_limitations; supported Rust finding retained; missing/unsupported scope named |
| File-budget partition | 0 | limited_partial_scope, downstream_consumable=false, gate_eligibility=ineligible; exact selected path and omissions |
| Hard Rust diff limit | 2 | JSON diff_scope_oversized refusal, non-consumable; badge generic analysis_failed refusal (its documented structural error classification) |
| Invalid partial budget, Rust/Python/TypeScript scope | 2 | non-consumable analysis_failed document; diagnostic names partial_budget_invalid and variable |
| JSON finding-array budget 1, two-finding Rust stimulus | 0 | one of two findings; limited_findings_bound, downstream_consumable=false; full analysis counts retained |
| Same rendering budget 0 | 0 | both findings; no render limitation; native badge/human remain independent projections |
| Perl without compiled adapter | 0 | partial_with_limitations; analyzed=false; actual build/exporter prerequisites named |
| Explicit Perl enablement without compiled feature | 2 | config_invalid refusal and matching prerequisites |
| Selected incomplete cases, SARIF and GitHub | 0 partial / 2 unsupported | SARIF run properties retain incomplete outcome; GitHub warning names limitation and recovery |

A Python valid-but-unmodeled lambda fixture was initially named `python-parse`; it establishes complete analysis with static_unknown, not a parser failure. A separate `python-malformed-source` control contains invalid syntax. Initial one-finding render cases cannot establish truncation; the dedicated `render-bound` two-finding control does. Preserve these distinctions when consuming receipts.

Preview-only badges intentionally omit calibrated preview gap counts (#6761/#6802); their green zero is not filed as a new defect. Completeness, calibrated policy scope, rendering completeness and process completion are separate facts.

## Reproduce

Build the frozen source with `cargo build --locked -p ripr --bin ripr`, then check `ripr --version` and SHA-256. See identity.json for this environment's Cargo/Rustup homes. No release-profile claim.

The `cases/` folders retain exact bytes without caches. For the narrow defect run independently:

```bash
RIPR_PARTIAL_DIFF_FILE_BUDGET=1 /absolute/path/ripr check --root /absolute/path/cases/mixed-partial --diff /absolute/path/cases/mixed-partial/change.patch --mode fast --format json --quiet
RIPR_PARTIAL_DIFF_FILE_BUDGET=1 /absolute/path/ripr check --root /absolute/path/cases/mixed-partial --diff /absolute/path/cases/mixed-partial/change.patch --mode fast --format human --quiet
```

Repeat with budget 3, and with `mixed-partial-ts`. Literal discriminator at budget1: selected_files exactly `["src/lib.rs"]`, preview files in summary equal 0, no preview finding, yet preview_languages analyzed true and human note claims one preview file analyzed. At budget3 there is no partial scope block, preview files equal 1 and the preview finding exists. JSON stdout must be parsed in full; do not infer a pass from status alone.

`run.py`, `extra.py`, `boundaries.py`, `git_commands.py` reproduce the wider matrices after adjusting their top-level source/evidence/binary paths. The partition controls' complete exact argv and budget are in `partition-control-receipt.json`; their fixture bytes are already retained. `git_commands.py` creates only its own tiny scratch repositories, uses local fixture commit identity, and retains base/head IDs; archived git metadata is omitted intentionally.

## Handoff boundary

#8201 remains sole native fanout owner. No fanout invocation, candidate edit or candidate qualification occurred. Standalone behavior is the comparison authority; fanout should retain it until its native owner changes. The narrow repair should make analyzed claims honor existing selected input, preserving disabled/unavailable presence disclosure, caps, identities, classifiers, exits, gate policy and preview tier. Positive selected-preview and negative omitted-preview controls should cover Python and TypeScript/JavaScript. Broader aggregation stays with #5546 descendants. No production repair or CI clearance is claimed. No denied actions or pending authorization blockers.
