<!-- section: Changed -->
- The `ripr init --agents` loop now skips the DETAIL step (`ripr explain`, then `ripr context`) when a gap row's `missing=` is code (for example `total == 100`) rather than `-` or a sentence; on the agent-eval scenarios the loop reads about 70% fewer estimated tokens for the same answers (#14298).
