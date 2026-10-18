<!-- section: Fixed -->
- A changed `impl` header line (`impl Tr for K where Self: Sized {`) no longer
  seeds a probe. Its `Self: Sized` text was read as a constructed field and
  reported as a `no_static_path` gap that does not exist. A one-line `impl`
  holding a `fn`, the `fn` bodies inside an `impl`, and function `where` bound
  lines (still reported as `static_unknown`) keep their probes (#8389).
