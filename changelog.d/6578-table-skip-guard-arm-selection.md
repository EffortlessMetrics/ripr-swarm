<!-- section: Fixed -->
- A Rust test whose table loop opens with `if SKIPPED.contains(&key) { continue; }` over a literal `const` list now lends the rows the guard lets through to a match-arm selection, so an asserted row that is not on the list is credited while a listed row stays a gap (RIPR-SPEC-0229, #6578).
