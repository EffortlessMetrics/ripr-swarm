---
description: Verify the RIPR plugin wiring (binary, MCP server, workspace status) without hidden configuration
argument-hint: [repository-root]
---

Verify the RIPR integration in this session, step by step. Do not write
any configuration files and do not work around a failed step.

1. Confirm the binary: run `ripr --version` and report it verbatim. If
   `ripr` is not on PATH, stop and tell the user to install RIPR first
   (for example `cargo install ripr` or their pinned internal build).
   The plugin declares only the MCP server; it never downloads a binary.
   The binary must expose the MCP server: released `ripr 0.10.0` does
   not (`ripr mcp --stdio` exits 2 with `unknown command "mcp"` and no
   `ripr_*` tools ever register). If tool discovery in step 2 finds no
   `ripr_*` tools and the version is 0.10.x, report exactly that: the
   installed binary predates the MCP server; a current (0.11+) build is
   required. Do not work around it with a hand-written MCP entry.
2. Confirm tool discovery: list the tools the `ripr` MCP server exposes.
   Expect `ripr_workspace_status`, `ripr_refresh`, `ripr_list_gaps`,
   `ripr_get_gap`, `ripr_get_top_limitation`, `ripr_prepare_repair`,
   `ripr_get_repair_attempt`, `ripr_get_receipt_status`,
   `ripr_get_repair_card`, and `ripr_list_repair_attempts`. If the server
   is absent or approval is pending, say exactly that and stop.
3. Call `ripr_workspace_status` (with the repository at $ARGUMENTS when
   one is given). Report: the resolved root posture, that server edit and
   execution authority read `none`, the current attempt state, and any
   limitation. Never print absolute host paths the server did not return.
4. Call `ripr_refresh` once and report the outcome kind, the snapshot id,
   and `fresh_for_working_tree`. An empty scope is not an all-clear;
   surface `empty_scope_hint` when present.
5. Finish with one line: setup is working, or the exact step that failed
   with its typed failure code. Do not claim support beyond what these
   calls just demonstrated in this session.
