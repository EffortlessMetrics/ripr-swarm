<!-- section: Fixed -->
- The MCP post-edit re-check route (`verify` on `ripr_refresh` and `after_editing_tests` on a refused `ripr_prepare_repair`) now carries the snapshot's explicit base, so following it re-checks the same changed behavior instead of silently returning to the default base and reporting zero findings (#13196).
