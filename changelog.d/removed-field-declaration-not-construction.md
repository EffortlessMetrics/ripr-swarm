<!-- section: Fixed -->
- A deleted struct field or enum-variant field declaration (one-line or
  multi-line variant) with no replacement is now one `static_unknown` probe, not
  a removed-side `field_construction`. The decision is made on the reconstructed
  old source with the same exact-line, single-owner rule as the added side; a
  removed body constructor, discriminant, macro-typed field or shared line keeps
  its probe, and an edited field keeps its before/after pair (#9183).
