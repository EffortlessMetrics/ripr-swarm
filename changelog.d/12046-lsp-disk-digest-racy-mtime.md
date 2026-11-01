<!-- section: Fixed -->
- The language server no longer judges a same-length rewrite equal to the previous file contents: on Unix the cached on-disk digest is keyed on the inode change time too, so a rewrite that restores the old mtime is seen, and a file whose modification time is within the last two seconds is not cached. This assumes filesystem timestamps no coarser than that window (#12046).
