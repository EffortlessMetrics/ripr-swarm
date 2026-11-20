<!-- section: Fixed -->
- Python try-body assertions now respect the first matching exception handler,
  retaining a re-raised assertion before a later swallowing handler. Unknown
  or rebound handler types leave the oracle unresolved (#14885).
