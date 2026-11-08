<!-- section: Internal / CI -->
- The sharded Windows lane's real-ACL step now runs `--workspace --exclude xtask` and then `-p xtask`, the same package selections the shard step built, instead of one `--workspace` run that rebuilt the workspace for about 17 of the step's 20 minutes (#12659).
