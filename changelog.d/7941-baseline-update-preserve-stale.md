<!-- section: Fixed -->
- `ripr baseline update --remove-resolved` keeps unmatched baseline entries
  whose absence is an analyzer-build discontinuity (differing or one-missing
  `subject.analyzer_version` stamps), including reviewed metadata, instead of
  shrinking them as resolved debt. Same-build genuine absence still shrinks
  (#7941).
