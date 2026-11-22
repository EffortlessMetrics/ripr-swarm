<!-- section: Fixed -->
- On macOS, ripr no longer reports a contained child process as an incomplete
  tree cleanup after it exits on its own. Darwin answers a signal to a process
  group whose only member is the exited, not-yet-reaped child with
  `Operation not permitted` rather than `No such process`; ripr now reads that
  answer as a settled group, but only once it has observed the child exit.
  Native macOS library tests that create non-UTF-8 file names, which APFS
  refuses, are now compiled out on Apple hosts with the reason stated, and the
  resource-cost tests treat macOS as the documented unsupported host (#16104).
