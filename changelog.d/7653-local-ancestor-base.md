<!-- section: Fixed -->
- `ripr check --base HEAD~99` (and other local ancestor expressions such as
  `main^2`) no longer tells the user to add or fetch a remote. When the
  ancestry base already resolves and the clone is not shallow, the refusal
  says how many commits that base reaches. Shallow clones still say
  `git fetch --unshallow`, and remote-qualified refs such as `origin/main`
  still name their remote (#7653).
