<!-- section: Fixed -->
- A whole-value assertion on a returned type no longer reads `exposed` in
  two more cases where that type's `Drop` impl exits the process. In both,
  a failing assertion still exits 0 while the returned value drops during
  unwinding.
  - The `Drop` impl is written by a macro in the crate. A macro that
    writes such an impl for a type it receives withholds the pin for
    every type.
  - The type is returned through a `use .. as` rename or a `type` alias,
    and the impl names the original type in another file.

  A macro that writes no `Drop` impl, and a rename of a type whose `Drop`
  is plain, keep the credit. Derive and attribute macros from other
  crates stay unread (#15107).
