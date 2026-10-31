<!-- section: Internal / CI -->
- LSP gap-state consumers (actions, diagnostic budget, repair packet) now decide actionability through the typed `EvidenceState` instead of inline "actionable" string comparisons; wire values and behavior are unchanged (#2956).
