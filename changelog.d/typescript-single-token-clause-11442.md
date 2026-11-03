<!-- section: Fixed -->
- TypeScript preview: the changed-boundary selector no longer names a
  comparison clause when two or more tokens were replaced inside one clause or
  when the edit added a comparison clause, so neither starts a repair packet.
  A pure operand swap with the mirrored operator (`total >= 100` to
  `100 <= total`) still names its clause (#11442).
