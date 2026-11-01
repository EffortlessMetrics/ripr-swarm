<!-- section: Fixed -->
- Python module-value credit no longer follows a submodule receiver (`import pkg.submod as t` then `t.lookup(...)`) when the owning package `__init__.py` rebinds the submodule name by assignment, `def`, `class` or `import ... as`, or when that initializer star-imports or does not parse; `from . import submod` re-exports still count (#11135).
