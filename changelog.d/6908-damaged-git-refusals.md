<!-- section: Fixed -->
- A damaged Git repository is refused in Git's own words with a repair route: a bad `.git/config` no longer reads as "not inside a Git work tree", a corrupt `packed-refs` no longer reads as a missing remote, and an unborn or broken `HEAD` or a corrupt object store names `git rev-parse HEAD` or `git fsck` (#6908).
