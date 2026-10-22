<!-- section: Fixed -->
- `ripr mcp --stdio` answers a request whose `params` has the wrong shape (for example `[]`) with `Invalid request` carrying that request's own id instead of null, so clients can match the reply; frames with no readable request id keep the explicit null (#8725).
