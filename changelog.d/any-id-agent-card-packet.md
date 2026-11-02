<!-- section: Changed -->
- `ripr agent card` and `ripr agent packet` accept any ID `ripr check` or `ripr pilot` prints (a `probe:` finding ID, a `gap:` ID or a `file:line`) through the same resolver `ripr agent repair` uses, and their missing-ID hint says so. Ambiguous, ineligible and stale IDs refuse with this command's own retry; exact seam IDs behave as before (#12420).
