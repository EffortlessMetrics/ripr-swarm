<!-- section: Fixed -->
- `ripr check` reads a file's `std::process` imports once instead of once per test function, which halves the warm check of a diff against a file with thousands of tests (this repository's two-xtask-file diff: 64 s to 31 s); findings are byte-identical (#10630).
