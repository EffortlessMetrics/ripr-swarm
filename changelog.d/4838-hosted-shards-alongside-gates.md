<!-- section: Internal / CI -->
- `.github/workflows/routed-rust.yml` starts the four hosted test shards alongside the hosted gates job instead of after it, so a code qualification takes about the longer of the two phases instead of their sum and fits the merge queue's status-check timeout; `Ripr Rust Small Result` still requires both (#4838).
