<!-- section: Fixed -->
- The sharded Windows workflow's failure summary strips nextest's ANSI color codes before filtering, so FAIL/LEAK rows are no longer dropped from the step summary; a regression test runs the checked-in step against a colored log (#9272).
