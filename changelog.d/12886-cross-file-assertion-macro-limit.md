<!-- section: Fixed -->
- Rust checks: a workspace assertion macro defined in another file (`#[macro_use] mod`, or `#[macro_export]` named by a top-level `use crate::name;`) that forwards its operands to a strong std assertion now names the macro-wrapped limit instead of an unnamed gap, and credits nothing (#12886).
