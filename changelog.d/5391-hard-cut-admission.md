<!-- section: Internal / CI -->
- Add an inactive-by-default 0.11 hard-cut packet to the existing pre-qualification candidate check. After the selected merge set lands, one reviewed packet-only control commit freezes ordinary qualification against the exact product cut until #1609 pins it; packet-only state updates can reopen or release rolling development without creating a merge queue (#5391).
