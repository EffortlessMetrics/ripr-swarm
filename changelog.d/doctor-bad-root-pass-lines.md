<!-- section: Fixed -->
- `ripr doctor --root <missing or non-directory>` no longer prints a passing `✓ Cache:` line or a `Config: not found; using built-in defaults` entry under `✓ Workspace:` for a root that cannot hold either; the cache check is reported as skipped with the root failure as the reason (follow-up to #5252, #9922).
