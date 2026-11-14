<!-- section: Fixed -->
- `ripr agent brief --json` now names a match-arm seam's result: a seam
  whose `expression` is only the arm head (`Size::Medium =>`) gains
  `arm_result` (`450`) when the result sits on the head's line, under the
  same fail-closed rule as the gap row. Seam ids and caches are unchanged
  (#14683).
