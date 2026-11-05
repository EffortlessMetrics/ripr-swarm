<!-- section: Internal / CI -->
- The relative-root cache export test holds the shared cwd lock, so parallel tests that change the working directory can no longer make its root disappear (#12989).
