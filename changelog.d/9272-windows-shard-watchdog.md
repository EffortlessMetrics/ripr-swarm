<!-- section: Fixed -->
- The Windows shard workflow runs nextest under a watchdog scoped to its own process tree: after 44 minutes it records that subtree and force-kills it by native PID, or, when the PID cannot be mapped, records that and kills only the MSYS job, intended to leave a log and a status to upload (#9272). Not yet demonstrated on a real hang.
