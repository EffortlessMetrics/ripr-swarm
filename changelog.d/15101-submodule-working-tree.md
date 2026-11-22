<!-- section: Security -->
- ripr's git calls no longer examine a populated submodule's working tree.
  Every `status` and `diff` call passes `--ignore-submodules=dirty`, so no
  `git status` child runs inside a submodule and programs named by the
  submodule's own configuration (for example a `filter.<driver>.clean`) do not
  run during `check`, `diff` or `pilot`. A moved submodule commit is still
  disclosed as a skipped pointer change; uncommitted edits inside a submodule
  are no longer reported (#15101).
