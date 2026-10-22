<!-- section: Fixed -->
- `cargo xtask goldens bless` now refuses to bless a fixture whose input
  files have CRLF or mixed working-tree line endings while the git index
  and attributes normalize them to LF, when the expected output embeds
  `input_identity`; `goldens check` warns on the same files instead of
  reporting a clean local pass. Binary (`-text`) inputs never fire, and
  no fixture file is modified by the guard (#8614).
