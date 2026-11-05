# Muse integration

RIPR ships a native Muse plugin at `plugins/muse/ripr`
(`.muse-plugin/plugin.json` is the authoritative manifest). The native
manifest is deliberate: the formal cross-agent Agent Plugins format
does not currently import its MCP declarations as working Muse
servers, while a native manifest declares the server directly.
Claude Code, Codex, and Agent Plugins distributions may follow as
separate adapters over the same workflow contract; none ships in 0.1.0.

## What the package declares

| Capability | Identity | Notes |
| --- | --- | --- |
| MCP server | `core`, stdio, `ripr mcp --stdio` | The existing read-only server (see `mcp.md`, ADR 0022). No new transport. |
| Skill | `ripr-setup` | Session diagnostics with explicit outcomes. |
| Skill | `ripr-repair` | One bounded repair with durable CLI attempt and separately executed proof. |

No hooks, no reminders, no bundled executable, no credentials. Plugin
version (0.1.0) is independent of the RIPR CLI version.

## Host behavior that shapes the workflow

- MCP servers are optional at startup. A failed or unapproved server
  leaves the session running with no `ripr_*` tools; successful Claude
  Code startup therefore proves nothing about this plugin. Setup
  inspects the live tool inventory and stops visibly.
- Installation, approval, and package updates affect new sessions, not
  a running one. Updating the package invalidates the prior server
  approval.
- The MCP process resolves its workspace at startup. Moving to a
  different repository or worktree in the same session does not
  retarget it; start a new session at the intended root.
- Tools appear under the plugin namespace (for example the `core`
  server's refresh tool). Required names: `ripr_workspace_status`,
  `ripr_refresh`, `ripr_list_gaps`, `ripr_get_gap`,
  `ripr_get_top_limitation`, `ripr_prepare_repair`,
  `ripr_get_repair_attempt`, `ripr_get_receipt_status`,
  `ripr_get_repair_card`, `ripr_list_repair_attempts`.

## Prerequisite and version floor

The `ripr` binary must be on `PATH` and MCP-capable. Released
`ripr 0.10.0` has no `mcp` command (`ripr mcp --stdio` exits 2), so its
server never starts and no tools register; this was observed in the
2026-11-04 cold-install evaluation, not inferred. Development against a
pre-release binary must identify it as a development build; it does not
create a released-product claim.

## Validation and evaluation

```text
muse plugins validate ./plugins/muse/ripr
muse skills validate ./plugins/muse/ripr/skills/ripr-setup
muse skills validate ./plugins/muse/ripr/skills/ripr-repair
muse plugins install ./plugins/muse/ripr
muse plugins inspect ripr
muse plugins approve ripr:mcp_server:core
```

Manifest validation alone is insufficient. The acceptance journey —
cold install, approval, new session, tool discovery, boundary repair,
refusal cases — and its retained receipts live under
`benchmarks/muse-plugin/` (the `muse-plugin-eval-v1` JSON Schema
lands with the scoreboard-ingestion change that owns its version
bindings, #13120). Plugin results report
beside the DX scoreboard; plugin runs never edit its baselines.
