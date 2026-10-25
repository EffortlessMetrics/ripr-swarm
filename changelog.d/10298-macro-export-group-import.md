<!-- section: Changed -->
- A grouped `use crate::{a, b};` now resolves each plain `#[macro_export]` test generator member like a single `use crate::name;` import, and the remaining members stay valid. Renamed, nested and glob members still index nothing (#10298).
