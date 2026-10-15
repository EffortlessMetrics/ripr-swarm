# check-public-api

Status: pass

## Why This Matters

The crate is the published product surface, so accidental public exports create compatibility expectations. This gate records every module-level `pub` item reachable from the crate root through `pub mod`, including items declared outside lib.rs (#3052). A `#[macro_export]` macro is recorded as `pub macro ripr::<name>` even when its declaring module is private, because Rust exports it at the crate root regardless of that module's visibility. It does not cover public struct fields, enum variants, trait items, or associated functions in `impl` blocks, and it does not resolve names: a `pub use` is recorded as the name it binds, and a glob re-export is recorded as a glob because a syntax walk cannot expand it.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-public-api
```
