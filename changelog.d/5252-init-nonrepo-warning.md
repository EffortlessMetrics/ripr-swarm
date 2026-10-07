<!-- section: Fixed -->
- `ripr init` warns when the target directory is not inside a Git work tree
  instead of silently writing branch-based configuration: the generated
  workflow and Next steps assume a branch to compare, which does not exist
  there. The warning is advisory (files are still written, exit stays 0).
  Only an established outside is named as one; a refused repository keeps
  its `safe.directory` repair, and a probe that never answers says Git
  could not confirm the state (#5252).
