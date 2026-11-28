<!-- section: Security -->
- The editor sidecar refuses a `target/ripr/reports` artifact that is not a
  regular file and caps its read, so a checkout that commits one as a link to
  `/dev/zero` or ships a FIFO there no longer exhausts memory or hangs
  diagnostics (#15275).
