<!-- section: Fixed -->
- A repository with a corrupt `.git/packed-refs` is no longer reported as having no commits yet; `ripr check` without `--base` now names Git's own error and the file to correct or restore (#10566).
