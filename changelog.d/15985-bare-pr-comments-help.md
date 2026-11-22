<!-- section: Changed -->
- Bare `ripr pr-comments` now prints the group help and writes nothing, like
  the other command groups. It used to run `plan` and write
  `target/ripr/review/comment-publish-plan.json` and `.md`; run
  `ripr pr-comments plan` for that (#15985).
