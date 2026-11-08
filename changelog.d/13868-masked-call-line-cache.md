<!-- section: Fixed -->
- Scoped `ripr check` no longer re-masks a whole test file for every
  helper-entry call line: the masked source is built once per file per
  classification run and only the requested line is read back (#13868).
