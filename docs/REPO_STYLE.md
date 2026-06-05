# Repo style

This repository is operated as an evidence machine.

Rust and `xtask` are the default construction material. Non-Rust files,
unsafe, panic paths, lint suppressions, generated files, workflow behavior,
process/network access, expensive CI lanes, and release claims must be owned
and receipted.

Static evidence runs first:

- `cargo-allow` for durable source exceptions when that upstream ledger is
  available for the exception class;
- `ripr` for static mutation-exposure analysis;
- `unsafe-review` for unsafe-contract reviewability when unsafe, FFI, raw
  pointer, layout-sensitive, or similar seams exist;
- rustc and Clippy for code-shape policy.

Runtime evidence runs where it pays:

- focused tests on ordinary PRs;
- targeted mutation for risk-bearing PRs;
- broader mutation, Miri, fuzz, and coverage lanes on nightly, main, release,
  or explicitly labelled runs.

CI is designed for proof per Linux-equivalent minute. Default PRs are cheap,
deterministic, and high-signal. Deep validation is preserved, but routed by
risk pack, label, main, nightly, or release.

Agents work one review-fast PR at a time. Review-fast does not mean tiny. It
means a coherent seam, nearby evidence, efficient verification, and an honest
claim boundary. Do not broaden scope to satisfy CI. Do not add invisible
exceptions.

## Tool roles

The durable target model is consolidated rather than allowlist-heavy:

| Tool | Repo role |
| --- | --- |
| `cargo-allow` | Source exception ledger for syntax-visible retained exceptions. |
| `ripr` | Static mutation-exposure signal for changed behavior and nearby tests. |
| `unsafe-review` | Advisory unsafe-contract reviewability for changed unsafe seams. |
| `xtask` | Repo control plane: orchestration, receipts, policy glue, CI, and release checks. |
| `cargo-mutants` | Runtime mutation backstop where risk justifies the cost. |
| Miri | Concrete UB execution backstop for selected witnesses. |
| Codecov / coverage receipts | Execution-surface telemetry, not a replacement for discriminating assertions. |

`xtask` wraps tools, aggregates receipts, and enforces repo-local glue. It must
not grow into a second implementation of the upstream tools it orchestrates.

## Exception doctrine

No source exception should be invisible. Unsafe, panic-family calls, lint
suppressions, generated source, checked-in executable files, workflows,
non-Rust files, process access, network access, and release-affecting claims
need an owner, reason, scope, and review path.

This repository currently has repo-local policy ledgers for several exception
classes. The target consolidation path is to use `cargo-allow` as the primary
source exception ledger where it can own the class, while keeping companion
ledgers only where they add behavior-specific semantics that a source ledger
cannot express.

## ripr boundary

`ripr` is static mutation-exposure analysis. It catches weak test/oracle signal
earlier and cheaper than runtime mutation because it reads the changed behavior
and current test suite statically at PR time.

`ripr` does not run mutants, execute tests, claim runtime mutation outcomes,
prove correctness, or replace runtime mutation testing. Mutation testing remains
the slower runtime backstop.

Static findings must keep the conservative vocabulary from `AGENTS.md`:

- `exposed`
- `weakly_exposed`
- `reachable_unrevealed`
- `no_static_path`
- `infection_unknown`
- `propagation_unknown`
- `static_unknown`

## CI economics

CI is not reduced because less verification is desired. Wasted CI is reduced so
more verification can be afforded where it matters.

LEM means Linux-equivalent minutes:

```text
LEM = wall-clock minutes × runner multiplier
```

Ordinary PRs should prefer the cheap lane that catches the relevant failure
mode. Expensive lanes belong behind risk packs, labels, main/nightly schedules,
or release readiness checks unless the PR changes the lane itself.

## Agent operating rule

For each PR:

1. Inspect current repo state and nearby policy.
2. Keep one coherent behavior, evidence, or policy slice.
3. Preserve the chain: spec or doctrine → test/fixture/receipt → code or docs →
   output/policy contract → metric where applicable.
4. Run the relevant acceptance checks.
5. Commit the shaped diff and open or update the PR with purpose, files,
   evidence, risk, and rollback boundary.
6. Clean up generated residue and stale local state before moving to the next
   work item.
