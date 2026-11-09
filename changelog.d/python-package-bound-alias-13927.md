<!-- section: Fixed -->
- Python: a test that writes `from . import tomllib`, where its test package's `__init__.py` binds `import tomli as tomllib`, now links to the code exactly as `import tomli as tomllib` does, instead of every changed line reading "no test reaches this" (#13927).
