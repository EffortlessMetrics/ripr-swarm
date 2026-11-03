<!-- section: Fixed -->
- Plain helpers that never invoke a closure parameter no longer lend assertions when the test passes a name bound to a boxed call-making closure or an alias chain (up to four hops) of a named call-making closure (#9997).
