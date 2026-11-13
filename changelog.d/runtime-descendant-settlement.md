<!-- section: Fixed -->
- Linux owned-process cleanup observes process-group quiescence before reaping its leader. Natural-exit polls and explicit cleanup share one signal-bound settlement deadline and retain incomplete-cleanup failures rather than waiting indefinitely (#14275, #14503).
