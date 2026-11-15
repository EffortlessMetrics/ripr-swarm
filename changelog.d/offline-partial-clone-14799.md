<!-- section: Fixed -->
- An offline partial clone (`--filter=blob:none`) no longer reports a "damaged object store": ripr names the partial clone and the unreachable remote, and its Git calls never wait on a credential prompt. A new test runs everyday commands with the network sealed (#14799).
