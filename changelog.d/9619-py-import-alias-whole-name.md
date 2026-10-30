<!-- section: Fixed -->
- Python: an import is matched as a whole name when deciding `missing_import_graph`, so a changed line such as `pos.x + 2` is no longer treated as a use of `import os`; `py-spec0233-ex27-module-imports-os` now credits (#9619).
