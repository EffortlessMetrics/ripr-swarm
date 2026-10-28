<!-- section: Fixed -->
- `ripr agent stub` no longer refuses an owner whose parameter is `impl Into<T>` or `impl AsRef<str>`; the stub declares that local with the concrete `T` or `&str` the call accepts, and every other `impl Trait` parameter is still refused as `parameter_unsupported` (#8361).
