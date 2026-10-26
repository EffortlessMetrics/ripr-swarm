<!-- section: Internal / CI -->
- Main's strict Clippy gate failed on `parent_dir_follows_symlink` being unused in the `xtask` build that path-includes `agent/loop_commands.rs`; the helper now lives beside its only caller in the `agent status` command (#10624 follow-up).
