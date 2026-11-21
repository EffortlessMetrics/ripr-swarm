<!-- section: Fixed -->
- TypeScript exposure no longer credits `exposed` when a change differs only for an input class (falsy, omitted-field, or whitespace-padded) that no related test supplies; such gaps now read `weakly_exposed` (#15285).
