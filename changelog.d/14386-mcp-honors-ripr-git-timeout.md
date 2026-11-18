<!-- section: Fixed -->
- `ripr mcp` refreshes now honor `RIPR_GIT_TIMEOUT` from the server's
  environment with the same parser and 300-second default as `ripr check`;
  an invalid value fails the refresh with `config_invalid` instead of being
  ignored, and the git-timeout repair guidance names the MCP route (#14386).
