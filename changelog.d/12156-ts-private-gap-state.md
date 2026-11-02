<!-- section: Fixed -->
- TypeScript `private_member_reach_unknown` findings now report `gap_state: unknown` (a real `EvidenceState`) instead of the off-vocabulary `advisory`; the specific reason stays in `actionability_category`, and a unit test requires every `gap_state` the TypeScript actionability owner can return to parse as an `EvidenceState` (#12156).
