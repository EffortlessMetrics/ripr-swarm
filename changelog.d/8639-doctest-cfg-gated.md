<!-- section: Fixed -->
- Doc examples on items gated by `#[cfg(..)]`, `#[cfg_attr(..)]`, or an inner `#![cfg(..)]` no longer register as tests unless the only cfg between the comment and the item is `#[cfg(doctest)]`, which rustdoc enables when running doctests (#8639).
