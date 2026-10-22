<!-- section: Fixed -->
- LSP: the stage reports held behind a work-done progress begin (#9362) are
  now sent within the client request liveness bound, so an editor that stops
  reading right after the begin can no longer keep the progress token from
  ending.
