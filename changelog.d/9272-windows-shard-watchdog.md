<!-- section: Fixed -->
- The Windows shard workflow runs nextest under a native watchdog: after 44 minutes it records the running processes and force-kills the tree, so a hung shard is intended to upload its log and name the stuck test binary (#9272). The first hang that exercises this path is the witness.
