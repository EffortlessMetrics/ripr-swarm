<!-- section: Changed -->
- A top-level `crate::name!` call to a `#[macro_export]` test generator now registers its tests without a `use`, when exactly one macro of that name exists in the file's crate. A bare `name!` call with no import, a nested `crate::name!` and other paths still index nothing (#10298).
