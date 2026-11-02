<!-- section: Fixed -->
- A `use` import or owner name inside a `macro_rules!` body that is neither invoked nor `#[macro_export]`ed no longer counts as the owner passed as a value, so an unused macro definition cannot withhold a direct weak test's gap under the indirect-owner limit; an invoked or exported macro keeps the limit (#12231).
