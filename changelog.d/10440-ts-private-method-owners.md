<!-- section: Fixed -->
- A change inside a TypeScript `#private` class method is now a `static_unknown` finding instead of a silent `typescript_owner_extraction_partial` gap. No test can call a private member by name and reach through a public caller is not modeled, so it is never `no_static_path` and earns no credit (#10440).
