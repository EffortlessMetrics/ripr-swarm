<!-- section: Fixed -->
- `ripr check --format agent` and `--json` on a clean base branch now name the last-commit rerun (`ripr check --base HEAD~1`), as the human view does, instead of only "pass --base REV"; JSON carries it as `canonical_next_action.stop.scope_route` (#14423).
