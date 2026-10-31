<!-- section: Added -->
- The DX scoreboard now checks two editor-server failure modes per corpus repo, `trust.editor_resilience_failures` (target 0): a burst of four saves must settle on one succeeded refresh, and a server terminated mid-refresh must not stop the next server from refreshing the same checkout (#11586).
