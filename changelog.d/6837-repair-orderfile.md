<!-- section: Fixed -->
- Repair attempts no longer abort when the user's `diff.orderFile` names a
  missing file. Git currentness and path inventories neutralize that setting,
  including after a focused test edit is committed; dirty production input
  still receives the existing actionable refusal (#6837).
