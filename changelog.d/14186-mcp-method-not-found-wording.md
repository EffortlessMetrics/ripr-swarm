<!-- section: Fixed -->
- `ripr mcp` answers every method it does not serve with
  `Method not found: <method>`. Standard methods such as `prompts/get`,
  `logging/setLevel` and `resources/subscribe` used to return `-32601` with
  only the bare method name as the message (#14186).
