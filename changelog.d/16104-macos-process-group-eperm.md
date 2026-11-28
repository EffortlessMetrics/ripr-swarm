<!-- section: Fixed -->
- On macOS, ripr no longer reports a contained child process as an incomplete
  tree cleanup after it exits on its own. Darwin answers a signal to a process
  group whose only member is the exited, not-yet-reaped child with
  `Operation not permitted` rather than `No such process`; ripr now reads that
  answer as a settled group, but only once it has observed the child exit.
  Limitation: Darwin gives the same answer when the only live members left
  changed to a user id ripr may not signal (for example a descendant that
  switched to another account); such a group is reported settled although
  that member may still run, and no signal from ripr could stop it (#16104).
- `ripr pilot` announces its extension even when one wait on a loaded host
  overshoots both the default budget and the extension; it previously timed
  out without the notice (#16104).
