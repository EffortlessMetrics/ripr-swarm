<!-- section: Fixed -->
- Rust diff analysis no longer credits an unchanged orphan test file merely
  because a comment or string mentions its module declaration. Real module
  declarations and unresolved module walks retain their existing behavior (#6979).
