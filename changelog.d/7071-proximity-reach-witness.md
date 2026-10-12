<!-- section: Fixed -->
- A change that tests reach only through a macro or a helper chain no
  longer reads as an actionable gap when the only related tests share
  the owner's file or a name token. ripr now runs the same transitive
  and macro reach witnesses it already runs for `no_static_path`
  findings and names the limit, such as
  `rust_macro_reach_unresolved`. The finding stays `weakly_exposed`, and
  the next step points at the witnessing test (#7071).
