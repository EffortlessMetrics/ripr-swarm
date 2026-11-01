<!-- section: Fixed -->
- On Windows the language server now rereads the on-disk bytes for every buffer-equals-disk check instead of reusing a cached digest, because std exposes no inode change time and a same-length in-place rewrite that restores the old mtime could not otherwise be told apart from an unchanged file. Unix keeps the cache, keyed on the inode change time (#12046).
