<!-- section: Fixed -->
- A receiver-qualified call (`config.parse(..)`) no longer credits
  `direct_owner_call` against a free-function owner: method-call syntax can
  never resolve to a free function, so a test whose only same-named call
  sites are receiver-qualified now relates at most `weak_token_substring`.
  Bare (`parse(..)`), path-qualified (`path::parse(..)`), turbofish, and
  range-bound call sites keep the credit, and method owners — including
  trait declarations, which share the free-function id shape — are
  untouched (#7006).
