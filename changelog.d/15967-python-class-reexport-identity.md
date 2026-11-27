<!-- section: Fixed -->
- Python: a test that imports a same-named class from another module no longer
  reads `exposed` for the owner's method; the class must come from the owner's
  module or a package `__init__.py` that re-exports it. A literal-instance
  fixture now resolves through such a re-export and beside standard-library or
  fixture-free workspace imports (#15967).
