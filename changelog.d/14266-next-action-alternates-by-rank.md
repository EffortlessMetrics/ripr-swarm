<!-- section: Fixed -->
- `ripr check`'s next action now offers the two best-ranked findings it did
  not select as its `considered finding` alternates, using the same priority
  as the `Hidden:` list, instead of the first two in report order, so an
  already-noticed finding no longer displaces a test gap (#14266).
