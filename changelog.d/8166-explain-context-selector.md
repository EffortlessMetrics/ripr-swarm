<!-- section: Fixed -->
- `ripr explain` and `ripr context` now refuse a selector that matches
  several findings as ambiguous with the bounded candidate finding IDs
  instead of silently rendering the first match, and a unique canonical
  gap identity (`gap:...`) now resolves its finding (#8166).
