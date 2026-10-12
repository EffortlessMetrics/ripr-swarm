<!-- section: Fixed -->
- `ripr review-comments` cards keep their redirect targets relative to the
  `--root` they print. A card rendered with `--root .` used to write
  `> /path/on/the/rendering/machine/target/ripr/workflow/agent-brief.json`,
  so a reviewer pasting it from their own checkout analyzed one repository
  and wrote to a directory that only existed on the CI runner. The brief,
  analysis-outcome and verify commands now write
  `> target/ripr/workflow/...` under the same root they analyze. Gate
  decisions and the PR review front panel carry the same commands (#4000).
- `ripr pilot`'s language routes (`ripr check --root ...` for TypeScript,
  JavaScript, Python and Perl files) now name the absolute repository pilot
  analyzed, like pilot's other next commands, instead of repeating the typed
  relative `--root` (#4000).
