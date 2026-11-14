<!-- section: Fixed -->
- `ripr check --diff` no longer refuses a valid patch as `malformed_diff`
  (`unsupported_input`) when the file ends in blank lines or is
  `git format-patch` output. Text after a hunk whose declared line counts are
  complete (trailing blank lines, the mail signature, the next message's
  headers and diffstat) is no longer read as hunk body, so it adds no findings
  at lines the patch never touched. A `+`, `-` or context line beyond the
  declared counts is still reported as malformed (#14072).
