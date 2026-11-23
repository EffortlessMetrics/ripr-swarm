<!-- section: Fixed -->
- `ripr check --diff` reads a UTF-16 diff that opens with a byte-order mark,
  the file Windows PowerShell 5.1 writes for `git diff > change.diff`. It was
  refused as "no parseable file changes". UTF-16 that is truncated or holds
  an unpaired surrogate is refused with the encoding named and
  `git diff --output=<file>` as the fix (#15907).
