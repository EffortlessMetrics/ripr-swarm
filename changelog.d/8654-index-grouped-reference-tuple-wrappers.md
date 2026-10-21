<!-- section: Fixed -->
- A test held behind an index read (`vec![x == 1, y][0]`), a parenthesized or
  `&`-prefixed wrapper (`(Wrapper(x == 1)).0`, `&Foo { ok: x == 1 }`) or a
  bare tuple (`(x == 1, y).0`) and compared with a non-literal no longer earns
  exact/strong credit; two tuples pair their elements like two lists, and a
  method compared with `true` caps like a field read of a wrapper holding
  other elements (RIPR-SPEC-0231 rule 1; #8654).
