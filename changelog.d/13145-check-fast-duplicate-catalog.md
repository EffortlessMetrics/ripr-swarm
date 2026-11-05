<!-- section: Internal / CI -->
- `cargo xtask check-command-catalog` passes again: #13202 registered
  `check-fast` twice in `known_commands()` and the command catalog; the
  duplicate entries (after `check-pr`) are removed, keeping the original
  registration (#13145).
