<!-- section: Fixed -->
- A managed Perl exporter that dies by SIGINT during an interrupted
  `ripr check` is now reported as the interruption, not as an exporter
  failure ("exited with status ... other languages continue"). The decision
  waits a bounded moment for the CLI's own interrupt flag instead of
  depending on which process observed the group signal first (#12935).
