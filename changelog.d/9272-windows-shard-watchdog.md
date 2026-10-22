<!-- section: Fixed -->
- The Windows shard workflow runs nextest under a watchdog scoped to its own process tree: after 44 minutes it records that subtree and force-kills it by PID, intended to leave a log and a status to upload (#9272). Not yet proven on a real hang.
