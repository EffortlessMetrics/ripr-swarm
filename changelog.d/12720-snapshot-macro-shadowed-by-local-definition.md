<!-- section: Fixed -->
- A bare `assert_snapshot!`-style call in a Rust test now counts as `insta`'s only when no file in the
  workspace defines a macro of that name. A `#[macro_use]` definition in another file shadows the `use insta`
  import, so a missing `.snap` no longer demotes a seam asserted through that local macro (#12720).
