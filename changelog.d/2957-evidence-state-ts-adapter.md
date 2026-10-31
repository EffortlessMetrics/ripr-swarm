<!-- section: Internal / CI -->
- TypeScript adapter gap states (`static_limitation`, `already_observed`) are produced from the typed `EvidenceState` instead of inline literals; emitted evidence text is unchanged. The adapter-only `advisory` state has no `EvidenceState` variant and is tracked separately (#2957).
