# Muse integration

RIPR's native Muse package lives at
[`plugins/muse/ripr`](../../plugins/muse/ripr/README.md). Start with that guide
for installation, approval, a first repair, input selection, and troubleshooting.
The [native manifest](../../plugins/muse/ripr/.muse-plugin/plugin.json) is the
authority for the installed capabilities. The Meta-plugin workstream currently
ships this Muse adapter; Claude Code, Codex, and Agent Plugins distributions
are separate adapters and are not part of plugin 0.1.0.

## Declared capabilities

| Capability | Identity | Contract |
| --- | --- | --- |
| MCP server | `core`, stdio, `ripr mcp --stdio` | Existing [read-only MCP server](mcp.md); no new transport or source-edit/test-execution authority. |
| Setup skill | `ripr-setup`, qualified invocation `/ripr:ripr-setup` | Binary capability, server approval, live tools, startup root, and an explicit first analysis. |
| Repair skill | `ripr-repair`, qualified invocation `/ripr:ripr-repair` | One bounded repair with a [durable CLI attempt](../REPAIR_ATTEMPT.md) and separately executed checks. |

The package declares no separate commands, hooks, reminders, bundled
executable, or credentials. Plugin version 0.1.0 is independent of the RIPR
CLI version. Its native manifest declares the MCP server directly; these
receipts do not establish support for another host's Agent Plugins import.

## Session and approval behavior

Muse can start successfully while a plugin MCP server is absent or
unapproved. Inspect the installed capability and the live tools; application
startup alone does not establish a working RIPR session.

The declared capability is `plugin:ripr:mcp_server:core`. The installation
guide uses that full identity for approval and re-inspects it for
`trusted_enabled`. Both skills can be effective while the server still
requires review. After editing the package's installation source, run
`muse plugins update ripr --json`, then inspect it again. Approval covers the
whole installed package: even a README or skill edit returns an approved MCP
capability to `modified`. That state and `review_needed` both require review
and approval of the current package before the server can load. An unchanged
update retains approval. Installation, approval, and package updates take effect
in new sessions; start a fresh session after the capability is
`trusted_enabled`. Muse's [trust rules](https://meta-models.github.io/muse-code-sdk/next/guides/plugins/concepts/trust-review-and-scopes/)
define these states.

The manifest does not specify `--root`. The MCP process resolves its root at
startup from the session's launch directory and repository markers. A later
shell `cd` cannot retarget it. `ripr_workspace_status` takes no arguments;
it reports validation posture and a hashed identity, not an absolute host
path. Launch a new session from the intended repository when the root differs
or cannot be established.

Tools appear under the host's plugin namespace. Match these ten tool names
within that namespace:

| Purpose | Tool names |
| --- | --- |
| Establish current inputs | `ripr_workspace_status`, `ripr_refresh` |
| Read gaps and limitations | `ripr_list_gaps`, `ripr_get_gap`, `ripr_get_top_limitation` |
| Inspect repair readiness | `ripr_get_repair_card`, `ripr_prepare_repair` |
| Discover and read attempts | `ripr_list_repair_attempts`, `ripr_get_repair_attempt`, `ripr_get_receipt_status` |

`ripr_prepare_repair` is in-memory session state. It does not create a durable
CLI baseline, authorize edits, or run tests. A refresh can write its analysis
cache under `target/ripr/` while remaining read-only with respect to source.

## Binary and analysis inputs

The binary on Muse's inherited `PATH` must expose `ripr mcp`. Check both
`ripr --version` and `ripr mcp --help`, then discover the live tools. The
[historical cold-install receipt](../../benchmarks/muse-plugin/receipts/2026-11-04-cold-install-and-repair.md)
records the released 0.10.0 binary refusing that subcommand and a pinned
0.11.0 development binary serving it. A version label alone is insufficient,
and a development-build result is not a released-product claim.

Pass `source` on every refresh and repeat any explicitly selected `base`.
Omission independently restores `committed` source or the default base; these
are not sticky settings. Saved worktree analysis excludes wholly untracked
files until deliberately included and never reads unsaved editor buffers.
Check the captured source/base, snapshot identity, freshness, and typed outcome
before dependent reads. A failed refresh's last-known-good snapshot is not
new after-edit evidence. The [MCP reference](mcp.md) owns these schemas and
failure contracts.

## Validation and evidence

Use the [package installation sequence](../../plugins/muse/ripr/README.md#install-and-check-the-connection)
for plugin and skill validation, installation, approval, and a fresh session.
Manifest validation establishes package structure, not tool discovery or a
successful repair.

The [evaluation suite](../../benchmarks/muse-plugin/README.md) defines eight
scenarios and the retained evidence boundaries. Its
[run-record schema](../../schemas/ripr/muse-plugin-eval.schema.json) is distinct
from the scenario expected-verdict files and historical prose receipts.
Plugin results report beside the DX scoreboard; these runs do not change
scoreboard baselines or establish effectiveness targets.
