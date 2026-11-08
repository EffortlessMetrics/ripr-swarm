<!-- section: Fixed -->
- TypeScript preview extraction no longer credits `.resolves` / `.rejects`
  matchers returned from callbacks whose value is discarded (for example
  synchronous `forEach` bodies), while runner-returned, awaited, and
  synchronous equality returns in the same shapes stay unchanged (#8933).
