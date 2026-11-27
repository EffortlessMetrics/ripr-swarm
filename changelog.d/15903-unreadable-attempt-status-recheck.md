<!-- section: Fixed -->
- `ripr agent status --attempt` for an attempt it cannot read (a missing
  before snapshot, a broken manifest) now says the attempt cannot be finished
  or receipted and names `ripr check --root <root>` as the way to read the
  current state, instead of ending with no next step (#15903).
