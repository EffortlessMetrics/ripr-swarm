<!-- section: Fixed -->
- A predicate finding's `Changed` block now shows `before:` and `after:` over
  the same span, for example `string.len() > MAX` above
  `string.len() >= MAX`. Before, `before:` showed the whole old line
  (`if string.len() > MAX {`), so a one-operator change looked larger than it
  was. The MCP `changed_behavior.before` field and the editor hover show the
  same span. Match arms still show the whole old arm (#6995).
