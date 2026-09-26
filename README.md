<p align="center">
  <img src="assets/logo/ripr-icon-dark.svg" alt="ripr logo" width="120" />
</p>

<h1 align="center">ripr</h1>

<p align="center">
  <strong>Find changed behavior your tests reach but do not actually check.</strong>
</p>

<p align="center">
  ripr gives developers, reviewers, and coding agents one bounded next test to
  write, the command that verifies it, and a before/after receipt—without
  running mutation testing.
</p>

<p align="center">
  <a href="https://github.com/EffortlessMetrics/ripr/releases"><img src="https://img.shields.io/github/v/release/EffortlessMetrics/ripr?sort=semver&label=release" alt="GitHub release" /></a>
  <a href="https://crates.io/crates/ripr"><img src="https://img.shields.io/crates/v/ripr.svg" alt="crates.io version" /></a>
  <a href="https://docs.rs/ripr"><img src="https://docs.rs/ripr/badge.svg" alt="docs.rs" /></a>
  <a href="https://github.com/EffortlessMetrics/ripr/actions/workflows/ci.yml"><img src="https://github.com/EffortlessMetrics/ripr/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI" /></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="License: MIT OR Apache-2.0" /></a>
</p>

## Start with one change

From a Git repository with a committed change:

```bash
cargo install ripr
cd your-repository
ripr check
```

`ripr check` is the first useful command. It returns one bounded `Start here:`
result:

- one changed behavior whose current test check appears too weak;
- one safe next action; or
- an explicit no-action or limited state when ripr cannot justify repair advice.

A no-action result is not a clean bill of health. It means the current static
evidence did not earn a bounded repair route. Use `--format human-full` for the
complete evidence or `--format json` for machine-readable output.

The zero-config path is intentional. `ripr.toml` is optional.

Using VS Code instead? Install `EffortlessMetrics.ripr`, open a workspace, and
start with **ripr: Show Status**. The extension manages its server; a separate
`cargo install ripr` is not required for the normal editor path.

## Repair one gap

For guided repository adoption, let `pilot` choose one current, repair-ready
work item:

```bash
ripr pilot --root .
```

It prints the repo-scoped ID and the exact command for the before phase. The
manual form is:

```bash
ripr agent repair --root . --seam-id <seam-id> --phase before
# edit one focused test outside ripr
ripr agent repair --root . --attempt <repair-attempt-id> --phase after
```

ripr owns the evidence, currentness checks, edit boundary, verification route,
and receipt. You—or an external coding agent—own the test edit. ripr does not
silently edit production code, generate a whole test, or report the edit as
verified merely because a file changed.

The seam ID selects the work item. The repair-attempt ID continues the prepared
before/after transaction. Probe IDs printed by `ripr check` are a different,
diff-scoped identifier. See [Repair attempt identity](docs/REPAIR_ATTEMPT.md)
for continuation and recovery.

## The question ripr answers

```text
coverage:          did this code execute?
ripr:              would a current test notice this changed behavior breaking?
mutation testing:  did a test fail when a concrete mutant ran?
```

ripr is **static mutation-exposure analysis**. It asks the mutation-testing
question early and cheaply while a change is still moving. It does not run
mutants, replace coverage, or prove correctness or test adequacy. Runtime
mutation testing remains the execution backstop.

Public docs use plain language first. Specs and machine output use terms such as
*seam*, *discriminator*, *oracle*, and *canonical gap*; the
[Terminology bridge](docs/TERMINOLOGY.md) maps those terms to the user job.

## What ripr returns

For a supported, repair-ready change, ripr can provide:

- the changed behavior and why the current check looks weak;
- the related test and exact missing boundary, value, variant, or effect;
- a bounded, test-only work order with allowed and forbidden files;
- the focused project command that verifies the repair; and
- a durable before/after receipt that keeps static movement separate from
  executed verification.

When one of those facts is missing, ripr under-emits and names the limitation
instead of inventing a target or stronger claim.

## Start from the surface you already use

| Surface | First action | Result |
| --- | --- | --- |
| CLI | `ripr check` | One selected gap or an honest no-action/limited state. |
| VS Code | Install `EffortlessMetrics.ripr`; run **ripr: Show Status** | Saved-workspace diagnostics, hover evidence, and bounded actions. |
| GitHub Actions | `ripr init --ci github` | Advisory PR summary and retained artifacts. |
| Coding agent | `ripr pilot --root .`, then the printed repair command | One bounded test-only work order and receipt path. |
| MCP client | `ripr mcp --stdio` | Read-only workspace status. |

See [Quickstart](docs/QUICKSTART.md) for the full first-hour paths.

## Language scope

