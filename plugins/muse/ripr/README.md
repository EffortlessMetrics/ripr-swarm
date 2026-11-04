# RIPR for Muse

Static mutation-exposure evidence and guided test repair for Claude
Code. RIPR asks: for the behavior changed in this diff, do the current
tests appear to contain a discriminator that would notice if that
behavior were wrong?

This is a thin native Muse plugin (version 0.1.0, independent of
the RIPR CLI version) over RIPR's existing interfaces. It adds no
analyzer, no repair engine, no hooks, and no background daemon:

- MCP server `core`: `ripr mcp --stdio` — read-only workspace status,
  refresh, gap evidence, and repair cards. It never edits or executes.
- Skill `ripr-setup`: session diagnostics (binary, approval, live tool
  inventory, first analysis) with explicit outcomes.
- Skill `ripr-repair`: one guided repair — durable CLI attempt, focused
  test edit, separately executed proof, static after-phase.

## Prerequisite

An MCP-capable `ripr` binary on `PATH` (current 0.11+ builds; released
`0.10.0` has no `mcp` command and cannot serve this plugin). The plugin
never downloads or installs it. No bundled executable ships in 0.1.0.

## Install (development package)

From the repository checkout, with Muse 1.4.14:

```text
muse plugins validate ./plugins/muse/ripr
muse skills validate ./plugins/muse/ripr/skills/ripr-setup
muse skills validate ./plugins/muse/ripr/skills/ripr-repair
muse plugins install ./plugins/muse/ripr
muse plugins inspect ripr
muse plugins approve ripr:mcp_server:core
```

Then start a **new** Muse session (installation, approval, and
package updates affect new sessions, not a running one; a package
update invalidates the prior server approval). Invoke setup first —
`/ripr:ripr-setup` is the unambiguous form; `/ripr-setup` and
`/ripr-repair` work when no higher-priority skill shadows them. The
qualified repair form is `/ripr:ripr-repair`.

A local development installation must work before any marketplace
publication; distribution through the `ripr` repository catalog is a
separate, later step with its own review.

## Claim boundary

RIPR classes (`exposed`, `weakly_exposed`, `reachable_unrevealed`,
`no_static_path`, `infection_unknown`, `propagation_unknown`,
`static_unknown`) are static evidence. This plugin never presents them
as runtime mutation outcomes, and a completed repair requires both a
better static read on the exact item and an executed test that rejects
the known wrong behavior.

Evaluation contract, scenarios, and receipts live under
`benchmarks/muse-plugin/`; the host integration notes are in
`docs/interop/muse-code.md`.
