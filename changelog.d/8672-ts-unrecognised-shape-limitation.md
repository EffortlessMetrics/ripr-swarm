<!-- section: Fixed -->
- TypeScript (#8672): a weakly exposed finding on a changed line the preview
  adapter does not recognise (for example `sum += a * b`) now says the shape is
  unrecognised, instead of claiming the line "has no statically comparable
  predicate boundary", which read as a wrong predicate claim about arithmetic.
