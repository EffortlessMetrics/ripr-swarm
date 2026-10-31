<!-- section: Fixed -->
- `ripr reports ci-packet` treats an empty `RIPR_COMMENT_MODE` as `off`, as its `--comment-mode` help and `ci-summary` already do, instead of running the "Plan RIPR inline comments" stage and failing with `unknown pr-comments plan mode ""`; `ripr pr-comments plan --mode ""` now says the mode is empty and how to publish nothing (#11146).
