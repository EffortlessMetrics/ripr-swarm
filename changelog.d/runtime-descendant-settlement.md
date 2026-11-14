<!-- section: Fixed -->
- Linux owned-process cleanup observes process-group quiescence before reaping its leader. Natural-exit polls and explicit cleanup share one signal-bound settlement deadline and retain incomplete-cleanup failures rather than waiting indefinitely (#14275, #14503).
- Linux owned-process cleanup treats the kernel's unavailable process-group sentinel during teardown as pending observation, rather than a numeric parse failure or proof of quiescence (#14503).
