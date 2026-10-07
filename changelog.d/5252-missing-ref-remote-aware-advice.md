<!-- section: Fixed -->
- The missing-base/head repair no longer prescribes `git fetch origin`:
  it reads the repo's own remote configuration. A remote-less repo gets
  the add-remote step, a remote-qualified ref (`origin/main`) names its
  own remote when that remote is configured (even when the branch tracks
  elsewhere) and the add-remote step when it is not, one configured
  remote is named, and several remotes (or an unanswerable probe) fall
  back to plain `git fetch`, which follows the default remote (#5252).
