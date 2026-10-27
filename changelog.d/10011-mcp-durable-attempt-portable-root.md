<!-- section: Fixed -->
- Ordinary MCP durable repair-attempt `next_command` displays no longer embed the host-local checkout path: they use the portable root `.`. CLI displays retain the bound checkout root; trust-bound, store-qualified and unparseable continuations keep their persisted spelling (#10009, #10011).
