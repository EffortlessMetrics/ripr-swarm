<!-- section: Internal / CI -->
- `Ripr Rust Small Result` now fails a `merge-ready` request when the VS Code extension e2e ran for the admitted head and did not succeed, and waits up to 20 minutes for one still running; a head where the e2e was not selected is unaffected (#12122).
