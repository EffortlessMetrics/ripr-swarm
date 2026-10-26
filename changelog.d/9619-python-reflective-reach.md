<!-- section: Changed -->
- Python: a file that uses reflection (`getattr`, `inspect`, ...) now withholds
  dict-table credit only for modules in its import component, not for every
  table in the workspace. A reflective file that loads modules by a runtime name
  still withholds all table credit (#9619).
