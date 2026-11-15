<!-- section: Fixed -->
- `ripr repair <id>` refused for uncommitted source now tells you to rerun `ripr repair <id> --root <root>`, the command you typed, instead of the advanced `ripr agent repair … --phase before` route. Its refusal for a build directory that is not Git-ignored no longer says "then rerun --phase before" next to the `next:` line that names the right command (#14394).
