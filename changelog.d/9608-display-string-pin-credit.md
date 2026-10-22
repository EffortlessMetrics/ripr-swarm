<!-- section: Fixed -->
- A `Display::fmt` that only calls `write!(f, "..", args)` and is pinned by an
  exact `value.to_string()` assertion against a string literal now reads
  `exposed` instead of `weakly_exposed` (#9608). Weaker assertions, `format!`
  sides, computed expectations and `Debug` owners keep their prior verdict.
