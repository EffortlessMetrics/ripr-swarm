<!-- section: Fixed -->
- The repair dirty-premise preflight now resolves git's repo-top-relative dirty paths against the work-tree top level, so a dirty production file under a subdirectory `--root` is refused early and a colliding repo-top path is no longer misreported (#13795).
