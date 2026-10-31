<!-- section: Fixed -->
- `ripr repair` on a seam whose owner file has no test module no longer ends at
  "repair route not ready" with no next step: it names the `ripr agent stub`
  command, which adds a module for an owner at the top of the file (#11330).
