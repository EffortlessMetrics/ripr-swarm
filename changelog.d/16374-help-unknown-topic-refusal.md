<!-- section: Fixed -->
- `ripr help <topic>` with an unknown topic now refuses as `unknown help topic`
  and names `ripr help` (lists commands) and `ripr help workflow` (lists
  workflows), instead of claiming the topic was an unknown command. Exit code
  2 is unchanged, a typo's suggestion now reads `Did you mean ripr help
  check?`, and the JSON refusal keeps code `unknown_command` with `next`
  pointing at `ripr help` (#16374).
