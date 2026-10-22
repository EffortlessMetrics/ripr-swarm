<!-- section: Internal / CI -->
- The First-run install workflow gains an advisory `first-run-main` job that walks the same pinned crates with a ripr built from the checked-out commit and writes each crate's verdict reading to the job summary, so a fix to the first-run verdicts is visible before the next release (#9276). The published-release lane and its baseline are unchanged.
