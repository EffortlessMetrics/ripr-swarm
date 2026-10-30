<!-- section: Fixed -->
- TypeScript test extraction no longer drops every test in a file when a string literal inside a test reads like an assignment to the runner (`"?test=1"`); real writes and template expressions are still seen (#10822).
