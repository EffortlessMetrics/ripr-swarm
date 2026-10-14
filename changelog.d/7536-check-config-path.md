<!-- section: Added -->
- `ripr check` names the `ripr.toml` that governed the run: the human header prints `config: <path>` after `root:` and check JSON carries top-level `config_path`, each exactly when a worktree config file was loaded. Built-in defaults stay an explicit absence, and candidate-tree runs keep their tree-config provenance in `identity.config_identity` (#7536).
