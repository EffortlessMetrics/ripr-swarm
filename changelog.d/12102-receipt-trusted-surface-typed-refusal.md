<!-- section: Fixed -->
- `ripr agent repair --phase after` and `ripr agent receipt` keep every
  refusal message byte-identical, but the receipt's trusted-surface refusal
  now carries its class as a typed `ReceiptBindingError` at the producer:
  which attempt record the refusal narrates into, whether the focused
  evaluation restores, and the decision exit code all read the type instead
  of matching message text, so rewording a refusal message can no longer
  flip the exit an agent branches on or misplace the durable refusal record
  (#12102, #14014).
