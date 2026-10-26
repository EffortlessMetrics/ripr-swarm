<!-- section: Fixed -->
- Formatting-only edits no longer seed exposure findings: a statement split or joined across lines with identical tokens, and a line changed only by reindentation, are behavior-neutral and produce no probes, while a token change inside such a rewrap is still analyzed (#9450).
