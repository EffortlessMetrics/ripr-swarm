<!-- section: Fixed -->
- A one-line `if`/`else` tail whose later arm returns the first arm's value no longer reads the first condition as exposed: the choice between equal values is unobservable, so the condition has no returned-value sink (#9843).
