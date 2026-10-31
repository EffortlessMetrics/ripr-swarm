<!-- section: Fixed -->
- A manual gap-bound `ripr agent repair --phase before` start now refuses, before writing the before-snapshot or starting an attempt, when the fresh analysis is incomplete; the refusal names the limitation and keeps the quoted root and base in its `ripr check` recovery command (#11403).
