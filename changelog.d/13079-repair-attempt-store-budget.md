<!-- section: Fixed -->
- `ripr agent repair --phase before` no longer records an ambiguous (unscorable)
  baseline once earlier retained attempts under `target/ripr/repair-attempts`
  exceed the 64 MiB exact-capture budget; retained attempt artifacts now use the
  bounded identity other ignored files use (#13079).
