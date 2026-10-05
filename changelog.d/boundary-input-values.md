- A changed boundary on a value ripr cannot map to test inputs (a local
  counter such as `count`, `s.len()`, a counted local) now reads
  `infection_unknown` with "Changed boundary input is unresolved" instead of a
  missing equality discriminator with "observed values: unknown" (#6674,
  #6693).
- A computed test argument such as `order_discount(base + 1)` or
  `&[b'f'; 16]` is no longer read as one of its literals; a boundary whose
  compared parameter receives one stays unresolved instead of a missing
  input (#6672).
- A computed boundary operand such as `CURRENT - 2` or `2 + 2` is no longer
  read as the literal it contains, so ripr no longer names a wrong boundary
  value in the repair hint (#6671).
