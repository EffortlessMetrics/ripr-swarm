<!-- section: Fixed -->
- A changed `enum`, `struct`, `union` or `trait` header line that opens its body
  (`enum Operator<P, Q: Ord + Copy> {`) is no longer read as a constructed
  field. The generic bound looked like a field `name: Type`, so the header was
  reported as a `field_construction` probe with a `weakly_exposed` or
  `no_static_path` warning. It is now one `static_unknown` probe, like an `impl`
  header (#9170).
