<!-- section: Fixed -->
- `ripr agent stub` no longer refuses an owner whose parameter is `impl Into<T>` for a plain, non-generic `T`, or `impl AsRef<str>` / `impl AsRef<[u8]>`; the stub declares that local with `T`, `&str` or `&[u8]`. Every other `impl Trait` parameter, including `impl AsRef<Path>` and `impl Into<Vec<u8>>`, is still refused as `parameter_unsupported` (#8361).
