<!-- section: Fixed -->
- CI docs gate: a docs-only PR that changes `README.md`, a top-level
  `docs/*.md`, `docs/interop/*.md`, `docs/status/*.md`,
  `docs/how-to/first-rust-test.md` or `docs/ci/coverage.md` now runs the test
  binaries that pin them (`cli_reference_doc`, `troubleshooting_guide`,
  `user_journeys`, `documented_commands`, `generated_review_workflow` in
  `ripr`, and `future_clippy` in
  `repo-policy`), so a broken asserted phrase fails a required check instead of
  reaching main with every Rust lane skipped (#14998). Other docs-only PRs do
  not pay for those test builds.
