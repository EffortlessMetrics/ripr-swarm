<!-- section: Fixed -->
- `ripr init --help` now describes the generated GitHub workflow accurately: it runs `ripr reports ci-packet` for the PR evidence packet and optionally renders inline review comments via `RIPR_COMMENT_MODE=inline`, instead of promising future test-guidance annotations that are not emitted (#9374).
