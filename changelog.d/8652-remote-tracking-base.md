<!-- section: Fixed -->
- `ripr check --base <name>` for a branch that only exists on a remote now
  names the resolvable remote-tracking form
  (`--base refs/remotes/<remote>/<name>`, probed for every configured
  remote) instead of re-prescribing the fetch that already created it
  (#8652).
