---
name: ripr-setup
description: Establish a usable RIPR session in Muse — verify the binary, plugin MCP approval, live tool inventory, workspace status, and a first explicit analysis. Use before any RIPR-guided repair, or when RIPR tools seem missing.
---

# RIPR setup

A short diagnostic, not an installer. Never silently change the machine:
do not write MCP configuration, do not download a binary, and do not work
around a failed step. Successful Muse startup is not proof that
this plugin works — plugin MCP servers are optional at startup, so a
failed server leaves the session running without any RIPR tools. Inspect
the live tool inventory and stop visibly when the server is absent.

## Steps

1. Identify the current Git worktree and HEAD (`git rev-parse HEAD`,
   branch or detached state, clean or dirty). Record them.
2. Confirm the binary: run `ripr --version` and report it verbatim. The
   binary must expose the MCP server. Released `ripr 0.10.0` does not:
   `ripr mcp --stdio` exits 2 (`unknown command "mcp"`), its server
   never starts, and no `ripr_*` tools register. A current (0.11+) build
   is required.
3. Confirm the plugin and approval state with `muse plugins inspect
   ripr` when the CLI is available: the `ripr-setup` and `ripr-repair`
   skills effective, and the MCP capability `plugin:ripr:mcp_server:core`
   present. If it is `review_needed`, the result is `approval_required`
   below — tell the user to approve it (`muse plugins approve
   ripr:mcp_server:core`) and start a new session; approval does not
   retarget an already-running session.
4. Check the live tool inventory in this session. Expect
   `ripr_workspace_status`, `ripr_refresh`, `ripr_list_gaps`,
   `ripr_get_gap`, `ripr_get_top_limitation`, `ripr_prepare_repair`,
   `ripr_get_repair_attempt`, `ripr_get_receipt_status`,
   `ripr_get_repair_card`, and `ripr_list_repair_attempts`. Absent tools
   with an approved server mean `server_unavailable`; absent tools with
   a 0.10.x binary mean the version floor in step 2.
5. Call `ripr_workspace_status`. Record the resolved root posture, that
   server edit and execution authority read `none`, the attempt state,
   and any limitation. The server resolves its workspace when the MCP
   process starts: if the intended repository or worktree differs from
   the one this session started in, stop and ask for a new session
   started at the intended root. Changing shell directories does not
   retarget the running server.
6. Run one explicit first analysis with `ripr_refresh`. Report the
   outcome kind, snapshot id, and `fresh_for_working_tree`. An empty
   scope is not an all-clear: surface `empty_scope_hint` when present.

## Outcomes (report exactly one, with its evidence)

| Condition | Result |
| --- | --- |
| Binary present, server approved, usable snapshot | `ready` |
| Plugin installed but MCP server unapproved | `approval_required` |
| Server approved but tools absent | `server_unavailable` |
| No Git analysis scope | `no_scope` (never “all clear”) |
| Wrong or unresolved base | `base_required`, with the explicit recovery |
| Unsupported or incomplete analysis | `limited`, with the producer explanation |

Close with the outcome word, the snapshot id when one exists, and any
limitation in the producer's own terms. Do not claim support beyond
what these calls just demonstrated in this session.
