<!-- section: Fixed -->
- LSP code-action, workspace/configuration, transport-bound and file-URI errors and MCP startup/argument errors now tell the editor or agent client what to do next (refresh diagnostics, resend the payload as returned, restart `ripr mcp --stdio`, pass --root) instead of only naming the failure; error codes and kinds are unchanged (#11539).
