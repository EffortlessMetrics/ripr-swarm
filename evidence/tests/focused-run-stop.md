# Focused run stop record

The post-commit focused script recorded exit 0 for reveal, transitive, integration, and CLI rerun. It then started `cargo xtask goldens check`; the command had no final exit receipt when the user directed that local commands be stopped. The runner and its active Rust compiler were terminated. Do not treat the partial `goldens.log` as a passing result. Exact-head honesty and dogfood were not run.
