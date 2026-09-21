---
name: on-linux
description: Linux execution contract. Use when the host is Linux, before running any command: POSIX shell grammar and the pipeline exit-code trap.
---

# Useful result

Commands run correctly on a Linux host the first time, with failures
observed rather than masked by pipes.

# Contract

- Commands run under a POSIX shell with GNU coreutils.
- A pipeline hides the producer's exit code: `cargo test … | grep …`
  reports the exit status of the last stage, so a real failure reads as
  success. Inspect the producer's result lines, or capture
  `${PIPESTATUS[0]}` before the pipe.
- Read only this skill for shell questions on Linux. Windows and macOS
  grammars live in `on-windows` / `on-macos` and do not apply here.
