<!-- section: Fixed -->
- An offline partial clone (`--filter=blob:none`) no longer reports a "damaged object store": ripr names the partial clone and the unreachable remote, and its Git calls no longer prompt on the terminal or open a Git Credential Manager window. A new test runs everyday commands with the network sealed (#14799).
