<!-- section: Fixed -->
- CI: the `Hard-cut main watch` is its own workflow with a per-commit concurrency group. As a job inside `routed-rust.yml` it never ran on a busy main, because each newer push cancelled the earlier run before any job started (#5391).
