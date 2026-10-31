<!-- section: Fixed -->
- `ripr init` now writes `enabled = ["rust", "python"]` and/or `"typescript"` when the project's markers show those languages, instead of pinning `["rust"]` and warning that the file it just wrote switches off the Python or TypeScript preview analysis `ripr check` was already running. A build without `lang-python` keeps the Rust-only pin and still says why (#11335).
