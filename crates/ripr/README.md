# ripr

**Find changed behavior your tests reach but do not actually check.**

`ripr` gives developers, reviewers, and coding agents one bounded next test to
write, the command that verifies it, and a before/after receipt—without running
mutation testing.

## Start with one change

```bash
cargo install ripr
cd your-repository
ripr check
```

`ripr check` analyzes the current Git change and returns one bounded
`Start here:` result:

- one changed behavior whose current test check appears too weak;
- one safe next action; or
- an explicit no-action or limited state when the static evidence does not
  justify repair advice.

`ripr.toml` is optional. Add `--worktree` when staged or unstaged edits should
be included. Pass `--base <ref>` only when the repository's automatically
resolved default base is not the comparison you need.

A no-action result is not a clean bill of health. It means the current evidence
did not earn a bounded repair route. Use `--format human-full` for the complete
evidence or `--format json` for machine-readable output.

## Repair one gap

Use the guided repository path to select one current work item:

```bash
ripr pilot --root .
```

Then run the before command it prints, edit one focused test, and finish the
same transaction:

```bash
ripr agent repair --root . --seam-id <seam-id> --phase before
# edit one focused test outside ripr
ripr agent repair --root . --attempt <repair-attempt-id> --phase after
```

`ripr` owns the evidence, currentness checks, edit boundary, verification route,
and receipt. You—or an external coding agent—own the test edit. `ripr` does not
silently edit production code, generate a whole test, or report the edit as
verified merely because a file changed.

The seam ID selects the work item. The repair-attempt ID continues the prepared
before/after transaction. Probe IDs printed by `ripr check` are separate,
diff-scoped identifiers.

## The question ripr answers

```text
coverage:          did this code execute?
ripr:              would a current test notice this changed behavior breaking?
mutation testing:  did a test fail when a concrete mutant ran?
```

`ripr` is **static mutation-exposure analysis**. It asks the mutation-testing
question early and cheaply while a change is still moving. It does not run
mutants, replace coverage, or prove correctness or test adequacy. Runtime
mutation testing remains the execution backstop.

Public docs use plain language first. Specs and machine output use terms such as
*seam*, *discriminator*, *oracle*, and *canonical gap*; the
[Terminology bridge](https://github.com/EffortlessMetrics/ripr/blob/main/docs/TERMINOLOGY.md)
maps those terms to the user job.

## What ripr can return

For a supported, repair-ready change, `ripr` can provide:

- the changed behavior and why the current check looks weak;
- the related test and exact missing boundary, value, variant, or effect;
- a bounded, test-only work order with allowed and forbidden files;
- the focused project command that verifies the repair; and
- a durable before/after receipt that keeps static movement separate from
  executed verification.

When one of those facts is missing, `ripr` under-emits and names the limitation
instead of inventing a target or stronger claim.

## Example

Illustrative bounded output, with paths shortened:

```text
Start here:
  State: top_gap
  File: src/lib.rs:2
  Static exposure: weakly_exposed
  Changed behavior: amount >= discount_threshold
  Missing discriminator: amount == discount_threshold
  Related test: tests/pricing.rs:4 below_threshold_has_no_discount
  Next step: add exact below/equal/above boundary assertions

More:
  Full evidence: rerun with --format human-full
  Machine data: rerun with --format json
```

The output describes static evidence and justified test work. It does not claim
a runtime mutation result.

## Language scope

| Language | Current public status |
| --- | --- |
| Rust | Main product path. The bounded gap-repair transaction is `usable alpha`; real-repository route yield and ordinary-user success are still being measured. |
| TypeScript / JavaScript | Opt-in preview for `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, and `.cjs`. Static findings are advisory and do not imply Rust parity. |
| Python | Preview static facts, with a scoped `usable alpha` repair route for selected pytest/unittest shapes when every required fact is present. |
| Perl | Preview/advisory in a custom `lang-perl` build with a compatible fact exporter; not yet a normal released-install path. |

Preview language packaging is not support promotion. A missing packet, static
limit, or inspect-only result stays non-actionable. Modern module extensions
are analyzed as preview inputs, but some downstream repair and rerun surfaces
can still under-emit until they consume the same extension authority.

## Installation and toolchains

Installing from crates.io builds `ripr` locally:

```bash
cargo install ripr
```

Building or installing `ripr` from source requires **Rust 1.95 or newer** and
the Rust 2024 edition toolchain.

That build MSRV is not a minimum Rust version for the repository being
analyzed. An already-built `ripr` binary can statically inspect a repository
that pins an older compiler. Project verification commands still use that
repository's own selected toolchain and can succeed, fail, or be unavailable
independently of static analysis.

For development from the repository checkout:

```bash
cargo install --path crates/ripr
```

Git must be available on `PATH`, and analysis must target a Git repository.
Use `ripr doctor --root .` when analysis cannot start or when loaded
configuration, language availability, or tool state needs inspection. It is a
diagnostic command, not the first-value analysis path.

## Other surfaces

- **VS Code:** install `EffortlessMetrics.ripr`; start with **ripr: Show
  Status**. The extension manages its server.
- **GitHub Actions:** run `ripr init --ci github` for advisory PR output and
  retained artifacts.
- **MCP:** run `ripr mcp --stdio` for read-only workspace status.
- **LSP:** run `ripr lsp --stdio` for the experimental saved-workspace sidecar.

## Trust boundary

`ripr` is alpha software and deliberately conservative:

- static evidence is not runtime proof;
- preview findings are advisory;
- `ripr check` is not a merge gate;
- generated CI is advisory until a repository explicitly adopts a gate;
- MCP and the language server do not gain source-edit authority merely because
  they can describe a repair; and
- failed, stale, partial, wrong-root, or incomparable evidence cannot become a
  successful receipt.

## Documentation

- [Quickstart](https://github.com/EffortlessMetrics/ripr/blob/main/docs/QUICKSTART.md)
- [Command hierarchy](https://github.com/EffortlessMetrics/ripr/blob/main/docs/COMMAND_HIERARCHY.md)
- [Repair attempt identity](https://github.com/EffortlessMetrics/ripr/blob/main/docs/REPAIR_ATTEMPT.md)
- [Support tiers](https://github.com/EffortlessMetrics/ripr/blob/main/docs/status/SUPPORT_TIERS.md)
- [Language adapter preview](https://github.com/EffortlessMetrics/ripr/blob/main/docs/LANGUAGE_ADAPTER_PREVIEW.md)
- [Output schema](https://github.com/EffortlessMetrics/ripr/blob/main/docs/OUTPUT_SCHEMA.md)
- [Static exposure model](https://github.com/EffortlessMetrics/ripr/blob/main/docs/STATIC_EXPOSURE_MODEL.md)

Public source, releases, crates.io publication, server assets, and marketplace
distribution are owned by
[`EffortlessMetrics/ripr`](https://github.com/EffortlessMetrics/ripr).
Development happens in
[`EffortlessMetrics/ripr-swarm`](https://github.com/EffortlessMetrics/ripr-swarm).
