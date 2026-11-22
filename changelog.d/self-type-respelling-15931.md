<!-- section: Fixed -->
- `ripr check` no longer reports a line that only writes `Self` for the
  implementing type's own name, or the reverse (`Shell::Bash` -> `Self::Bash`
  inside `impl .. for Shell`, the `clippy::use_self` cleanup), as changed
  behavior. A different type name, another changed token, macro input, a
  generic or reference self type, a type the scope of the `impl` does not
  itself declare, or an attribute macro on the item keeps its finding
  (#15931).
