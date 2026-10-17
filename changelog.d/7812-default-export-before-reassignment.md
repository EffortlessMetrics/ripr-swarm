<!-- section: Fixed -->
- TypeScript now keeps a default-exported `let`/function owner when `export default name` snapshots the binding before a later reassignment, so a change in the captured body still produces a finding. Reassignment before that export stays fail-closed (#7812).
