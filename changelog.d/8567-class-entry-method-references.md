<!-- section: Fixed -->
- TypeScript same-class entry graph: record edges for synchronous method references (`this.helper` in array callbacks, or `.call`/`.apply`/immediate `.bind(this)()`), so related tests can reach helpers with weak tier-2 reach instead of `no_static_path` (#8567).
