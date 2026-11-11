<!-- section: Fixed -->
- `ripr check` in a bare repository now says the root is not a Git work tree and names what to run instead, where it used to print a raw `git status` failure. The skipped-submodule notice now says to run `ripr check` from inside the submodule to check its changes (#14104).
