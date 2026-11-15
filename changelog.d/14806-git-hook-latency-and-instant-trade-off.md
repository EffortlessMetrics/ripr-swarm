<!-- section: Changed -->
- `docs/INTEGRATIONS.md` states how long a commit-time hook takes (about
  0.07 s on a small crate, 36 s warm on ripr's own 3,700-file workspace), how
  to add `--mode instant` for large workspaces, and what `instant` misses
  (tests outside the changed files and their parent modules). The lefthook snippet shows the
  Python and TypeScript glob (#14806).
