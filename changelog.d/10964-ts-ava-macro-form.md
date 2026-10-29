<!-- section: Fixed -->
- TypeScript test extraction reads AVA's macro form `test(title, macro, async (t) => {})` (and `ns.macro`): the function after the macro is the callback, so its assertions are no longer dropped (#10964).
