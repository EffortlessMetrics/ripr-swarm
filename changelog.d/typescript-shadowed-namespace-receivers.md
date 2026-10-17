<!-- section: Fixed -->
- TypeScript no longer credits `ns.owner(...)` pins when the test body rebinds the namespace import `ns`. The remaining loose-number, lookup-table, observation, predicate-boundary, spread-override, and dropped-term callers now use the same unshadowed receiver list that #8092 applied to class and match-object pins (#8108).
