<!-- section: Fixed -->
- Shared Git diffs use the platform's null device for `diff.orderFile`, so
  Windows Git can ignore a dangling orderfile without relying on a Unix path
  alias (#16058).
