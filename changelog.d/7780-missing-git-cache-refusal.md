<!-- section: Fixed -->
- Cache roots are no longer admitted when Git is missing from `PATH` but a validated `.git` marker (directory, or gitfile with a `gitdir:` prefix) sits above the cache dir: `tracked_files_under` now fails closed so repository-shipped cache entries are not trusted, while plain directories and extracted archives keep the warm path (#7780).
