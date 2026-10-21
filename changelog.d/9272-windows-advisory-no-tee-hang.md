<!-- section: Internal / CI -->
- The nightly Windows advisory lane no longer hangs until its 90-minute job
  limit after cargo has finished. Its cargo steps wrote logs through
  `| tee`, which waited for every process holding the pipe, so a test that
  leaked a child blocked the step; they now redirect to the log file and
  replay it, and each step has its own timeout with `continue-on-error` so a
  stuck step is reported as missing evidence while later steps still run
  (#9272).
