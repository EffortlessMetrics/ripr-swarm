<!-- section: Fixed -->
- `ripr pilot --root <dir>` on a directory with no `Cargo.toml` and no source files now stops with an actionable message and writes nothing, instead of printing "RIPR pilot complete" and creating `target/ripr/` there (#9922).
