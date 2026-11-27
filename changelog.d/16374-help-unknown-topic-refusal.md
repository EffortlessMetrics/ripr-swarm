<!-- section: Fixed -->
- `ripr help <topic>` with an unknown topic now refuses as `unknown help topic`
  and names `ripr help` (lists commands) and `ripr help workflow` (lists
  workflows), instead of claiming the topic was an unknown command. Exit code
  2 is unchanged, and a typo keeps its `Did you mean` suggestion (#16374).
