## What we learned (2026-07-19)

After a full session of issue review (140 threads), deep codebase audit (12 scouts, 197 issues filed), and 18 merged PRs, the major structural learnings are:

### 1. The gate decision path has a correctness gap (#1933)

The gap-decision-ledger blocking path is non-functional: `repair_route.rs:114` hardcodes `seam_id: None`, making `gate_repair_route_is_complete` always false for gap-ledger candidates. The tests named `*_fails_closed_*` actually assert `advisory`/exit-0 — fail-open at CI. This contradicts `CALIBRATED_GATE_POLICY.md:74-75`. Either the hardcoded None is a bug (should be populated from the gap record) or the gap-ledger blocking path is aspirational and the docs/tests need honest correction.

This is the **single most important finding** — it means ripr's gate cannot block on the exact findings it was designed to gate on.

### 2. The receipt chain is unverified (#1941)

`ripr agent verify` reads two JSON files and computes static movement — no test re-execution, no schema validation, no git binding. `ripr agent receipt` accepts any hand-authored `agent-verify.json`. `ripr receipt write` stamps any SHA. The entire repair-receipt loop is advisory at the verification layer — a fixer can fabricate a receipt that looks like valid evidence.

The mitigations (advisory status, safe_to_merge=false, provenance.runtime_mutation_execution=false) are presentation-layer claims, not enforcement. This is honest about ripr being static-only, but downstream consumers who treat the receipt as proof of testing are deceived.

### 3. The LSP loop re-runs everything on every save (#1908)

Every `did_save` triggers a full `git diff` + repo walk + Rust re-index. No incremental analysis, no cached index on the diff path, no debounce. The `saved_workspace_revision` counter defeats the dedup path (every save has a different identity). The 33ms–11s range means a moderately-sized diff costs full analysis time on every keystroke-save.

The cache infrastructure exists (`build_index_from_loaded_files_with_cache`) but is only wired into the repo-seam path, not the diff path. Wiring it in is the single highest-leverage performance fix.

### 4. The CI consumer experience is the adoption bottleneck (#2008/#2009/#2010)

The generated CI workflow (1) compiles ripr from source on every PR with no caching, (2) uses `continue-on-error: true` on ~30 of ~31 steps (silent green-on-error), and (3) depends on three undocumented repo variables. A consumer who runs the workflow sees green, assumes ripr found nothing, and moves on — never knowing the gate mode was empty or that steps silently failed.

### 5. No standard LSP progress signal (#1909)

The server never calls `window/workDoneProgress/create` or `$/progress`. Stock VS Code / Neovim users see nothing during 11s+ analysis. All feedback is either a custom notification (requires client opt-in) or output-panel logs. This is the primary editor-user complaint waiting to happen.

### 6. The codebase is unusually disciplined but has specific structural debt

**What's clean:** Zero `unwrap()`/`panic!` in production (enforced by xtask gate). Zero `TODO`/`FIXME`/`dead_code` (enforced by clippy + xtask). `unsafe_code = "forbid"`. Explicit debt ledger in `docs/DEFERRED.md`. 3,668 tests. 200 fixtures. Honest fail-closed posture on missing data.

**What's rough:** `Result<_, String>` everywhere (2,474 sites, no typed error enum). `cli/commands.rs` is a 10,715-line monolith (133 handlers). 4 duplicated `run_git` helpers. No string interning. Single-threaded index build. No snapshot/property testing.

### 7. Cross-language adapters diverge more than expected

- `SeamKind` is Rust-only; preview adapters use `ProbeFamily` (different variant set, no crosswalk)
- Perl has its own `OracleKind`/`OracleStrength` enums (12/5 variants) separate from the domain enum (9/6)
- Python and TypeScript never emit `ReachableUnrevealed`, `InfectionUnknown`, or `PropagationUnknown`
- `analyze_repo` is a stub for Python and TypeScript
- Perl produces only 1 of 15 `StaticLimitKind` values
- Run-status vocabulary is duplicated between `backend.rs` and `diagnostics.rs` with subtle drift

### 8. The honesty corpus has a graduation gap (#1945)

4 of 14 `python_adversarial_*` false-exposed guards are missing from the evidence-promotion-honesty corpus despite AGENTS.md:232-238 mandating graduation. These are protected only by `goldens check` (which accepts a CHANGELOG-blessed re-bless), not by the independent honesty gate that catches a dishonest re-bless without the CHANGELOG escape hatch.

### 9. The file-policy gate is a footgun for merged PRs (#1848)

A merged PR that adds a new file type (e.g., `.allow/conformance/*.json`) without adding a `non-rust-allowlist.toml` entry breaks `check-file-policy` on main for every subsequent PR. This happened during this session (the authority-map PR added the conformance fixture) and required an emergency fix. The gate should either be advisory or provide a clearer error message pointing to the allowlist.

### 10. The CI disk-pressure problem is structural (#1058/#1438/#1724)

The self-hosted runners (CX43/CPX42/CX53) are at 100% disk (7.3T). The scratch reaper threshold is 45 min, but cross-repo `_work` checkouts from other repos (adze-swarm 65GB, tokmd-swarm 17GB) are under 45 min during busy periods. The `du`-based largest-first sweep is not implemented. The fallback routing (#1494) works (GitHub-hosted fallback succeeds), but the hosted runner also fails under the same disk pressure. Multiple PRs during this session failed CI purely due to disk tempfail.
