<!-- section: Changed -->
- `ripr doctor` default human output now fits one 80x24 screen: failures
  first, each with a `Fix:` line, then compact passing groups. Verbose
  (`-v`/`--verbose`) restores binary identity, config defaults, test
  surfaces, Perl preview, known limitations, and start-here detail.
  Compact still names a refused `[perl].executable` and does not
  recommend `ripr check` after a config, Cargo.toml, or required-runtime
  failure. `ripr doctor --json` is unchanged (#8168).
