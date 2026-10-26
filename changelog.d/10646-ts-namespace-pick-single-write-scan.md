<!-- section: Changed -->
- TypeScript: reading the members picked off a `require()` namespace now scans
  the file for writes once instead of cutting and rescanning it per pick, so a
  generated test file with dozens of picks no longer costs several whole-file
  scans per pick. Verdicts are unchanged (#10646).
