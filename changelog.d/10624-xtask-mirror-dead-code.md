<!-- section: Fixed -->
- The strict hosted Clippy gate is green again: the `#[path]` mirror that compiles ripr's `agent::loop_commands` inside xtask now carries a reviewed `dead_code` allowance, since helpers consumed only by ripr proper (for example `parent_dir_follows_symlink`) are dead in the mirror compilation by design (#10624).
