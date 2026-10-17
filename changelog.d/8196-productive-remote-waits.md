<!-- section: Internal / CI -->
- Agent delivery: goal owners now take one batched remote-state snapshot,
  advance another accepted claim when useful, and yield `GOAL_IN_FLIGHT` when
  all work is remote-owned instead of running minute-scale CI polling loops
  (#8196).
