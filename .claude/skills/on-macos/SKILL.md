---
name: on-macos
description: macOS execution deltas. Use when the host is macOS, after on-linux: only what differs from Linux (BSD tools, clipboard, no procfs).
---

# Useful result

macOS commands use BSD-correct forms instead of failing on GNU assumptions.

# Contract

- Start from `on-linux`; this skill holds only deltas.
- BSD `sed -i` requires a backup argument (`sed -i '' …`); GNU-style
  `sed -i` without one fails.
- `pbcopy` / `pbpaste` replace `xclip`-style clipboard tools.
- There is no `/proc`; do not probe it for process state.
- Read only this skill plus `on-linux` for shell questions on macOS.
  Windows grammar lives in `on-windows` and does not apply here.
