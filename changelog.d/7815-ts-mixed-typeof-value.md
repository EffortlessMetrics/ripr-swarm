<!-- section: Fixed -->
- TypeScript no longer drops a mixed assertion that both type-checks an owner and later observes its value (`typeof price === 'function' ? price(150) : 0`). Only the `typeof`-consumed occurrence is excluded; a genuine later value check can still witness the changed sink. Pure `typeof price` observations stay non-value (#7815).
