<!-- section: Fixed -->
- Commands that read a check output or gate decision file (`baseline`,
  `review-comments --check-output`, `first-pr --check-output`, `gate`) accept
  the encodings Windows PowerShell 5.1 writes: UTF-16 with a byte-order mark
  from a `>` redirect, and UTF-8 with a byte-order mark. They were refused as
  invalid UTF-8 or invalid JSON, with recovery text that repeated the
  redirect (#15936).
