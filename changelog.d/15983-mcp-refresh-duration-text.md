<!-- section: Changed -->
- The MCP `ripr_refresh` tool description now says a refresh can take minutes on a large repository, that no other request is answered while it runs, and that after a client timeout the agent should wait and call `ripr_workspace_status` instead of sending `ripr_refresh` again; the interop guide explains why and names the client timeout settings (#15983).
