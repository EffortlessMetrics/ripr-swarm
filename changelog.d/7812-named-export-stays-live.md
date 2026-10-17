<!-- section: Fixed -->
- A TypeScript binding that is both snapshotted by `export default name` and exported by name (`export let`, `export { name }`, `export { name as alias }`) no longer keeps its pre-reassignment owner, because named importers observe the later write (#7812).
