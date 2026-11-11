<!-- section: Fixed -->
- `ripr pr-evidence` publishes its JSON packet, Markdown panel and diff
  through the shared atomic writer, so an interrupted or failed run leaves
  the previous complete file instead of a truncated one (#14274).
