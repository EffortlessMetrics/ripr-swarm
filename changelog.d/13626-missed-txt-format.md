<!-- section: Added -->
- `ripr check --format missed-txt` prints one sorted `path:line:col: <classification> <probe family>: <expression> -> <missing discriminator>` line per candidate-actionable, unsuppressed, non-exposed finding and nothing for exposed findings, using the shared finding owner and static vocabulary only (#13626)
