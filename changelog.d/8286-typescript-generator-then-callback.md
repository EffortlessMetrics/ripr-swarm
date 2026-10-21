<!-- section: Fixed -->
- TypeScript no longer credits assertions inside a returned or awaited `.then(function* () { ... })` or `async function*` callback. Invoking a generator only creates an iterator, so its assertions never run; ordinary and async `.then` callbacks keep their credit (#8286).
