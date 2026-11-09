<!-- section: Fixed -->
- TypeScript tests that call several members of a default-exported object (`utils.merge` beside
  `utils.extend`) now credit each called member's owner instead of dropping all of them to a
  name-match link; any non-call use of the default import still withholds every member, and so does calling a sibling whose body could
  replace the member (`this`, `require`, `exports`, a self-import, an imported or reassigned sibling, ...) (#6653).
