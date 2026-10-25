<!-- section: Fixed -->
- TypeScript/JavaScript preview: a one-line guard such as `if (total >= 100) return total - 10;` now reads its comparison as the predicate boundary, like the braced form and like Rust, so the finding names the missing `total == 100` input instead of "no statically comparable predicate boundary" with a generic exact-value-assertion step (#10463).
