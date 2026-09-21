---
name: on-windows
description: Windows execution contract. Use when the host is Windows, before running any command: PowerShell grammar, available and forbidden tools, and how to read cargo output through harness noise.
---

# Useful result

Commands run correctly on a Windows host the first time, with no Unix-ism
retries and no misread build results.

# Contract

- Commands run under PowerShell. Use PowerShell syntax and environment
  variables (`$env:NAME`), never Bash or `cmd.exe` syntax.
- Unix coreutils do not exist: no `touch`, `sed`, `awk`, `head`, `grep -r`.
  Use `Get-Content`, `Select-String`, and `Select-Object` instead.
  `Select-String` has no `-Recurse` flag; scope paths explicitly.
- Cargo writes build progress to stderr, and some harnesses surface stderr
  text as failure. A harness-reported failure alongside a `Finished` /
  `test result: ok` line is noise, not a build failure: read the `error`,
  `warning`, `test result`, and `Finished` lines before diagnosing.
- Read only this skill for shell questions on Windows. Linux and macOS
  grammars live in `on-linux` / `on-macos` and do not apply here.
