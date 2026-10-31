<!-- section: Fixed -->
- Python no longer credits a literal zero-argument `patch` when the test module rebinds `patch` (or `mock` for `mock.patch`) besides the single `unittest.mock` import, or when the patch target's module prefix is not the owner's module, so a `def patch` shadow or `other.mod.time.time` cannot hide an edit around `time.time()` (RIPR-SPEC-0238 rule 8, #10267).
