<!-- section: Fixed -->
- Match-arm repair starts now refuse mutable result bindings and calls whose free-function owner cannot be authenticated, including dependency twins and lexical-fallback rivals (#11515).
- `ripr check` withholds a match-arm repair start when incomplete analysis cannot establish the owner across the bounded readiness handoff (#11515).
- The bounded match-arm readiness pass now requires transient owner/caller authority from the producer's complete index, preserving refusals for parsed rivals omitted from that pass (#11515).
