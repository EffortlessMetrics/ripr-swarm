<!-- section: Fixed -->
- Agent loop status: `ripr agent status --json` (and the review-summary JSON
  surface) now mark `missing_commands[]` and `next_command` entries whose
  command still embeds the unresolved `<seam-id>` placeholder with
  `requires_substitution: true`, so a machine consumer can filter out
  non-executable template commands mechanically instead of executing them
  verbatim and eating a `seam_id_unknown` refusal per entry. Ready-to-run
  entries carry no marker, the command strings and the plain-text surface are
  unchanged, and the additive field does not bump the status schema version
  (#13244).
