<!-- section: Added -->
- `ripr repair` and `ripr continue` accept `--json`. A repair start prints the
  agent packet with `repair_attempt.attempt_id` on stdout, a repeated start or
  an already-complete continue prints the attempt's status document, and
  `continue` prints its after-phase document. Refusals and argv usage errors
  keep their exit codes, stderr prose and empty stdout (#11584).
