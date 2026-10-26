<!-- section: Changed -->
- TypeScript: reading the members picked off a `require()` namespace now scans
  the file for writes once per file (on the first pick) instead of cutting and
  rescanning it per pick; identifier occurrences are still read per pick.
  Verdicts are unchanged (#10646).
