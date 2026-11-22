<!-- section: Fixed -->
- TypeScript and JavaScript: adding or removing a `|| default` or `?? default` fallback (`out[k] = kv[1]` to `kv[1] || ''`) no longer reads `exposed` unless a related test's call is seen to pass the falsy or null input the fallback catches; otherwise it reads `weakly_exposed` and names the missing input (#15285).
