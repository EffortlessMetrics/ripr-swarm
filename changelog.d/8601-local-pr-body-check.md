<!-- section: Internal / CI -->
- CI: `cargo xtask check-pr-shape` now validates a supplied PR body offline
  (`--body-file` / `--body-env`, raw Markdown or `gh pr view --json body`
  JSON) and fails naming each missing or placeholder
  `pull_request_template.md` Source-of-truth Links / Candidate Relationships
  line before first Ready; agent skill guidance requires those lines verbatim
  with real values (#8601).
