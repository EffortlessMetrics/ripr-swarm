<!-- section: Internal / CI -->
- Agent journey byte budget: `cli_smoke` now pins the whole check -> explain -> context -> verify loop (cold vs steady-state check, one explain, one context packet, final `verify=` check) with per-step byte receipts and an exact-duplicate-line redundancy budget, extending the single-surface budgets for agent token efficiency (#13673).
