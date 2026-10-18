<!-- section: Fixed -->
- `ripr agent repair --phase before` and `ripr repair` now refuse a `--root`
  that is a subdirectory of the Git checkout (a workspace member, for
  example) before writing any workflow artifact or lock under it, and the
  refusal names the top-level to pass as `--root`.
