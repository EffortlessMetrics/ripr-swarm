<!-- section: Fixed -->
- Rust finding IDs retain literal whitespace and significant line-comment
  terminators in the hashed expression while normalizing formatting, including
  LF/CRLF comment endings. Raw string, byte and C literals terminate before
  surplus hash tokens, keeping literal data and following code intact.
  Affected old suppression IDs fail closed with visible
  unmatched warnings and require re-review; ambiguous IDs are never retargeted.
  The fingerprint format and historical ID reconstruction remain unchanged (#8832).
