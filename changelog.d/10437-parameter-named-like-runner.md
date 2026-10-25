<!-- section: Fixed -->
- TypeScript partial-extraction disclosure no longer reports a call through a function parameter named like the runner (`async function run({ test }) { await test(); }`) as a dropped registration. The same body without the parameter is still disclosed. (#10437)
