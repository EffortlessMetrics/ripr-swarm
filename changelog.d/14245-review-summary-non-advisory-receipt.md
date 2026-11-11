<!-- section: Fixed -->
- `ripr agent review-summary` no longer reports a review packet as `ready`
  when the agent receipt it reads is `invalid`, `incomplete`, or records no
  `advisory` status. The packet is `incomplete`, and the headline, "what
  changed" and Markdown say the recorded static movement is not review
  evidence (#14245).
