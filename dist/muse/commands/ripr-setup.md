---
description: Verify the RIPR plugin wiring (binary, MCP server, workspace status) without hidden configuration
argument-hint: [expected-repository-root]
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
3. Call `ripr_workspace_status` with no arguments. The server root is
   resolved once from the Muse session's startup working directory.
   $ARGUMENTS, if supplied, names an expected root for comparison, not a
   tool argument or a request the running server can apply. If the
   intended root differs or cannot be established, stop and start a fresh
   Muse session from that repository; a later shell `cd` does not retarget
   the server. Report the returned root posture and identity, server edit
   and execution authority (`none`), attempt state, and limitations. Never
   invent an absolute path from the returned hashed root identity.
4. Call `ripr_refresh` once with explicit `{"source": "committed"}`, or
   explicit `"worktree"` if the requested check includes saved edits.
   Repeat any explicitly selected `base`; neither option is sticky.
   Report the outcome kind, returned snapshot source/base where disclosed,
   snapshot id, and `fresh_for_working_tree`. Do not promote a failed
   refresh's last-known-good snapshot to a successful current check.
   An empty scope is not an all-clear; surface `empty_scope_hint`.
5. Finish with one line: setup is working, or the exact step that failed
   with its typed failure code. Do not claim support beyond what these
   calls just demonstrated in this session.
