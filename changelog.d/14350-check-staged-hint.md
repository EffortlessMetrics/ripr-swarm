<!-- section: Changed -->
- `ripr check --staged` and `--cached` are still refused. The message now says
  that check already reads staged and unstaged changes together, and gives
  `git diff --cached | ripr check --diff -` to limit findings to the staged
  lines, noting that source is still read from the working tree (#14350).
