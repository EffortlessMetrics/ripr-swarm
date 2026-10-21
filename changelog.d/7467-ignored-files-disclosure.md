<!-- section: Fixed -->
- `ripr check` names gitignored Rust source files it excludes (an ignored
  file, or files under an ignored directory such as `target/`) in the human,
  human-full, GitHub, and JSON (`ignored_working_tree_source_paths`) output
  with the force intent-to-add repair, instead of dropping them silently (#7467).
