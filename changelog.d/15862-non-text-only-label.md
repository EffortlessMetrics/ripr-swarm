<!-- section: Fixed -->
- A diff with only binary or file-mode changes now reads "the diff has only
  binary or file-mode changes" in the human report, editor sentence and task
  summary. It no longer reads "the diff could not be parsed" beside a detail
  that says the diff is well formed. The JSON `malformed_diff` token is
  unchanged (#15862).
