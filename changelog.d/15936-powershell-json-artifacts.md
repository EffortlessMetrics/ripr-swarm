<!-- section: Fixed -->
- `baseline`, `review-comments --check-output`, the JSON inputs of
  `first-pr`, and every JSON input of `gate evaluate` (such as
  `--pr-guidance`) accept
  the encodings Windows PowerShell 5.1 writes: UTF-16 with a byte-order mark
  from a `>` redirect, and UTF-8 with a byte-order mark. They were refused as
  invalid UTF-8 or invalid JSON, with recovery text that repeated the
  redirect. Input hashes still cover the file as written (#15936).
