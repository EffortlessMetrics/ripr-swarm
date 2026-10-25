<!-- section: Fixed -->
- Receiver attribution again credits `new_*`, `from_*` and `with_*` associated functions that the indexed `impl` declares returning `Self` or the type itself, restoring the credit the #8782 fence removed. A factory returning another type, or a name declared with differing return types, still gets none (#10301).
