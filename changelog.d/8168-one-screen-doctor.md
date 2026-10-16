<!-- section: Changed -->
- `ripr doctor` default human output now fits one 80x24 screen: failures
  first, each with a `Fix:` line, then compact passing groups. Verbose
  (`-v`/`--verbose`) restores binary identity, config defaults, test
  surfaces, Perl preview, known limitations, and start-here detail.
  `ripr doctor --json` is unchanged (#8168).
