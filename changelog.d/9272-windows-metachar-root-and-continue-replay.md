<!-- section: Internal / CI -->
- Five Windows test expectations now match what the host prints: the hostile
  metacharacter-root test uses a legal file name, the repeated-repair test
  decodes the printed `ripr continue` command in the host's quoting form, the
  missing-snapshot capture hints expect the native path spelling, and the
  gap-bound status inventory check matches the JSON-escaped quoted base (#9272).
