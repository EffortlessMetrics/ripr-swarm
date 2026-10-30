<!-- section: Internal / CI -->
- Move the existing serde_json registry receipt reader into its own internal
  module, preserving provider identity, currentness checks, and refusal behavior
  before adding another registry-backed assertion profile (#10868).
