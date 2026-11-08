<!-- section: Fixed -->
- `ripr check` no longer lists gitignored dependency and output directories (`node_modules`, `.venv`/`venv`, `vendor`, root `dist` and `build`) in the "Gitignored files" note or the `ignored_working_tree_source_paths` JSON list, and no longer suggests `git add -f -N` for them. Gitignored sources you wrote are still listed (#13597).
