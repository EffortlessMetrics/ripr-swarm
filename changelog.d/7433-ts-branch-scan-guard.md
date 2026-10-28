<!-- section: Fixed -->
- TypeScript branch scan: a one-line changed `if` and a changed `if` inside a nested arrow or `function` no longer read the enclosing block's returns as the branch pair, so the boundary-liveness check gives up instead of calling a dead expectation live (#7433).
