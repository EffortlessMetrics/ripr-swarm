<!-- section: Fixed -->
- The `merge-ready conflict` status posted on a conflicting PR now clears when a
  re-applied `merge-ready` label finds the conflict gone, instead of staying red
  until the next push. The notice workflow clears only its own earlier failure
  on the same head (#9842).
