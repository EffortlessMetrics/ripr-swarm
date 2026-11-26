<!-- section: Fixed -->
- A refused gap-bound `ripr agent repair` start restores the prior workflow
  baseline, findings snapshot and packet through the shared atomic writer, so
  an interrupted restore leaves a complete file instead of a truncated
  baseline (#15444).
