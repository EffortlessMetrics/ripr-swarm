<!-- section: Fixed -->
- The sharded Windows advisory lane streams each shard's nextest log to the step output as it is written, and its Summarize and Upload steps now run even when the job is cancelled at its cap, so a shard that wedges leaves its SLOW rows and the log so far instead of nothing. Pass and fail semantics are unchanged (#12092).
