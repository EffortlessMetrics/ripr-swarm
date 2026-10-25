<!-- section: Fixed -->
- Direct-owner-call receiver binding credits any associated function the indexed inherent `impl` declares returning `Self` or the type itself (`Site::with_langs(..) -> Self`), restoring the credit the #8782 fence removed for `new_*`/`from_*`/`with_*`. A factory returning another type, or a name declared with differing return types, still gets none (#10301).
