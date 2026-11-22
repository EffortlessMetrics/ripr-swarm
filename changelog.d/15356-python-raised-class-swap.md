<!-- section: Fixed -->
- Python preview: a sole-line raised-type swap to a class imported with a plain
  `from M import Name` (`raise ValueError(f"...")` -> `raise InvalidVersion(f"...")`)
  no longer abstains as `missing_import_graph` ("changed line calls an imported
  symbol"); it reads as a `weakly_exposed` error-path gap naming the new class.
  A `pytest.raises(..., match=...)` or `assertRaisesRegex` of a base class,
  `Exception` or a tuple holding one no longer reads `exposed` for a raised-class
  swap: only the exact new class, resolved to the owner's own class, or the old
  builtin class that the new class provably does not derive from, credits it
  (#15356).
