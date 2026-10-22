<!-- section: Internal / CI -->
- `cargo xtask precommit` (and so the Docs Gate that runs it) now runs `check-doc-roles`, so missing required proposal/ADR headings fail the shared docs gate instead of only the standalone command (#8009).
