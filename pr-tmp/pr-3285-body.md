# Production delta

Closes #3285 — S4 of #3213, governed by **RIPR-SPEC-0155** (new, `proposed`): every actionability-bearing surface now consumes the one producer-owned source-role model, and the legacy path predicate is retired from production code.

- **The LSP scope partition converges** on `classify_with` with declared Cargo targets and the repository's `production_like_targets` opt-in — the same authority as diff seeding and the seam inventory, fed from the same `config.repo_config()` single source. An opted-in target now keeps its editor projection (reproduced dropped on main by the new opt-in test); hover/actions/status inherit the converged snapshot. Because partition and seeding share one authority and one config, the partition suppresses nothing seeding already excluded — `out_of_scope_test_file_findings` becomes a structural zero for ordinary configs and remains the typed safety net (its status-payload machinery unchanged, every nonzero pin rewritten to zero).
- **`is_production_rust_path` is deleted from production code** — its contract (src requirement, exclusion components, `tests.rs` stem) lives on in the role model's layout base, pinned by the carry-over test; the last `#[cfg(test)]` consumers (seam inventory test entries) converge too. Zero production references remain (grep-verified; the symbol was never crate-root public).
- **The `source_role_harness_suppression` fixture** pins the full downstream-suppression vocabulary in one changed integration test — `Result<()>` with `?`, `map_err` chains, `Ok(())` terminals, an Err-return guard (crediting via #3284's twin construction), a harness-only `.contains()` output check, assertion-driver helpers — at **zero production obligations** (all three findings on the production owner; `tests/contract.rs` appears only as related-test evidence) while the exact `assert_eq!` boundary assertion keeps crediting strong.

# Evidence delta

- **Opt-in editor projection reproduced failing on main** (the exact divergence documented in-code since #3283), then fixed; the two LSP scope tests that pinned the divergence are rewritten to the converged contract (nested-src example — a production subject under the declared #3283 divergence — KEEPS its projection; test-only fixture moved to a cargo-discoverable example). Reviewer confirmed all three discriminate against a path-predicate regression.
- Adversarial review (separate agent, eight challenge areas): **no blocking findings** — anchor-only context sufficiency proven (the partition's manifest set is a subset of seeding's, so it can never over-suppress; opt-in precedes declared-target in `classify_with` ordering), config single-sourcing traced end to end, deletion cleanliness repo-wide, zero-drift structural (the predicate's only production consumer was this partition). Its one comment-level finding (stale caller comment describing the pre-fix state) fixed in c471c318.
- Fixture verified by direct run: 3 probes all `src/lib.rs`, Err-guard credited `relational_check`, `assert_eq!` credited strong — exactly the SPEC Must-Not/Must shape.
- 4316 lib tests, workspace clippy clean, `goldens check` zero drift (structural), `fixtures`, `dogfood`, `precommit`, full policy battery green.

# Acceptance matrix (#3285)

| Acceptance | Status |
| --- | --- |
| All surfaces agree on role/eligibility | ✅ one authority; the last divergent surface converged |
| Harness code: no production obligation, evidence retained | ✅ fixture + #3284 family |
| Production controls with identical syntax stay eligible | ✅ nested-example projection pin + owner findings |
| Unknown/ambiguous explicit, fail-closed | ✅ role model gates (from #3283) unchanged |
| Suppression reproducers pass without suppression | ✅ in-repo shapes (upstream files absent here); downstream removal blocked on published release |
| Recurrence: removing the shared projection fails a test | ✅ three discriminating LSP tests (reviewer-verified) |
| Downstream fixture/receipt for #9907 | ✅ `source_role_harness_suppression` golden trio (the established producer-authentic export pattern) |

# Non-claims

- #3213 row 7 (downstream suppression removal) is blocked on a published release carrying this proof — this PR completes rows 1-6; the parent closes with row 7 honestly dispositioned as release-blocked.
- No schema bump: findings are production subjects by construction, so a per-finding role field would be constant.
- No TS/Python adapter test-file models (adapter-owned).

Candidate head: `c471c318` (base `origin/main` @ `96002f06`).
