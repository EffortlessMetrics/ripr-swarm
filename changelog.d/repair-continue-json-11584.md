<!-- section: Added -->
- `ripr repair` and `ripr continue` accept `--json`. A repair start prints the
  agent packet with `repair_attempt.attempt_id` on stdout, a repeated start or
  an already-complete continue prints the attempt's status document, and
  `continue` prints its after-phase document. Under `continue --json` a typed
  after-phase refusal or failure also prints its document on stdout
  (`repair_after_refusal`, `repair_after_failure`, or the verify document)
  with the same exit code and stderr prose. Selection refusals and the
  `repair` refusals keep their exit codes, stderr prose and empty stdout.
  An unknown flag under `--json` keeps exit 2 and its stderr prose and
  writes the standalone `cli_refusal` document to stdout (#11584).
