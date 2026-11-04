<!-- section: Changed -->
- `output` no longer imports the CLI for unknown-value wording or `first-pr` argv: the near-miss suggestion authority moved to a shared `accepted_value` module that `cli::suggest` reuses, and `first-pr` argument parsing and help now live in the CLI adapter. Command output is unchanged (#5193).
