<!-- section: Changed -->
- `ripr doctor` default human output now fits one 80x24 screen: failures
  first, each with a `Fix:` line, then compact passing groups. Verbose
  (`-v`/`--verbose`) restores binary identity, config defaults, test
  surfaces, Perl preview, known limitations, and start-here detail.
  Compact still names a refused `[perl].executable` and does not
  recommend `ripr check` after a config, Cargo.toml, or required-runtime
  failure. The enablement tip is one physical line and names `enabled =
  [...]` under `[languages]`. A tool `--version` failure or timeout gets
  a `Fix:` that names repair of the binary on PATH, not a missing-tool
  install. Compact path, evidence, and advisory text spell control
  characters so a newline-bearing root stays within 24 lines; JSON keeps
  the raw path. Verbose config failures keep source-free parser reasons.
  `ripr doctor --json` schema is unchanged (#8168).
  Owner-return pin JSON-import visibility scans each file's `use` items
  once so dogfood `ripr check` on this doctor diff can finish inside the
  PR-evidence ceiling.
