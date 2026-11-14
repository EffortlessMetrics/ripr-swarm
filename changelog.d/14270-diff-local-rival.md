<!-- section: Fixed -->
- `ripr check`: stop crediting a test that reaches a changed free function
  only through a test-local rival module (`mod local` plus a bare or glob
  `use`) as a direct call of the production owner; the relation now demotes
  to `weak_token_substring` with no reach, matching the repo-exposure grip
  path (#14270).
