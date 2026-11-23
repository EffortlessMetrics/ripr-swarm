<!-- section: Fixed -->
- Python preview no longer reports `exposed` for an edit that only changes the
  literal fallback of `<parameter>.get(<key>, <value>)` when every related test
  call passes a dict that holds the key, so the fallback never runs; it reads
  `weakly_exposed` and asks for a dict without the key (#16203).
