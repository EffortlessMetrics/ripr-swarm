<!-- section: Fixed -->
- TypeScript literal rejection equality through an unshadowed Jest/Vitest
  `expect` import observes the error path and no longer credits an unrelated
  normal return value. A pinned reason the changed line does not throw stays
  `broad_error` / weak, so an unexercised error branch is never exposed.
  Oracle text retains the `.rejects` chain (#7315).