| Language | Current public status |
| --- | --- |
| Rust | Main product path. The bounded gap-repair transaction is `usable alpha`; real-repository route yield and ordinary-user success are still being measured. |
| TypeScript / JavaScript | Opt-in preview for `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, and `.cjs`. Static findings are advisory and do not imply Rust parity. |
| Python | Preview static facts, with a scoped `usable alpha` repair route for selected pytest/unittest shapes when every required fact is present. |
| Perl | Preview/advisory in a custom `lang-perl` build with a compatible fact exporter; not yet a normal released-install path. |

Preview language packaging is not support promotion. A missing packet, static
limit, or inspect-only result stays visible and non-actionable. Modern module
extensions are analyzed as preview inputs, but some downstream repair and
rerun surfaces can still under-emit until they consume the same extension
authority. Read
[Language adapter preview](docs/LANGUAGE_ADAPTER_PREVIEW.md) and
[Support tiers](docs/status/SUPPORT_TIERS.md) before adopting preview evidence
as policy.

## Installation and environment

### Build or install the CLI with Cargo

```bash
cargo install ripr
```

Building or installing ripr from source requires **Rust 1.95 or newer** and the
Rust 2024 edition toolchain.

That build MSRV is not a minimum Rust version for the repository being
analyzed. An already-built ripr binary can statically inspect a repository that
pins an older compiler. Project verification commands still use that
repository's own selected toolchain and can succeed, fail, or be unavailable
independently of static analysis.

For development from this checkout:

```bash
cargo install --path crates/ripr
```

### Repository requirements

- Git must be available on `PATH`.
- Run against a Git repository.
- By default, `ripr check` resolves the remote default branch (`origin/HEAD`),
  then common local/remote fallbacks. Pass `--base <ref>` when you need an
  explicit base.
- Committed history is the default input. Add `--worktree` to include staged
  and unstaged edits.
- Use `ripr doctor --root .` when analysis cannot start or when you need to
  inspect loaded configuration, language availability, and tool state. It is a
  diagnostic command, not the first-value analysis path.

## Repository-wide advisory badges

<a href="https://github.com/EffortlessMetrics/ripr/blob/main/docs/BADGE_POLICY.md"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/EffortlessMetrics/ripr/main/badges/ripr.json" alt="ripr open gaps" /></a>
<a href="https://github.com/EffortlessMetrics/ripr/blob/main/docs/BADGE_POLICY.md"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/EffortlessMetrics/ripr/main/badges/ripr-plus.json" alt="ripr+ test-efficiency gaps" /></a>

These generated, repository-scoped badges count unresolved actionable static
repair gaps. `ripr+` adds actionable test-efficiency repairs projected into the
same repair/verify/receipt model. They are not coverage, runtime mutation
results, all behavior seams, or all code without tests. Diff-scoped evidence
belongs in PR summaries and CI artifacts. See
[Badge policy](docs/BADGE_POLICY.md).

## Trust boundary

ripr is alpha software and deliberately conservative:

- static evidence is not runtime proof;
- preview findings are advisory;
- `ripr check` is not a merge gate;
- generated CI is advisory until a repository explicitly adopts a gate;
- MCP and the language server do not gain source-edit authority merely because
  they can describe a repair; and
- failed, stale, partial, wrong-root, or incomparable evidence cannot become a
  successful receipt.

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

The wording is intentionally limited. It says what the static evidence found
and what test work is justified; it does not claim a runtime mutant result.

## Documentation

| Need | Read |
| --- | --- |
| Get useful output in the first hour | [Quickstart](docs/QUICKSTART.md) |
| Understand command roles | [Command hierarchy](docs/COMMAND_HIERARCHY.md) |
| Translate internal vocabulary | [Terminology](docs/TERMINOLOGY.md) |
| Understand the analysis model | [Static exposure model](docs/STATIC_EXPOSURE_MODEL.md) |
| See support and maturity boundaries | [Support tiers](docs/status/SUPPORT_TIERS.md) |
| Evaluate preview languages | [Language adapter preview](docs/LANGUAGE_ADAPTER_PREVIEW.md) |
| Turn a gap into a focused test | [Targeted test workflow](docs/TARGETED_TEST_WORKFLOW.md) |
| Install and use the extension | [Editor extension](docs/EDITOR_EXTENSION.md) |
| Integrate advisory CI | [CI strategy](docs/CI.md) |
| Read output and schema contracts | [Output schema](docs/OUTPUT_SCHEMA.md) |
| Browse all documentation | [Documentation index](docs/README.md) |

## Development and releases

`EffortlessMetrics/ripr-swarm` is the development trunk. Public source,
release tags, GitHub Releases, crates.io publication, server assets, and
marketplace distribution are owned by
[`EffortlessMetrics/ripr`](https://github.com/EffortlessMetrics/ripr).

Contributors should use the repository automation:

```bash
cargo xtask shape
cargo xtask check-pr
```

See [Contributing](CONTRIBUTING.md),
[Scoped PR contract](docs/SCOPED_PR_CONTRACT.md), and
[Agent instructions](AGENTS.md).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
