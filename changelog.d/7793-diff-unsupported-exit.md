<!-- section: Fixed -->
- `ripr diff` now exits 2 for typed `unsupported_input` after emitting its
  original report once, matching `ripr check`. Complete and usable partial
  reports remain advisory exit 0; JSON discovery names the mapping (#7793).
