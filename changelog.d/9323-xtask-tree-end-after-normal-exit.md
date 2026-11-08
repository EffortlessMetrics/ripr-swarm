<!-- section: Internal / CI -->
- After a normal primary exit, xtask `capture_output_sampled` ends the remaining owned process tree before draining pipes, so a Windows Job Object descendant cannot hold the capture for the post-exit grace; Unix still uses the bounded truncation diagnostic when a post-reap group cannot be signalled (#9323).
