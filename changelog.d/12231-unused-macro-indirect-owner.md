<!-- section: Fixed -->
- A `use` import or owner name inside a `macro_rules!` body that is never named elsewhere and not `#[macro_export]`ed no longer counts as the owner passed as a value, so an unused macro definition cannot withhold a direct weak test's gap under the indirect-owner limit; a macro that is invoked, forwarded, named in a comment or exported keeps the limit (#12231).
