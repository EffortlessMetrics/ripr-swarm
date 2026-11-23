<!-- section: Docs -->
- `docs/EXIT_CODES.md` no longer says `ripr check` exits 2 on `exposed` or
  `weakly_exposed` findings: `check` is advisory and the blocking exit
  belongs to `ripr gate evaluate`. `docs/CONFIGURATION.md` describes how
  the base is resolved when `--base` is absent and documents the
  TypeScript and Perl settings (#3881, #2737).
