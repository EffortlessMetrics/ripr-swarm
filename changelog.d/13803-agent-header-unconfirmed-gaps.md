<!-- section: Changed -->
- `ripr check --format agent`: when the only listed gap is a preview-language (Python/TypeScript) or reach-limited one, the header adds `unconfirmed_gaps=N` beside `gaps=0`, so it no longer reads as "no gaps" next to a listed gap and its `open=` line. `gaps=` still counts confirmed gaps only (#13803).
