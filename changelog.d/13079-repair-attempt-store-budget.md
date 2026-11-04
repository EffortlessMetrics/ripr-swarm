<!-- section: Fixed -->
- `ripr agent repair --phase before` no longer records an ambiguous (unscorable)
  baseline once earlier retained attempts under `target/ripr/repair-attempts`
  exceed the 64 MiB exact-capture budget; large retained artifacts (over 1 MiB,
  such as `before.repo-exposure.json`) now use the bounded identity other
  ignored files use, while manifests and receipts stay exact (#13079).
