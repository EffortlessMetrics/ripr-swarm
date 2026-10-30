<!-- section: Fixed -->
- The LSP server now distinguishes a rejected `riprAgent` client advertisement from one the client never sent: a typed rejection (kind plus bounded detail) is logged once per session and projected in the bounded status payload, so an unsupported protocol version or malformed block is diagnosable instead of silently reading as "agent support absent" (#9604).
