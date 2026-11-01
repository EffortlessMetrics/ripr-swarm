<!-- section: Fixed -->
- A predicate on a one-line `if` statement that closes its own block
  (`if x >= 5 { r = 1; } else { r = 1; }`) no longer takes the next line as its
  returned-value sink, so identical-arm assignments followed by `r + 0` stop
  reading `exposed` on a coincident row (#10880).
