<!-- section: Fixed -->
- A predicate whose old side was split across lines (`if a` / `&& b` / `{`)
  is now compared against its whole old `&&`/`||` chain, so an edit that drops
  or adds an operand no longer reads `exposed` from an input at the surviving
  comparison's boundary (#7945).
