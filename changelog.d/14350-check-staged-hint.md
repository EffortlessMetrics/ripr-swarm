<!-- section: Changed -->
- `ripr check --staged` and `--cached` are still refused. The message now says
  that check already reads staged and unstaged changes together, and gives the
  staged-only spelling `git diff --cached | ripr check --diff -` (#14350).
