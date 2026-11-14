<!-- section: Fixed -->
- `ripr check`: an edit wholly inside a `match` scrutinee (`match s.rfind(..)`
  to `match s.find(..)`, arms unchanged) no longer reads as a changed match
  arm with a "strengthen the assertion" gap that asks for an assertion naming
  the arm. It reads like the same expression edit on a plain line, or as the
  comparison's own finding when the scrutinee holds one (#14722).
