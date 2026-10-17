<!-- section: Added -->
- `ripr doctor --json` now carries an optional `canonical_next_action` when
  setup needs a precise recovery (missing root, unloadable `ripr.toml`, or
  failed prerequisite checks). It is the same decision the human
  `- Safe next action:` line follows, with `stop.check_name` naming the failing
  check, and is omitted when the state names a runnable command. Additive in
  doctor schema `0.4`; no version bump (#6986).
