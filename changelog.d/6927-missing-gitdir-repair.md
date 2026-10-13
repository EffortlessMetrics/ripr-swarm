<!-- section: Fixed -->
- A checkout whose `.git` is a gitfile pointing at a vanished gitdir (a deleted worktree admin dir, or a submodule whose `.git/modules/<name>` was removed) is refused with `git worktree repair` / `git submodule update --init`, not the generic "not inside a Git work tree" message (#6927).
