<!-- section: Changed -->
- `ripr check` and editor saves no longer re-parse a whole file each time a
  refusal asks which `#[cfg]` gates guard the same function; the answer is
  reused within one analysis run (#13754).
