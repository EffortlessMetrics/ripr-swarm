<!-- section: Fixed -->
- Every `Wrote <path>` confirmation now renders through the shared
  `output::path::human_path` helper, so paths print with forward slashes and
  without verbatim `\\?\` prefixes on every command and platform (#9806).
