<!-- section: Fixed -->
- `ripr agent brief --diff` and `ripr agent verify --before/--after` failures
  for a missing or unreadable file no longer expose the internal
  `canonicalize` operation; they name the unreadable input and the recovery
  route, matching `explain --diff`'s user vocabulary (#9855).
