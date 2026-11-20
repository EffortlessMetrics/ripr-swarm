<!-- section: Changed -->
- `ripr explain --format agent` now prints `owner=` and `before=` lines and omits
  its `next=ripr context` pointer for a test-side gap that names no concrete
  missing discriminator and has at most one related test, because the JSON packet
  adds nothing more there. The `init --agents` loop for such findings drops from
  find, explain and a roughly 1,000-token context packet to find and explain (#14298).
