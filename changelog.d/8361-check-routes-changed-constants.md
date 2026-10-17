<!-- section: Added -->
- `ripr check` now routes a changed `const` or `static` to the function in the same file that consumes it: it prints that function's `ripr agent stub` command, or the stub producer's refusal, instead of nothing (#8361).
