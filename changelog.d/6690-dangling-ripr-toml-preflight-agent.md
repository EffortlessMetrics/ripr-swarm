<!-- section: Fixed -->
- `ripr first-pr` preflight and agent-receipt config fingerprinting treat a
  present but unreadable `ripr.toml` (dangling symlink or unreadable lookup)
  as unreadable, not as built-in defaults (#6690).
