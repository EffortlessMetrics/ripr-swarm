<!-- section: Fixed -->
- `ripr check` reads a file's `std::process` imports once instead of once per test function, which halves the warm check of a diff against a file with over a thousand tests (this repository's two-xtask-file diff, whose test file has 1,225: 64 s to 31 s); findings are byte-identical (#10630).
